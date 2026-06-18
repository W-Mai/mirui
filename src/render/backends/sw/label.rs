use super::SwRenderer;
use crate::render::font::{Font, GlyphKind};
use crate::types::{Color, Fixed, Point, Rect};

impl SwRenderer<'_> {
    pub(super) fn draw_label_inner(
        &mut self,
        pos: &Point,
        text: &[u8],
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        let phys_pos = self.viewport.point_to_physical(*pos);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let scale = self.viewport.scale().to_int().max(1);
        let phys_bounds = phys_clip.pixel_bounds();
        let (mut cx, cy) = phys_pos.floor();
        let metrics = font.metrics();
        let char_h = metrics.line_height as i32;
        // Decode the byte stream as UTF-8 so multi-byte CJK glyphs
        // map to one codepoint each. Invalid bytes fall back to U+FFFD,
        // which the font is free to skip.
        let chars = core::str::from_utf8(text)
            .map(|s| s.chars().collect::<alloc::vec::Vec<_>>())
            .unwrap_or_else(|_| text.iter().map(|&b| b as char).collect());
        for ch in chars {
            let Some(g) = font.glyph(ch) else {
                continue;
            };
            let advance = g.advance as i32 * scale;
            match &g.kind {
                GlyphKind::Mono(bitmap) => {
                    self.blit_mono_glyph(bitmap, cx, cy, scale, char_h, phys_bounds, color, opa);
                }
                GlyphKind::Sdf {
                    atlas,
                    source_size,
                    bit_depth,
                    spread,
                    ..
                } => {
                    self.blit_sdf_glyph(
                        atlas,
                        *source_size,
                        *bit_depth,
                        *spread,
                        cx,
                        cy,
                        scale,
                        phys_bounds,
                        color,
                        opa,
                    );
                }
            }
            cx += advance;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_mono_glyph(
        &mut self,
        bitmap: &[u8],
        cx: i32,
        cy: i32,
        scale: i32,
        char_h: i32,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        for row in 0..char_h.min(bitmap.len() as i32) {
            let byte = bitmap[row as usize];
            for col in 0..8 {
                if byte & (0x80 >> col) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = cx + col * scale + sx;
                        let py = cy + row * scale + sy;
                        if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                            self.target.blend_pixel(
                                Fixed::from_int(px),
                                Fixed::from_int(py),
                                color,
                                opa,
                            );
                        }
                    }
                }
            }
        }
    }
}
