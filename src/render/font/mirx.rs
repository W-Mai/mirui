//! One-face MIRX font provider with shared placement and representation-specific geometry.

use alloc::rc::Rc;

use super::{
    Font, FontBackend, FontFaceId, FontMetrics, FontProvider, FontSurfaceId, GlyphId, GlyphSurface,
    RasterGlyph,
};
use mirx::{
    font::{
        FontError, FontGlyphs, FontRepresentationFallback, FontRepresentationRequest, FontView,
    },
    image::{CoverageBudget, SurfaceRequirements, SurfaceView, UnitGroup},
    reader::PayloadLimits,
};

fn scale(value: mirx::types::Fixed, numerator: u16, denominator: u16) -> crate::types::Fixed {
    crate::types::fixed::checked_scale_mirx(value, numerator, denominator).unwrap_or(
        if value.is_negative() {
            crate::types::Fixed::MIN
        } else {
            crate::types::Fixed::MAX
        },
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MirxFontError {
    Container,
    MissingFace,
    MultipleFaces,
    InvalidFace(FontError),
    EncodedStorage,
    SurfaceSlotsTooSmall {
        needed: usize,
        available: usize,
    },
    SurfaceOutputTooSmall {
        surface: usize,
        needed: usize,
        available: usize,
    },
    SurfaceSizeOverflow {
        surface: usize,
    },
    SurfaceWorkspace {
        surface: usize,
        error: mirx::image::BufferRequirementError,
    },
    EncodedSurface {
        surface: usize,
        error: mirx::font::EncodedGlyphError,
    },
}

/// Caller-owned persistent and temporary storage for encoded FONT surfaces.
///
/// `surfaces` and `output` remain borrowed by the resulting provider. `groups`
/// and `workspace` are temporary construction scratch and may be reused after
/// the constructor returns.
pub struct MirxFontStorage<'scratch> {
    surfaces: &'static mut [Option<SurfaceView<'static>>],
    output: &'static mut [u8],
    groups: &'scratch mut [Option<UnitGroup<'static>>],
    workspace: &'scratch mut [u8],
    requirements: SurfaceRequirements,
}

impl<'scratch> MirxFontStorage<'scratch> {
    pub fn new(
        surfaces: &'static mut [Option<SurfaceView<'static>>],
        output: &'static mut [u8],
        groups: &'scratch mut [Option<UnitGroup<'static>>],
        workspace: &'scratch mut [u8],
    ) -> Self {
        Self {
            surfaces,
            output,
            groups,
            workspace,
            requirements: SurfaceRequirements::new(),
        }
    }

    pub const fn with_requirements(mut self, requirements: SurfaceRequirements) -> Self {
        self.requirements = requirements;
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MirxFontProvider {
    id: FontFaceId,
    face: FontView<'static>,
    default_size: u16,
    decoded: &'static [Option<SurfaceView<'static>>],
}

impl MirxFontProvider {
    /// Opens exactly one FONT face from a complete MIRX file.
    pub fn from_mirx(bytes: &'static [u8], limits: &PayloadLimits) -> Result<Self, MirxFontError> {
        let reader = mirx::Reader::open(bytes).map_err(|_| MirxFontError::Container)?;
        let mut faces = reader
            .chunks()
            .filter(|chunk| chunk.chunk_type() == mirx::ChunkType::FONT);
        let chunk = faces.next().ok_or(MirxFontError::MissingFace)?;
        if faces.next().is_some() {
            return Err(MirxFontError::MultipleFaces);
        }
        let face = chunk
            .font(limits)
            .map_err(MirxFontError::InvalidFace)?
            .expect("FONT filter");
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_raw_view(face, face_id(bytes))
    }

    /// Opens one standalone FONT payload whose backing storage is static.
    pub fn from_payload(
        payload: &'static [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, MirxFontError> {
        let face = FontView::open(payload, limits).map_err(MirxFontError::InvalidFace)?;
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_raw_view(face, face_id(payload))
    }

    /// Opens exactly one FONT face and reconstructs encoded surfaces into
    /// caller-owned persistent storage before returning the provider.
    pub fn from_mirx_with_storage(
        bytes: &'static [u8],
        limits: &PayloadLimits,
        storage: MirxFontStorage<'_>,
    ) -> Result<Self, MirxFontError> {
        let reader = mirx::Reader::open(bytes).map_err(|_| MirxFontError::Container)?;
        let mut faces = reader
            .chunks()
            .filter(|chunk| chunk.chunk_type() == mirx::ChunkType::FONT);
        let chunk = faces.next().ok_or(MirxFontError::MissingFace)?;
        if faces.next().is_some() {
            return Err(MirxFontError::MultipleFaces);
        }
        let face = chunk
            .font(limits)
            .map_err(MirxFontError::InvalidFace)?
            .expect("FONT filter");
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_view_with_storage(face, face_id(bytes), limits, storage)
    }

    /// Opens one standalone FONT payload and reconstructs encoded surfaces into
    /// caller-owned persistent storage before returning the provider.
    pub fn from_payload_with_storage(
        payload: &'static [u8],
        limits: &PayloadLimits,
        storage: MirxFontStorage<'_>,
    ) -> Result<Self, MirxFontError> {
        let face = FontView::open(payload, limits).map_err(MirxFontError::InvalidFace)?;
        face.preflight(limits).map_err(MirxFontError::InvalidFace)?;
        Self::from_view_with_storage(face, face_id(payload), limits, storage)
    }

    fn from_raw_view(face: FontView<'static>, id: FontFaceId) -> Result<Self, MirxFontError> {
        for index in 0..face.representations().len() {
            if matches!(face.glyphs(index), Some(FontGlyphs::Encoded(_))) {
                return Err(MirxFontError::EncodedStorage);
            }
        }
        Ok(Self::from_view(face, id, &[]))
    }

    fn from_view_with_storage(
        face: FontView<'static>,
        id: FontFaceId,
        limits: &PayloadLimits,
        storage: MirxFontStorage<'_>,
    ) -> Result<Self, MirxFontError> {
        let MirxFontStorage {
            surfaces,
            output,
            groups,
            workspace,
            requirements,
        } = storage;
        if surfaces.len() < face.surface_count() {
            return Err(MirxFontError::SurfaceSlotsTooSmall {
                needed: face.surface_count(),
                available: surfaces.len(),
            });
        }

        // Admission and exact arena placement complete before any persistent
        // byte or view slot changes. The second pass replays immutable plans.
        let mut output_cursor = 0usize;
        let mut budget = CoverageBudget::new(limits.max_raster_work());
        for surface in 0..face.surface_count() {
            let Some(FontGlyphs::Encoded(encoded)) = Self::surface_storage(face, surface) else {
                continue;
            };
            let prepared = encoded
                .groups_into(groups, &mut budget)
                .map_err(|error| MirxFontError::EncodedSurface { surface, error })?;
            let plan = prepared
                .decode_surface_plan(requirements, limits)
                .map_err(|error| MirxFontError::EncodedSurface { surface, error })?;
            let needed = plan.memory_plan().buffer_requirements();
            let address = (output.as_ptr() as usize)
                .checked_add(output_cursor)
                .ok_or(MirxFontError::SurfaceSizeOverflow { surface })?;
            let alignment = needed.base_alignment();
            let alignment = usize::try_from(alignment.get()).expect("u32 fits usize");
            let padding = (alignment - address % alignment) % alignment;
            let span = padding
                .checked_add(needed.byte_len())
                .ok_or(MirxFontError::SurfaceSizeOverflow { surface })?;
            let available = output.len().saturating_sub(output_cursor);
            if span > available {
                return Err(MirxFontError::SurfaceOutputTooSmall {
                    surface,
                    needed: span,
                    available,
                });
            }
            needed
                .validate(&output[output_cursor + padding..])
                .expect("checked aligned surface range");
            plan.workspace_requirements()
                .validate(workspace)
                .map_err(|error| MirxFontError::SurfaceWorkspace { surface, error })?;
            output_cursor += span;
        }

        surfaces[..face.surface_count()].fill(None);
        let mut remaining = output;
        let mut budget = CoverageBudget::new(limits.max_raster_work());
        for (surface, slot) in surfaces.iter_mut().enumerate().take(face.surface_count()) {
            let Some(FontGlyphs::Encoded(encoded)) = Self::surface_storage(face, surface) else {
                continue;
            };
            let prepared = encoded
                .groups_into(groups, &mut budget)
                .map_err(|error| MirxFontError::EncodedSurface { surface, error })?;
            let plan = prepared
                .decode_surface_plan(requirements, limits)
                .map_err(|error| MirxFontError::EncodedSurface { surface, error })?;
            let needed = plan.memory_plan().buffer_requirements();
            let address = remaining.as_ptr() as usize;
            let alignment = needed.base_alignment();
            let alignment = usize::try_from(alignment.get()).expect("u32 fits usize");
            let padding = (alignment - address % alignment) % alignment;
            let span = padding
                .checked_add(needed.byte_len())
                .expect("surface span admitted in the first pass");
            let current = core::mem::take(&mut remaining);
            let (allocation, rest) = current.split_at_mut(span);
            remaining = rest;
            let decoded = plan
                .decode_into(&mut allocation[padding..], workspace)
                .expect("immutable font surface admitted in the first pass");
            *slot = Some(decoded);
        }
        Ok(Self::from_view(face, id, &surfaces[..face.surface_count()]))
    }

    fn surface_storage(face: FontView<'static>, surface: usize) -> Option<FontGlyphs<'static>> {
        let representation = (0..face.representations().len()).find(|index| {
            face.representation(*index)
                .is_some_and(|value| usize::from(value.record().surface_index()) == surface)
        })?;
        face.glyphs(representation)
    }

    fn from_view(
        face: FontView<'static>,
        id: FontFaceId,
        decoded: &'static [Option<SurfaceView<'static>>],
    ) -> Self {
        let representations = face.representations();
        let default_size = representations
            .iter()
            .map(|record| record.representation())
            .filter(|representation| {
                matches!(
                    representation.kind(),
                    mirx::font::FontRepresentationKind::Coverage { .. }
                )
            })
            .map(|representation| representation.design_ppem())
            .max()
            .unwrap_or_else(|| {
                representations
                    .get(0)
                    .expect("nonempty admitted face")
                    .representation()
                    .design_ppem()
            });
        Self {
            id,
            face,
            default_size,
            decoded,
        }
    }

    pub const fn default_size(self) -> u16 {
        self.default_size
    }
    pub const fn view(self) -> FontView<'static> {
        self.face
    }

    fn selected(&self, size: u16) -> Option<mirx::font::FontRepresentationView<'static>> {
        self.face
            .select(
                FontRepresentationRequest::new(size)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .ok()
    }
}

impl FontProvider for MirxFontProvider {
    fn face_id(&self) -> FontFaceId {
        self.id
    }

    fn map_char(&self, ch: char) -> Option<GlyphId> {
        self.face
            .map_char(ch)
            .map(|glyph| GlyphId::new(glyph.get()))
    }

    fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<crate::types::Fixed> {
        use textflow::shaping::GlyphSource;

        let source = crate::text::mirx::MirxGlyphSource::new(self.id, self.face);
        let advance = source.glyph_advance(glyph).ok()?.x;
        crate::types::fixed::checked_scale_mirx(
            mirx::types::Fixed::from_le_bytes(advance.to_le_bytes()),
            ppem,
            self.face.face().units_per_em(),
        )
    }

    fn pair_kerning(&self, left: GlyphId, right: GlyphId, ppem: u16) -> crate::types::Fixed {
        use textflow::shaping::GlyphSource;

        let source = crate::text::mirx::MirxGlyphSource::new(self.id, self.face);
        let Some(value) = source.kerning(left, right).ok().and_then(|value| {
            crate::types::fixed::checked_scale_mirx(
                mirx::types::Fixed::from_le_bytes(value.to_le_bytes()),
                ppem,
                self.face.face().units_per_em(),
            )
        }) else {
            return crate::types::Fixed::ZERO;
        };
        value
    }

    fn raster(
        &self,
        glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        let selected = self.selected(output_ppem)?;
        let mirx_glyph = mirx::font::GlyphId::new(glyph.value());
        let ordinal = self.face.raster_ordinal(mirx_glyph)?;
        let metric = selected.raster_metrics(mirx_glyph)?;
        let (plane, region) = match self.face.glyphs(selected.index())? {
            FontGlyphs::Raw(storage) => {
                let raster = storage.get(ordinal)?;
                (raster.storage().plane(0)?, raster.region())
            }
            FontGlyphs::Encoded(_) => {
                let surface = usize::from(selected.record().surface_index());
                let decoded = self.decoded.get(surface).copied().flatten()?;
                (decoded.plane(0)?, selected.map().get(ordinal)?)
            }
        };
        let geometry = plane.geometry();
        let surface_index = selected.record().surface_index();
        let surface = GlyphSurface::new(
            plane.bytes(),
            geometry.width(),
            geometry.height(),
            plane.memory().stride(),
            selected.surface().sample_layout(),
            plane.memory().required_alignment(),
            surface_id(self.id, surface_index),
        )
        .ok()?;
        Some(RasterGlyph {
            surface,
            region: (region.width() != 0 && region.height() != 0).then_some(region),
            representation: selected.record().representation(),
            offset_x: scale(
                metric.offset_x(),
                layout_ppem,
                selected.record().representation().design_ppem(),
            ),
            offset_y: scale(
                metric.offset_y(),
                layout_ppem,
                selected.record().representation().design_ppem(),
            ),
        })
    }

    fn metrics(&self, requested_size: u16) -> FontMetrics {
        self.selected(requested_size)
            .map(|_| {
                let face = self.face.face();
                FontMetrics {
                    ascender: scale(face.ascender(), requested_size, face.units_per_em()),
                    descender: scale(face.descender(), requested_size, face.units_per_em()),
                    line_height: scale(face.ascender(), requested_size, face.units_per_em())
                        - scale(face.descender(), requested_size, face.units_per_em())
                        + scale(face.line_gap(), requested_size, face.units_per_em()),
                }
            })
            .unwrap_or(FontMetrics {
                ascender: crate::types::Fixed::ZERO,
                descender: crate::types::Fixed::ZERO,
                line_height: crate::types::Fixed::ONE,
            })
    }

    fn notdef_glyph(&self) -> Option<GlyphId> {
        Some(GlyphId::new(self.face.face().default_glyph().get()))
    }

    fn shaping_data(&self) -> Option<&[u8]> {
        self.face.shaping_data().map(|data| data.as_bytes())
    }

    fn shape_into(
        &self,
        ppem: u16,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        use textflow::shaping::Typeface;

        crate::text::mirx::MirxGlyphSource::new(self.id, self.face)
            .typeface(ppem)
            .shape_into(request, output)
    }
}

fn face_id(bytes: &[u8]) -> FontFaceId {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    FontFaceId::new(hash | 1 << 63)
}

fn surface_id(face: FontFaceId, surface: u16) -> FontSurfaceId {
    FontSurfaceId::new(face.value().rotate_left(17) ^ u64::from(surface))
}

pub fn font_from_mirx(
    family: &'static str,
    bytes: &'static [u8],
    limits: &PayloadLimits,
) -> Result<Font, MirxFontError> {
    let provider = MirxFontProvider::from_mirx(bytes, limits)?;
    Ok(Font {
        family,
        size: provider.default_size(),
        backend: FontBackend::Custom(Rc::new(provider)),
    })
}

/// Builds a [`Font`] whose encoded surfaces reside in caller-owned storage.
pub fn font_from_mirx_with_storage(
    family: &'static str,
    bytes: &'static [u8],
    limits: &PayloadLimits,
    storage: MirxFontStorage<'_>,
) -> Result<Font, MirxFontError> {
    let provider = MirxFontProvider::from_mirx_with_storage(bytes, limits, storage)?;
    Ok(Font {
        family,
        size: provider.default_size(),
        backend: FontBackend::Custom(Rc::new(provider)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirx::{
        coding::Rle,
        font::{
            CmapEntry, FontAdvanceSource, FontAsset, FontFace, GlyphId, GlyphMap,
            GlyphSurfaceAsset, RasterMetrics, RawGlyphs, RepresentationAsset,
        },
        image::{
            AtlasMap, ColorDescription, EncodedImageAsset, PlaneMemoryLayout, Region, SampleLayout,
            SurfaceDescriptor,
        },
        types::Fixed,
    };

    fn atlas_face() -> MirxFontProvider {
        let cmap = [
            CmapEntry::new(' ', GlyphId::new(0)),
            CmapEntry::new('A', GlyphId::new(1)),
        ];
        let regions = [
            Region::new(0, 0, 0, 0).unwrap(),
            Region::new(3, 1, 3, 2).unwrap(),
        ];
        let atlas = AtlasMap::new(8, 4, &regions).unwrap();
        let map = GlyphMap::atlas(atlas);
        let surface =
            SurfaceDescriptor::new(8, 4, SampleLayout::A1, ColorDescription::NONE).unwrap();
        let memory = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_stride(64)
            .with_alignment(mirx::types::ByteAlignment::new(64).unwrap())
            .build()
            .unwrap();
        let samples = [0xa5; 256];
        let glyphs = RawGlyphs::builder(map, SampleLayout::A1)
            .with_memory_layout(memory)
            .build(&samples)
            .unwrap();
        let advances = [Fixed::from_int(250), Fixed::from_ratio(1_375, 4)];
        let face = FontFace::new(
            1_000,
            GlyphId::NOTDEF,
            2,
            Fixed::from_int(750),
            Fixed::from_int(-250),
            Fixed::ZERO,
        )
        .unwrap();
        let representation = RepresentationAsset::new(
            mirx::font::FontRepresentation::coverage(1, 16, 4).unwrap(),
            0,
        )
        .with_atlas_map(0);
        let raster_metrics = [
            RasterMetrics::new(Fixed::ZERO, Fixed::from_int(12)),
            RasterMetrics::new(Fixed::from_ratio(-1, 2), Fixed::from_ratio(45, 4)),
        ];
        let payload = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
            .with_rasters(
                &[representation],
                &raster_metrics,
                &[GlyphSurfaceAsset::raw(glyphs)],
            )
            .with_atlas_maps(&[atlas])
            .encode()
            .unwrap();
        MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST)
            .unwrap()
    }

    #[test]
    fn atlas_regions_and_fractional_metrics_reach_the_renderer_unchanged() {
        let provider = atlas_face();
        let glyph_id = provider.map_char('A').unwrap();
        let glyph = provider.raster(glyph_id, 16, 16).unwrap();
        assert_eq!(
            provider.glyph_advance(glyph_id, 16),
            Some(crate::types::Fixed::from_ratio(11, 2))
        );
        assert_eq!(glyph.surface.stride(), 64);
        let region = glyph.region.unwrap();
        assert_eq!(
            (region.x(), region.y(), region.width(), region.height()),
            (3, 1, 3, 2)
        );
        assert_eq!(glyph.offset_x, crate::types::Fixed::from_ratio(-1, 2));
        assert_eq!(glyph.offset_y, crate::types::Fixed::from_ratio(45, 4));
        let space = provider.map_char(' ').unwrap();
        assert!(provider.raster(space, 16, 16).unwrap().region.is_none());
        assert!(provider.map_char('Z').is_none());
    }

    #[test]
    fn face_metrics_and_glyph_advances_scale_with_requested_size() {
        let provider = atlas_face();
        let metrics = provider.metrics(8);
        assert_eq!(metrics.ascender, crate::types::Fixed::from_int(6));
        assert_eq!(metrics.descender, crate::types::Fixed::from_int(-2));
        assert_eq!(metrics.line_height, crate::types::Fixed::from_int(8));
        assert_eq!(
            provider.glyph_advance(provider.map_char('A').unwrap(), 8),
            Some(crate::types::Fixed::from_ratio(11, 4))
        );
    }

    #[test]
    fn encoded_surfaces_require_an_explicit_decode_workspace_provider() {
        let cmap = [
            CmapEntry::new('A', GlyphId::new(0)),
            CmapEntry::new('B', GlyphId::new(1)),
        ];
        let advances = [Fixed::ONE; 2];
        let face =
            FontFace::new(1, GlyphId::NOTDEF, 2, Fixed::ONE, Fixed::ZERO, Fixed::ZERO).unwrap();
        let map = GlyphMap::cells(2, 2, cmap.len()).unwrap();
        let surface =
            SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let image = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
        let representation = RepresentationAsset::new(
            mirx::font::FontRepresentation::coverage(8, 1, 8).unwrap(),
            0,
        );
        let raster_metrics = [RasterMetrics::default(); 2];
        let payload = FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
            .with_rasters(
                &[representation],
                &raster_metrics,
                &[GlyphSurfaceAsset::Encoded { map, image }],
            )
            .encode()
            .unwrap();
        assert!(matches!(
            MirxFontProvider::from_payload(alloc::vec::Vec::leak(payload), &PayloadLimits::HOST),
            Err(MirxFontError::EncodedStorage)
        ));
    }

    fn encoded_face_payload() -> &'static [u8] {
        let cmap = [
            CmapEntry::new('A', GlyphId::new(0)),
            CmapEntry::new('B', GlyphId::new(1)),
        ];
        let advances = [Fixed::ONE; 2];
        let face =
            FontFace::new(1, GlyphId::NOTDEF, 2, Fixed::ONE, Fixed::ZERO, Fixed::ZERO).unwrap();
        let map = GlyphMap::cells(2, 2, cmap.len()).unwrap();
        let surface =
            SurfaceDescriptor::new(2, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let image = EncodedImageAsset::new(surface, Rle::new().record(), &[0x87, 42]);
        let representation = RepresentationAsset::new(
            mirx::font::FontRepresentation::coverage(8, 1, 8).unwrap(),
            0,
        );
        let raster_metrics = [RasterMetrics::default(); 2];
        alloc::vec::Vec::leak(
            FontAsset::new(face, &cmap, FontAdvanceSource::Advances(&advances))
                .with_rasters(
                    &[representation],
                    &raster_metrics,
                    &[GlyphSurfaceAsset::Encoded { map, image }],
                )
                .encode()
                .unwrap(),
        )
    }

    #[test]
    fn encoded_surfaces_reside_in_aligned_caller_storage() {
        #[repr(align(64))]
        struct Aligned([u8; 320]);

        let payload = encoded_face_payload();
        let surfaces = alloc::boxed::Box::leak(alloc::boxed::Box::new([None]));
        let output =
            &mut alloc::boxed::Box::leak(alloc::boxed::Box::new(Aligned([0xa5; 320]))).0[1..];
        let mut groups = [None];
        let mut workspace = [0x5a; 8];
        let storage = MirxFontStorage::new(surfaces, output, &mut groups, &mut workspace)
            .with_requirements(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            );
        let provider =
            MirxFontProvider::from_payload_with_storage(payload, &PayloadLimits::HOST, storage)
                .unwrap();

        for (ch, y) in [('A', 0), ('B', 2)] {
            let glyph = provider
                .raster(provider.map_char(ch).unwrap(), 8, 8)
                .unwrap();
            let samples = glyph.surface.samples();
            let stride = glyph.surface.stride();
            let region = glyph.region.unwrap();
            assert_eq!(samples.as_ptr() as usize % 64, 0);
            assert_eq!(samples.len(), 256);
            assert_eq!(stride, 64);
            assert_eq!(region, Region::new(0, y, 2, 2).unwrap());
            for row in 0..4 {
                assert_eq!(&samples[row * 64..row * 64 + 2], &[42; 2]);
                assert!(
                    samples[row * 64 + 2..row * 64 + 64]
                        .iter()
                        .all(|byte| *byte == 0)
                );
            }
        }

        groups.fill(None);
        workspace.fill(0);
    }

    #[test]
    fn encoded_surface_admission_precedes_persistent_writes() {
        #[repr(align(64))]
        struct Short([u8; 255]);

        let payload = encoded_face_payload();
        let surfaces = alloc::boxed::Box::leak(alloc::boxed::Box::new([None]));
        let surfaces_ptr = surfaces.as_ptr();
        let output = &mut alloc::boxed::Box::leak(alloc::boxed::Box::new(Short([0xa5; 255]))).0;
        let output_ptr = output.as_ptr();
        let mut groups = [None];
        let mut workspace = [0x5a; 8];
        let storage = MirxFontStorage::new(surfaces, output, &mut groups, &mut workspace)
            .with_requirements(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                    .with_stride_multiple(64),
            );
        assert!(matches!(
            MirxFontProvider::from_payload_with_storage(payload, &PayloadLimits::HOST, storage),
            Err(MirxFontError::SurfaceOutputTooSmall {
                surface: 0,
                needed: 256,
                available: 255,
            })
        ));

        // SAFETY: construction failed, so no provider retains the static
        // buffers; the leaked allocations remain live and uniquely owned here.
        let output = unsafe { core::slice::from_raw_parts(output_ptr, 255) };
        let surfaces = unsafe { core::slice::from_raw_parts(surfaces_ptr, 1) };
        assert!(output.iter().all(|byte| *byte == 0xa5));
        assert_eq!(surfaces, &[None]);
    }
}
