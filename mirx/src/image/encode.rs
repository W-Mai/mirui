use alloc::vec::Vec;

use super::{
    PLANE_RECORD_LEN, PlaneMemoryError, PlaneMemoryLayout, SURFACE_RECORD_LEN, SurfaceDescriptor,
    SurfaceView,
};
use super::{color_table::ColorTableError, output::PayloadOutput};
use crate::media::{
    MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaSectionKind,
};
use crate::wire::write_u16_le;

/// Borrowed RAW IMAGE authoring input.
///
/// The outer plane slice and every plane byte slice remain caller-owned. Tight
/// canonical layouts need only those bytes; padded or aligned surfaces attach
/// one checked [`PlaneMemoryLayout`] per derived plane.
#[derive(Clone, Copy, Debug)]
pub struct RawImageAsset<'planes, 'data> {
    surface: SurfaceDescriptor,
    planes: &'planes [&'data [u8]],
    memory: Option<&'planes [PlaneMemoryLayout]>,
    color_table: Option<&'data [u8]>,
}

impl<'planes, 'data> RawImageAsset<'planes, 'data> {
    pub const fn new(surface: SurfaceDescriptor, planes: &'planes [&'data [u8]]) -> Self {
        Self {
            surface,
            planes,
            memory: None,
            color_table: None,
        }
    }

    pub const fn with_memory_layouts(mut self, memory: &'planes [PlaneMemoryLayout]) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Attaches straight-alpha RGBA entries for an indexed sample layout.
    pub const fn with_color_table(mut self, rgba: &'data [u8]) -> Self {
        self.color_table = Some(rgba);
        self
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub const fn planes(self) -> &'planes [&'data [u8]] {
        self.planes
    }
    pub const fn memory_layouts(self) -> Option<&'planes [PlaneMemoryLayout]> {
        self.memory
    }
    pub const fn color_table(self) -> Option<&'data [u8]> {
        self.color_table
    }

    /// Validates and borrows the decoded surface without encoding it.
    /// The result borrows plane bytes, not the temporary plane-reference array.
    pub fn view(self) -> Result<SurfaceView<'data>, ImageEncodeError> {
        let expected = usize::from(self.surface.plane_count());
        if self.planes.len() != expected {
            return Err(ImageEncodeError::PlaneCountMismatch {
                expected,
                actual: self.planes.len(),
            });
        }
        if let Some(memory) = self.memory {
            if memory.len() != expected {
                return Err(ImageEncodeError::MemoryLayoutCountMismatch {
                    expected,
                    actual: memory.len(),
                });
            }
        }
        self.surface
            .read_color_table(self.color_table)
            .map_err(ImageEncodeError::from)?;
        validate_planes(self)?;
        Ok(SurfaceView::from_asset(self))
    }

    /// Returns the exact canonical payload length after complete validation.
    pub fn encoded_len(self) -> Result<usize, ImageEncodeError> {
        self.view()?.encoded_len()
    }

    /// Validates before writing and preserves any unused output suffix.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, ImageEncodeError> {
        self.view()?.encode_into(out)
    }

    /// Allocates one exact-length canonical payload.
    pub fn encode(self) -> Result<Vec<u8>, ImageEncodeError> {
        self.view()?.encode()
    }
}

impl SurfaceView<'_> {
    /// Returns the exact canonical RAW IMAGE payload length.
    pub fn encoded_len(self) -> Result<usize, ImageEncodeError> {
        Ok(RawImagePlan::new(self)?.payload_len)
    }

    /// Encodes a canonical RAW IMAGE into caller-owned storage.
    /// All validation completes before writing; the unused suffix is preserved.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, ImageEncodeError> {
        let plan = RawImagePlan::new(self)?;
        if out.len() < plan.payload_len {
            return Err(ImageEncodeError::BufferTooSmall {
                needed: plan.payload_len,
                available: out.len(),
            });
        }
        plan.emit(PayloadOutput::buffer(&mut out[..plan.payload_len]));
        Ok(plan.payload_len)
    }

    /// Allocates one exact-length canonical RAW IMAGE payload.
    pub fn encode(self) -> Result<Vec<u8>, ImageEncodeError> {
        let plan = RawImagePlan::new(self)?;
        let mut out = Vec::new();
        out.try_reserve_exact(plan.payload_len)
            .map_err(|_| ImageEncodeError::AllocationFailed)?;
        out.resize(plan.payload_len, 0);
        plan.emit(PayloadOutput::buffer(&mut out));
        Ok(out)
    }

    /// Compares exact canonical RAW IMAGE bytes without allocating or writing.
    ///
    /// Semantically equivalent but noncanonical records or padding do not
    /// match. CRC and every payload byte participate in the comparison.
    pub fn matches_payload(self, payload: &[u8]) -> Result<bool, ImageEncodeError> {
        let plan = RawImagePlan::new(self)?;
        Ok(payload.len() == plan.payload_len && plan.emit(PayloadOutput::comparison(payload)))
    }
}

#[derive(Clone, Copy, Debug)]
struct RawImagePlan<'a> {
    view: SurfaceView<'a>,
    section_count: u16,
    planes_offset: Option<usize>,
    color_table_offset: Option<usize>,
    data_offset: usize,
    data_len: u32,
    payload_len: usize,
}

impl<'a> RawImagePlan<'a> {
    fn new(view: SurfaceView<'a>) -> Result<Self, ImageEncodeError> {
        let mut has_planes = false;
        let mut canonical_offset = 0;
        let mut data_len = 0;
        let mut required_alignment_log2 = 0;
        for plane in view.planes() {
            let canonical = PlaneMemoryLayout::builder(plane.geometry())
                .with_data_offset(canonical_offset)
                .build()
                .map_err(|_| ImageEncodeError::SizeOverflow)?;
            has_planes |= plane.memory() != canonical;
            canonical_offset = canonical.data_end();
            data_len = plane.memory().data_end();
            required_alignment_log2 = required_alignment_log2
                .max(plane.memory().required_alignment().trailing_zeros() as u8);
        }
        let color_table_len = view.color_table().map_or(0, |table| table.as_bytes().len());
        let section_count = 2 + u16::from(has_planes) + u16::from(color_table_len != 0);
        let mut cursor =
            MEDIA_HEADER_LEN + usize::from(section_count) * MEDIA_SECTION_LEN + SURFACE_RECORD_LEN;
        let planes_offset = has_planes.then_some(cursor);
        if has_planes {
            cursor += usize::from(view.plane_count()) * PLANE_RECORD_LEN;
        }
        let color_table_offset = (color_table_len != 0).then_some(cursor);
        cursor += color_table_len;
        let alignment = 1usize << required_alignment_log2;
        let data_offset = cursor
            .checked_add(alignment - 1)
            .map(|value| value & !(alignment - 1))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let payload_len = data_offset
            .checked_add(usize::try_from(data_len).map_err(|_| ImageEncodeError::SizeOverflow)?)
            .and_then(|end| end.checked_add(MEDIA_CRC_LEN))
            .ok_or(ImageEncodeError::SizeOverflow)?;
        u32::try_from(payload_len).map_err(|_| ImageEncodeError::SizeOverflow)?;
        Ok(Self {
            view,
            section_count,
            planes_offset,
            color_table_offset,
            data_offset,
            data_len,
            payload_len,
        })
    }

    fn emit(self, mut out: PayloadOutput<'_>) -> bool {
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        write_u16_le(&mut header, 2, self.section_count);
        out.header(&header);

        let surface_offset = MEDIA_HEADER_LEN + usize::from(self.section_count) * MEDIA_SECTION_LEN;
        out.section(
            MediaSectionKind::SURFACE,
            surface_offset,
            SURFACE_RECORD_LEN,
        );
        if let Some(offset) = self.planes_offset {
            out.section(
                MediaSectionKind::PLANES,
                offset,
                usize::from(self.view.plane_count()) * PLANE_RECORD_LEN,
            );
        }
        if let Some(offset) = self.color_table_offset {
            out.section(
                MediaSectionKind::COLOR_TABLE,
                offset,
                self.view
                    .color_table()
                    .expect("planned color table")
                    .as_bytes()
                    .len(),
            );
        }
        out.section(
            MediaSectionKind::DATA,
            self.data_offset,
            self.data_len as usize,
        );

        let mut surface = [0; SURFACE_RECORD_LEN];
        self.view
            .surface()
            .encode_record_into(&mut surface)
            .expect("exact surface record");
        out.write(&surface);
        if self.planes_offset.is_some() {
            for plane in self.view.planes() {
                let mut record = [0; PLANE_RECORD_LEN];
                plane
                    .memory()
                    .encode_record_into(&mut record)
                    .expect("exact plane record");
                out.write(&record);
            }
        }
        if let Some(table) = self.view.color_table() {
            out.write(table.as_bytes());
        }
        out.pad_to(self.data_offset);
        out.begin_data();
        for plane in self.view.planes() {
            out.pad_to(self.data_offset + plane.memory().data_offset() as usize);
            out.write(plane.bytes());
        }
        debug_assert_eq!(out.position() + MEDIA_CRC_LEN, self.payload_len);
        out.finish()
    }
}

/// Failure while validating or encoding an IMAGE asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageEncodeError {
    Preflight(super::EncodedImageError),
    UnexpectedCoding(crate::media::CodingId),
    /// Explicit groups cannot be combined with an asset-level alignment override.
    ConflictingAlignment,
    Codings(crate::media::CodingTableError),
    Group(super::UnitGroupError),
    PlaneCountMismatch {
        expected: usize,
        actual: usize,
    },
    MemoryLayoutCountMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidPlaneLayout {
        index: u8,
        error: PlaneMemoryError,
    },
    PlaneLengthMismatch {
        index: u8,
        expected: usize,
        actual: usize,
    },
    PlaneRangesOverlap {
        previous: u8,
        next: u8,
    },
    MissingColorTable,
    UnexpectedColorTable,
    ColorTableLengthMismatch {
        expected: usize,
        actual: usize,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
    AllocationFailed,
}

impl From<ColorTableError> for ImageEncodeError {
    fn from(error: ColorTableError) -> Self {
        match error {
            ColorTableError::Missing => Self::MissingColorTable,
            ColorTableError::Unexpected => Self::UnexpectedColorTable,
            ColorTableError::SizeMismatch { expected, actual } => {
                Self::ColorTableLengthMismatch { expected, actual }
            }
            ColorTableError::SizeOverflow => Self::SizeOverflow,
        }
    }
}

fn validate_planes(asset: RawImageAsset<'_, '_>) -> Result<(u32, u8), ImageEncodeError> {
    let mut previous_end = 0;
    let mut alignment_log2 = 0;
    for (index, bytes) in asset.planes.iter().enumerate() {
        let index = u8::try_from(index).map_err(|_| ImageEncodeError::SizeOverflow)?;
        let geometry = asset
            .surface
            .plane(index)
            .ok_or(ImageEncodeError::SizeOverflow)?;
        let memory = match asset.memory {
            Some(memory) => {
                let layout = memory[usize::from(index)];
                layout
                    .validate_for(geometry)
                    .map_err(|error| ImageEncodeError::InvalidPlaneLayout { index, error })?;
                layout
            }
            None => PlaneMemoryLayout::builder(geometry)
                .with_data_offset(previous_end)
                .build()
                .map_err(|error| ImageEncodeError::InvalidPlaneLayout { index, error })?,
        };

        let expected =
            usize::try_from(memory.byte_len()).map_err(|_| ImageEncodeError::SizeOverflow)?;
        if bytes.len() != expected {
            return Err(ImageEncodeError::PlaneLengthMismatch {
                index,
                expected,
                actual: bytes.len(),
            });
        }
        if memory.data_offset() < previous_end {
            return Err(ImageEncodeError::PlaneRangesOverlap {
                previous: index - 1,
                next: index,
            });
        }
        previous_end = memory.data_end();
        alignment_log2 = alignment_log2.max(memory.required_alignment().trailing_zeros() as u8);
    }
    Ok((previous_end, alignment_log2))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::image::{ColorDescription, RawImageView, SampleLayout};

    #[test]
    fn canonical_encoding_omits_explicit_default_plane_records() {
        let surface =
            SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let pixels = [1, 2, 3, 4];
        let planes: &[&[u8]] = &[&pixels];
        let layouts = [PlaneMemoryLayout::tight(surface.plane(0).unwrap()).unwrap()];
        let tight = RawImageAsset::new(surface, planes).encode().unwrap();
        let explicit = RawImageAsset::new(surface, planes)
            .with_memory_layouts(&layouts)
            .encode()
            .unwrap();
        assert_eq!(tight, explicit);
        assert!(
            RawImageView::open(&explicit)
                .unwrap()
                .media()
                .section(MediaSectionKind::PLANES)
                .is_none()
        );
    }

    #[test]
    fn comparison_and_encoding_share_every_canonical_byte() {
        let surface =
            SurfaceDescriptor::new(3, 2, SampleLayout::I4, ColorDescription::SRGB).unwrap();
        let pixels = [0x12; 8];
        let planes: &[&[u8]] = &[&pixels];
        let layouts = [PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_stride(4)
            .with_data_offset(64)
            .with_alignment(64)
            .build()
            .unwrap()];
        let palette = [0x5a; 64];
        let view = RawImageAsset::new(surface, planes)
            .with_memory_layouts(&layouts)
            .with_color_table(&palette)
            .view()
            .unwrap();
        let mut encoded = view.encode().unwrap();
        assert!(view.matches_payload(&encoded).unwrap());
        for index in 0..encoded.len() {
            encoded[index] ^= 1;
            assert!(!view.matches_payload(&encoded).unwrap(), "byte {index}");
            encoded[index] ^= 1;
        }
        assert!(!view.matches_payload(&encoded[..encoded.len() - 1]).unwrap());
        encoded.push(0);
        assert!(!view.matches_payload(&encoded).unwrap());
    }

    #[test]
    fn tight_nv12_round_trips_without_a_planes_section() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let y = [0x10; 15];
        let uv = [0x80; 12];
        let planes: &[&[u8]] = &[&y, &uv];
        let asset = RawImageAsset::new(surface, planes);
        let bytes = asset.encode().unwrap();
        assert_eq!(asset.encoded_len(), Ok(bytes.len()));

        let image = RawImageView::open(&bytes).unwrap();
        assert!(image.media().section(MediaSectionKind::PLANES).is_none());
        assert_eq!(image.plane(0).unwrap().bytes(), y);
        assert_eq!(image.plane(1).unwrap().bytes(), uv);
    }

    #[test]
    fn indexed_asset_round_trips_its_separate_color_table() {
        let surface =
            SurfaceDescriptor::new(3, 2, SampleLayout::I4, ColorDescription::SRGB).unwrap();
        let indices = [0x12, 0x30, 0x45, 0x60];
        let planes: &[&[u8]] = &[&indices];
        let colors = [0xa5; 64];
        let bytes = RawImageAsset::new(surface, planes)
            .with_color_table(&colors)
            .encode()
            .unwrap();
        let image = RawImageView::open(&bytes).unwrap();
        assert_eq!(image.color_table().unwrap().as_bytes(), colors);
        assert_eq!(image.plane(0).unwrap().bytes(), indices);
    }

    #[test]
    fn explicit_padding_is_zeroed_and_plane_records_round_trip() {
        let surface =
            SurfaceDescriptor::new(3, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let plane = surface.plane(0).unwrap();
        let memory = PlaneMemoryLayout::builder(plane)
            .with_allocation_extent(4, 2)
            .with_stride(16)
            .with_data_offset(64)
            .with_alignment(64)
            .build()
            .unwrap();
        let pixels = [0x5a; 32];
        let planes: &[&[u8]] = &[&pixels];
        let memory_layouts = [memory];
        let bytes = RawImageAsset::new(surface, planes)
            .with_memory_layouts(&memory_layouts)
            .encode()
            .unwrap();
        let image = RawImageView::open(&bytes).unwrap();
        assert!(image.media().section(MediaSectionKind::PLANES).is_some());
        assert_eq!(image.plane(0).unwrap().memory(), memory);
        assert_eq!(image.plane(0).unwrap().bytes(), pixels);
        let data = image
            .media()
            .section(MediaSectionKind::DATA)
            .unwrap()
            .bytes();
        assert_eq!(&data[..64], &[0; 64]);
    }

    #[test]
    fn validation_precedes_capacity_and_preserves_output() {
        let surface =
            SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let short_plane = [0; 3];
        let short_planes: &[&[u8]] = &[&short_plane];
        let invalid = RawImageAsset::new(surface, short_planes);
        let mut out = [0xa5; 16];
        assert_eq!(
            invalid.encode_into(&mut out),
            Err(ImageEncodeError::PlaneLengthMismatch {
                index: 0,
                expected: 4,
                actual: 3,
            })
        );
        assert_eq!(out, [0xa5; 16]);

        let pixels = [0; 4];
        let planes: &[&[u8]] = &[&pixels];
        let valid = RawImageAsset::new(surface, planes);
        let needed = valid.encoded_len().unwrap();
        assert_eq!(
            valid.encode_into(&mut out),
            Err(ImageEncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            })
        );
        assert_eq!(out, [0xa5; 16]);
    }

    #[test]
    fn plane_and_color_table_cardinality_are_exact() {
        let indexed =
            SurfaceDescriptor::new(1, 1, SampleLayout::I1, ColorDescription::SRGB).unwrap();
        let pixel = [0; 1];
        let planes: &[&[u8]] = &[&pixel];
        assert_eq!(
            RawImageAsset::new(indexed, planes).encoded_len(),
            Err(ImageEncodeError::MissingColorTable)
        );
        assert_eq!(
            RawImageAsset::new(indexed, planes)
                .with_color_table(&[0; 7])
                .encoded_len(),
            Err(ImageEncodeError::ColorTableLengthMismatch {
                expected: 8,
                actual: 7,
            })
        );

        let rgb =
            SurfaceDescriptor::new(1, 1, SampleLayout::RGB565, ColorDescription::SRGB).unwrap();
        let none: &[&[u8]] = &[];
        assert_eq!(
            RawImageAsset::new(rgb, none).encoded_len(),
            Err(ImageEncodeError::PlaneCountMismatch {
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn invalid_explicit_layout_is_rechecked_against_the_surface() {
        let small = SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let large = SurfaceDescriptor::new(4, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let memory = PlaneMemoryLayout::tight(small.plane(0).unwrap()).unwrap();
        let pixels = [0; 4];
        let planes: &[&[u8]] = &[&pixels];
        assert_eq!(
            RawImageAsset::new(large, planes)
                .with_memory_layouts(&[memory])
                .encoded_len(),
            Err(ImageEncodeError::InvalidPlaneLayout {
                index: 0,
                error: PlaneMemoryError::AllocationWidthTooSmall {
                    minimum: 4,
                    actual: 2,
                },
            })
        );
    }

    #[test]
    fn exact_buffer_encoding_preserves_suffix() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let pixel = [0x7f];
        let planes: &[&[u8]] = &[&pixel];
        let asset = RawImageAsset::new(surface, planes);
        let needed = asset.encoded_len().unwrap();
        let mut out = vec![0xa5; needed + 9];
        assert_eq!(asset.encode_into(&mut out), Ok(needed));
        assert_eq!(&out[needed..], &[0xa5; 9]);
        assert_eq!(
            RawImageView::open(&out[..needed])
                .unwrap()
                .plane(0)
                .unwrap()
                .bytes(),
            pixel
        );
    }
}
