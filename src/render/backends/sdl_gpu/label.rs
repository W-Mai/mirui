use super::SdlGpuRenderer;
use crate::render::font::Font;
use crate::types::{Color, Point, Rect, Transform};

pub(super) struct GlyphRunDraw<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub font: &'a Font,
    pub transform: &'a Transform,
    pub clip: &'a Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

pub(super) struct PosedGlyphRunDraw<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub frames: &'a [textflow::placement::GlyphFrame],
    pub font: &'a Font,
    pub transform: &'a Transform,
    pub clip: &'a Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

impl SdlGpuRenderer<'_> {
    pub(super) fn draw_glyph_run_inner(&mut self, draw: GlyphRunDraw<'_>) {
        let GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform,
            clip,
            color,
            opacity,
        } = draw;
        self.label_cache.draw_glyph_run(
            self.canvas,
            pos,
            glyphs,
            font,
            transform,
            clip,
            color,
            opacity,
            self.viewport,
        );
    }

    pub(super) fn draw_posed_glyph_run_inner(&mut self, draw: PosedGlyphRunDraw<'_>) {
        self.label_cache.draw_posed_glyph_run(
            self.canvas,
            *draw.pos,
            draw.glyphs,
            draw.frames,
            draw.font,
            *draw.transform,
            *draw.clip,
            *draw.color,
            draw.opacity,
            self.viewport,
        );
    }
}
