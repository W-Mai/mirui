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
        for (positioned, frame) in draw.glyphs.iter().zip(draw.frames) {
            let tangent = Point {
                x: crate::types::fixed::from_textflow(frame.unit_tangent.x),
                y: crate::types::fixed::from_textflow(frame.unit_tangent.y),
            };
            let pose = Transform {
                m00: tangent.x,
                m01: crate::types::Fixed::ZERO - tangent.y,
                tx: draw.pos.x + crate::types::fixed::from_textflow(frame.local_origin.x),
                m10: tangent.y,
                m11: tangent.x,
                ty: draw.pos.y + crate::types::fixed::from_textflow(frame.local_origin.y),
            };
            let transform = draw.transform.compose(&pose);
            let glyph = [textflow::shaping::PositionedGlyph::new(
                positioned.glyph_id(),
                textflow::shaping::FlowPoint { x: 0, y: 0 },
            )];
            self.label_cache.draw_glyph_run(
                self.canvas,
                &Point::ZERO,
                &glyph,
                draw.font,
                &transform,
                draw.clip,
                draw.color,
                draw.opacity,
                self.viewport,
            );
        }
    }
}
