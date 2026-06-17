use super::SwRenderer;
use crate::render::font::Font;
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
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_clip.pixel_bounds();
        let (mut cx, cy) = phys_pos.floor();
        let metrics = font.metrics();
        let char_h = metrics.line_height as i32;
        for &ch in text {
            let Some(g) = font.glyph(ch as char) else {
                continue;
            };
            let advance = g.advance as i32 * scale;
            for row in 0..char_h.min(g.bitmap.len() as i32) {
                let byte = g.bitmap[row as usize];
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
            cx += advance;
        }
    }
}
