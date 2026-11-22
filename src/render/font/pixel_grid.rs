//! Pixel fonts drawn only at whole multiples of their design size.

use alloc::rc::Rc;

use super::{Font, FontFaceId, FontMetrics, FontProvider, GlyphId, RasterGlyph};
use crate::types::Fixed;

/// Wraps a face whose glyphs are drawn on a grid of `step` pixels per em.
///
/// Every size asked of it snaps to the nearest multiple of `step`, ties going
/// down and nothing below one step, so a strike baked at that multiple draws
/// each font pixel as a whole block instead of resampling it.
struct PixelGrid {
    inner: Rc<dyn FontProvider>,
    step: u16,
}

fn snap(step: u16, ppem: u16) -> u16 {
    let step = u32::from(step.max(1));
    let steps = (2 * u32::from(ppem) + step - 1) / (2 * step);
    u16::try_from(steps.max(1) * step).unwrap_or(u16::MAX)
}

impl FontProvider for PixelGrid {
    fn face_id(&self) -> FontFaceId {
        self.inner.face_id()
    }

    fn revision(&self) -> u64 {
        self.inner.revision()
    }

    fn snap_ppem(&self, ppem: u16) -> u16 {
        snap(self.step, ppem)
    }

    fn map_char(&self, ch: char) -> Option<GlyphId> {
        self.inner.map_char(ch)
    }

    fn glyph_advance(&self, glyph: GlyphId, ppem: u16) -> Option<Fixed> {
        self.inner.glyph_advance(glyph, snap(self.step, ppem))
    }

    fn raster(
        &self,
        glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        let layout = snap(self.step, layout_ppem);
        let output = u32::from(output_ppem) * u32::from(layout) / u32::from(layout_ppem.max(1));
        let output = snap(self.step, u16::try_from(output).unwrap_or(u16::MAX));
        self.inner.raster(glyph, layout, output)
    }

    fn metrics(&self, ppem: u16) -> FontMetrics {
        self.inner.metrics(snap(self.step, ppem))
    }

    fn notdef_glyph(&self) -> Option<GlyphId> {
        self.inner.notdef_glyph()
    }

    fn pair_kerning(&self, left: GlyphId, right: GlyphId, ppem: u16) -> Fixed {
        self.inner.pair_kerning(left, right, snap(self.step, ppem))
    }

    fn shaping_data(&self) -> Option<&[u8]> {
        self.inner.shaping_data()
    }

    fn supports_complex_shaping(&self) -> bool {
        self.inner.supports_complex_shaping()
    }

    fn covers(&self, cluster: &str) -> bool {
        self.inner.covers(cluster)
    }

    fn shape_into(
        &self,
        ppem: u16,
        request: &textflow::shaping::ShapeRequest<'_>,
        output: &mut [textflow::shaping::ShapedGlyph],
    ) -> Result<usize, textflow::shaping::ShapeError> {
        self.inner
            .shape_into(snap(self.step, ppem), request, output)
    }
}

impl Font {
    /// Draws this face only at whole multiples of `step` pixels, for a pixel
    /// font whose strikes are baked at those sizes. A text asking for any
    /// other size is laid out and drawn at the nearest multiple instead (ties
    /// go down, and no size goes below `step`), so its glyphs stay sharp.
    pub fn with_pixel_grid(mut self, step: u16) -> Self {
        let step = step.max(1);
        self.size = snap(step, self.size);
        self.provider = Rc::new(PixelGrid {
            inner: self.provider,
            step,
        });
        self
    }

    /// The size this face lays out and draws a text that asks for
    /// `requested`, or for its own size when the text names none.
    pub fn ppem_for(&self, requested: Option<u16>) -> u16 {
        self.provider
            .snap_ppem(requested.unwrap_or(self.size).max(1))
            .max(1)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/font/pixel_grid.rs"]
mod tests;
