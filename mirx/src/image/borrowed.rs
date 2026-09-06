use core::iter::FusedIterator;

mod rows;
mod transfer;
pub use rows::{PlaneAccessError, PlaneRows};
pub use transfer::SurfaceCopyError;

use super::{
    AccessCapabilities, ColorDescription, ImageEncodeError, PlaneMemoryLayout, RawImageAsset,
    RawImageView, SampleLayout, SurfaceDescriptor, SurfaceMemoryPlan, SurfacePlane,
};
use crate::{ColorFormat, ColorTableView, ImageView};

// All decodable sample layouts have at most three planes. Unknown layouts
// remain opaque and cannot construct a SurfaceDescriptor.
const MAX_PLANES: usize = 3;

/// Validated decoded pixels independent of their containing wire layout.
///
/// Plane descriptors are stored inline. Pixel bytes and indexed color tables
/// remain borrowed, including when the planes occupy separate allocations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceView<'a> {
    surface: SurfaceDescriptor,
    planes: [Option<SurfacePlane<'a>>; MAX_PLANES],
    color_table: Option<ColorTableView<'a>>,
}

impl<'a> SurfaceView<'a> {
    /// Reports direct sample access available from this validated surface.
    pub fn access_capabilities(self) -> AccessCapabilities {
        let has_linear_rows = self
            .planes()
            .all(|plane| plane.memory().flags().bits() == 0);
        AccessCapabilities::raw_surface(has_linear_rows)
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        self.color_table
    }

    pub const fn plane_count(self) -> u8 {
        self.surface.plane_count()
    }

    pub fn plane(self, index: u8) -> Option<SurfacePlane<'a>> {
        self.planes.get(usize::from(index)).copied().flatten()
    }

    pub fn planes(self) -> SurfacePlanes<'a> {
        SurfacePlanes {
            remaining: self.planes.into_iter(),
            len: usize::from(self.plane_count()),
        }
    }

    /// Checks the actual plane addresses against their declared alignments.
    pub fn data_addresses_are_aligned(self) -> bool {
        self.planes().all(SurfacePlane::address_is_aligned)
    }

    /// Borrows already validated output and its matching indexed color table.
    pub(crate) fn from_plan(
        plan: SurfaceMemoryPlan,
        bytes: &'a [u8],
        color_table: Option<ColorTableView<'a>>,
    ) -> Self {
        Self {
            surface: plan.surface(),
            planes: core::array::from_fn(|index| {
                plan.plane(index as u8).map(|memory| SurfacePlane {
                    geometry: plan
                        .surface()
                        .plane(index as u8)
                        .expect("planned surface plane"),
                    memory,
                    bytes: memory.bytes(bytes).expect("validated output plane range"),
                })
            }),
            color_table,
        }
    }

    pub(super) fn from_asset(asset: RawImageAsset<'_, 'a>) -> Self {
        let surface = asset.surface();
        let mut planes = [None; MAX_PLANES];
        let mut offset = 0;
        for (index, slot) in planes
            .iter_mut()
            .enumerate()
            .take(usize::from(surface.plane_count()))
        {
            let geometry = surface.plane(index as u8).expect("validated plane index");
            let memory = match asset.memory_layouts() {
                Some(memory) => memory[index],
                None => PlaneMemoryLayout::builder(geometry)
                    .with_data_offset(offset)
                    .build()
                    .expect("validated canonical plane"),
            };
            *slot = Some(SurfacePlane {
                geometry,
                memory,
                bytes: asset.planes()[index],
            });
            offset = memory.data_end();
        }
        Self {
            surface,
            planes,
            color_table: asset
                .color_table()
                .and_then(ColorTableView::from_rgba_bytes),
        }
    }
}

impl<'a> RawImageView<'a> {
    /// Borrows decoded planes without retaining a dependency on wire offsets.
    pub fn view(self) -> SurfaceView<'a> {
        SurfaceView {
            surface: self.surface(),
            planes: core::array::from_fn(|index| self.plane(index as u8)),
            color_table: self.color_table(),
        }
    }
}

impl<'a> ImageView<'a> {
    /// Exposes packed FLAT or atlas pixels through the common surface model.
    pub fn surface(self) -> Result<SurfaceView<'a>, ImageEncodeError> {
        let layout = SampleLayout::from_color_format(self.format());
        let color = if layout.is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        let surface = SurfaceDescriptor::new(self.width(), self.height(), layout, color)
            .expect("validated packed image surface");
        let main = PlaneMemoryLayout::builder(surface.plane(0).expect("main plane"))
            .with_stride(self.stride())
            .build()
            .map_err(|error| ImageEncodeError::InvalidPlaneLayout { index: 0, error })?;
        let planes = [self.main(), self.extra().unwrap_or(&[])];
        let mut memory = [main; 2];
        if self.format() == ColorFormat::RGB565A8 {
            memory[1] = PlaneMemoryLayout::builder(surface.plane(1).expect("alpha plane"))
                .with_data_offset(main.data_end())
                .build()
                .map_err(|error| ImageEncodeError::InvalidPlaneLayout { index: 1, error })?;
        }
        let count = usize::from(surface.plane_count());
        let mut asset =
            RawImageAsset::new(surface, &planes[..count]).with_memory_layouts(&memory[..count]);
        if let Some(table) = self.inline_palette() {
            asset = asset.with_color_table(table.as_bytes());
        }
        // A FLAT image can have a larger combined extent than one IMAGE DATA
        // range. Validate that conversion before constructing the surface.
        asset.view()
    }
}

/// Exact-size iteration over decoded planes in sample-layout order.
#[derive(Clone, Debug)]
pub struct SurfacePlanes<'a> {
    remaining: core::array::IntoIter<Option<SurfacePlane<'a>>, MAX_PLANES>,
    len: usize,
}

impl<'a> Iterator for SurfacePlanes<'a> {
    type Item = SurfacePlane<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let plane = self.remaining.find_map(core::convert::identity)?;
        self.len -= 1;
        Some(plane)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len, Some(self.len))
    }
}

impl DoubleEndedIterator for SurfacePlanes<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let plane = self
            .remaining
            .by_ref()
            .rev()
            .find_map(core::convert::identity)?;
        self.len -= 1;
        Some(plane)
    }
}

impl ExactSizeIterator for SurfacePlanes<'_> {
    fn len(&self) -> usize {
        self.len
    }
}

impl FusedIterator for SurfacePlanes<'_> {}

/// Borrowed decoded pixels supplied to checked IMAGE authoring operations.
///
/// Implementations return validated views without transferring pixel ownership.
pub trait ImageSource {
    fn view(&self) -> Result<SurfaceView<'_>, crate::ImagePayloadError>;
}

impl ImageSource for SurfaceView<'_> {
    fn view(&self) -> Result<SurfaceView<'_>, crate::ImagePayloadError> {
        Ok(*self)
    }
}

impl ImageSource for RawImageAsset<'_, '_> {
    fn view(&self) -> Result<SurfaceView<'_>, crate::ImagePayloadError> {
        RawImageAsset::view(*self).map_err(crate::ImagePayloadError::Surface)
    }
}

impl ImageSource for RawImageView<'_> {
    fn view(&self) -> Result<SurfaceView<'_>, crate::ImagePayloadError> {
        Ok(RawImageView::view(*self))
    }
}

impl<'a> SurfaceView<'a> {
    /// Projects a surface into the packed FLAT/atlas model without dropping
    /// color, geometry, or physical storage requirements.
    pub fn packed(self) -> Option<ImageView<'a>> {
        let surface = self.surface();
        let format = surface.sample_layout().color_format()?;
        let expected_color = if surface.sample_layout().is_alpha() {
            ColorDescription::NONE
        } else {
            ColorDescription::SRGB
        };
        if surface.color() != expected_color
            || surface.flags().bits() != 0
            || surface.pixel_aspect() != (1, 1)
        {
            return None;
        }
        for plane in self.planes() {
            let memory = plane.memory();
            if memory.allocation_width() != plane.geometry().width()
                || memory.allocation_height() != plane.geometry().height()
                || memory.required_alignment() != crate::ByteAlignment::ONE
                || memory.flags().bits() != 0
            {
                return None;
            }
        }
        let main = self.plane(0)?;
        let extra = if format == ColorFormat::RGB565A8 {
            let alpha = self.plane(1)?;
            if alpha.memory().stride() != surface.width() {
                return None;
            }
            (!alpha.bytes().is_empty()).then_some(alpha.bytes())
        } else {
            self.color_table().map(|table| table.as_bytes())
        };
        Some(ImageView::from_validated_planes(
            crate::payload::image::ImageMeta {
                width: surface.width(),
                height: surface.height(),
                stride: main.memory().stride(),
                format,
            },
            main.bytes(),
            extra,
        ))
    }
}

impl<'a> RawImageView<'a> {
    /// Projects a packed surface only when FLAT can retain its storage contract.
    pub fn packed(self) -> Option<ImageView<'a>> {
        use crate::media::{MediaSectionFlags, MediaSectionKind};
        let media = self.media();
        if media.header().flags().unknown_bits() != 0
            || media.sections().any(|section| {
                let descriptor = section.descriptor();
                descriptor.flags() != MediaSectionFlags::REQUIRED
                    || !matches!(
                        descriptor.kind(),
                        MediaSectionKind::SURFACE
                            | MediaSectionKind::PLANES
                            | MediaSectionKind::COLOR_TABLE
                            | MediaSectionKind::DATA
                            | MediaSectionKind::INTEGRITY
                    )
            })
        {
            return None;
        }
        self.view().packed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::image::ImageMeta;

    #[test]
    fn packed_projection_keeps_envelope_promises_out_of_flat_demotion() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let payload = RawImageAsset::new(surface, &[&[7]]).encode().unwrap();
        assert!(RawImageView::open(&payload).unwrap().packed().is_some());
        for (offset, value) in [(1, 0x80), (crate::media::MEDIA_HEADER_LEN + 2, 0x81)] {
            let mut changed = payload.clone();
            changed[offset] = value;
            super::super::test_support::refresh_crc(&mut changed);
            let raw = RawImageView::open(&changed).unwrap();
            assert_eq!(raw.view().plane(0).unwrap().bytes(), &[7]);
            assert!(raw.packed().is_none());
            let mut document = crate::Document::new();
            let id = document
                .push_raw(crate::RawChunkInput::new(crate::ChunkType::IMAGE, changed))
                .unwrap();
            document.set_primary(id).unwrap();
            assert_eq!(
                document.demote_to_flat(),
                Err(crate::EditError::NotRepresentableAsFlat)
            );
        }
    }

    #[test]
    fn coded_sections_stay_opaque_to_raw_access() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let mut payload = RawImageAsset::new(surface, &[&[7]]).encode().unwrap();
        payload[crate::media::MEDIA_HEADER_LEN] =
            crate::media::MediaSectionKind::CODINGS.raw() as u8;
        super::super::test_support::refresh_crc(&mut payload);
        assert!(crate::media::MediaPayload::open(&payload).is_ok());
        assert_eq!(
            RawImageView::open(&payload),
            Err(super::super::RawImageViewError::UnexpectedSection(
                crate::media::MediaSectionKind::CODINGS
            ))
        );
    }

    #[test]
    fn decoded_view_outlives_temporary_metadata_without_copying_pixels() {
        let y = [1; 15];
        let uv = [2; 12];
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let view = {
            let planes: &[&[u8]] = &[&y, &uv];
            RawImageAsset::new(surface, planes).view().unwrap()
        };
        assert_eq!(view.plane(0).unwrap().bytes().as_ptr(), y.as_ptr());
        assert_eq!(view.plane(1).unwrap().bytes().as_ptr(), uv.as_ptr());
        let mut planes = view.planes();
        assert_eq!(planes.len(), 2);
        assert_eq!(planes.next_back().unwrap().bytes(), uv);
        assert_eq!(planes.next().unwrap().bytes(), y);
        assert_eq!(planes.len(), 0);
        assert_eq!(planes.next_back(), None);
        assert_eq!(planes.next(), None);
    }

    #[test]
    fn packed_alpha_and_palette_are_distinct_surface_components() {
        for format in [ColorFormat::RGB565A8, ColorFormat::I1] {
            let stride = format.minimum_stride(3).unwrap() + 2;
            let main = alloc::vec![0; (stride * 2) as usize];
            let extra = alloc::vec![0; format.extra_size(3, 2, stride).unwrap() as usize];
            let packed = ImageView::from_validated_planes(
                ImageMeta {
                    width: 3,
                    height: 2,
                    stride,
                    format,
                },
                &main,
                Some(&extra),
            );
            let view = packed.surface().unwrap();
            assert_eq!(view.plane(0).unwrap().memory().stride(), stride);
            assert_eq!(view.plane(0).unwrap().bytes().as_ptr(), main.as_ptr());
            if format == ColorFormat::RGB565A8 {
                assert_eq!(view.plane_count(), 2);
                assert_eq!(view.plane(1).unwrap().memory().stride(), 3);
                assert_eq!(view.plane(1).unwrap().bytes().as_ptr(), extra.as_ptr());
                assert!(view.color_table().is_none());
            } else {
                assert_eq!(view.plane_count(), 1);
                assert_eq!(
                    view.color_table().unwrap().as_bytes().as_ptr(),
                    extra.as_ptr()
                );
            }
        }
    }

    #[test]
    fn every_known_layout_fits_inline_plane_metadata() {
        for id in 0..=u16::MAX {
            let layout = SampleLayout::new(id);
            if let Some(count) = layout.plane_count() {
                assert!(usize::from(count) <= MAX_PLANES);
            }
        }
    }
}
