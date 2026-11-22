//! One-face MIRX font provider with shared placement and representation-specific geometry.

use alloc::rc::Rc;
use core::cell::Cell;

use super::{
    Font, FontFaceId, FontMetrics, FontProvider, FontSurfaceId, GlyphId, GlyphSurface, RasterGlyph,
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
    InvalidSize,
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

#[derive(Clone, Debug)]
pub struct MirxFontProvider {
    id: FontFaceId,
    face: FontView<'static>,
    decoded: &'static [Option<SurfaceView<'static>>],
    selected: Cell<Option<(u16, mirx::font::FontRepresentationView<'static>)>>,
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
        Self {
            id,
            face,
            decoded,
            selected: Cell::new(None),
        }
    }

    pub const fn view(&self) -> FontView<'static> {
        self.face
    }

    fn selected(&self, size: u16) -> Option<mirx::font::FontRepresentationView<'static>> {
        if let Some((cached_size, selected)) = self.selected.get()
            && cached_size == size
        {
            return Some(selected);
        }
        let selected = self
            .face
            .select(
                FontRepresentationRequest::new(size)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .ok()?;
        self.selected.set(Some((size, selected)));
        Some(selected)
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
            mirx::types::Fixed::from_ratio(advance, 256),
            ppem,
            self.face.face().units_per_em(),
        )
    }

    fn pair_kerning(&self, left: GlyphId, right: GlyphId, ppem: u16) -> crate::types::Fixed {
        use textflow::shaping::GlyphSource;

        let source = crate::text::mirx::MirxGlyphSource::new(self.id, self.face);
        let Some(value) = source.kerning(left, right).ok().and_then(|value| {
            crate::types::fixed::checked_scale_mirx(
                mirx::types::Fixed::from_ratio(value, 256),
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
        let (plane, region) = match selected.glyphs() {
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
        let face = self.face.face();
        FontMetrics {
            ascender: scale(face.ascender(), requested_size, face.units_per_em()),
            descender: scale(face.descender(), requested_size, face.units_per_em()),
            line_height: scale(face.ascender(), requested_size, face.units_per_em())
                - scale(face.descender(), requested_size, face.units_per_em())
                + scale(face.line_gap(), requested_size, face.units_per_em()),
        }
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

impl Font {
    /// Opens a MIRX font at an explicit logical layout size.
    pub fn from_mirx(
        family: &'static str,
        size: u16,
        bytes: &'static [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, MirxFontError> {
        if size == 0 {
            return Err(MirxFontError::InvalidSize);
        }
        let provider = MirxFontProvider::from_mirx(bytes, limits)?;
        Ok(Self {
            family,
            size,
            provider: Rc::new(provider),
        })
    }

    /// Reconstructs encoded MIRX surfaces into caller-owned storage.
    pub fn from_mirx_with_storage(
        family: &'static str,
        size: u16,
        bytes: &'static [u8],
        limits: &PayloadLimits,
        storage: MirxFontStorage<'_>,
    ) -> Result<Self, MirxFontError> {
        if size == 0 {
            return Err(MirxFontError::InvalidSize);
        }
        let provider = MirxFontProvider::from_mirx_with_storage(bytes, limits, storage)?;
        Ok(Self {
            family,
            size,
            provider: Rc::new(provider),
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/font/mirx.rs"]
mod tests;
