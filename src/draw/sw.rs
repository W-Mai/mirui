use crate::types::{Color, Point, Rect};

use super::command::DrawCommand;
use super::renderer::Renderer;

pub struct SwRenderer<'a> {
    buf: &'a mut [u8],
    width: u32,
    height: u32,
    pub scale: u16,
}

impl<'a> SwRenderer<'a> {
    pub fn new(buf: &'a mut [u8], width: u32, height: u32) -> Self {
        Self {
            buf,
            width,
            height,
            scale: 1,
        }
    }

    fn put_pixel(&mut self, x: i32, y: i32, color: &Color, opa: u8) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let idx = ((y as u32 * self.width + x as u32) * 4) as usize;
        if idx + 3 >= self.buf.len() {
            return;
        }
        let a = ((color.a as u16) * (opa as u16) / 255) as u8;
        if a == 255 {
            self.buf[idx] = color.r;
            self.buf[idx + 1] = color.g;
            self.buf[idx + 2] = color.b;
            self.buf[idx + 3] = 255;
        } else if a > 0 {
            let inv = 255 - a as u16;
            self.buf[idx] = ((color.r as u16 * a as u16 + self.buf[idx] as u16 * inv) / 255) as u8;
            self.buf[idx + 1] =
                ((color.g as u16 * a as u16 + self.buf[idx + 1] as u16 * inv) / 255) as u8;
            self.buf[idx + 2] =
                ((color.b as u16 * a as u16 + self.buf[idx + 2] as u16 * inv) / 255) as u8;
            self.buf[idx + 3] = 255;
        }
    }

    /// Check if a pixel at (px, py) relative to rect origin is inside the rounded rect
    fn is_in_rounded_rect(px: i32, py: i32, w: u16, h: u16, r: u16) -> bool {
        let r = r as i32;
        let w = w as i32;
        let h = h as i32;

        // Four corner checks
        if px < r && py < r {
            // top-left corner
            let dx = r - px - 1;
            let dy = r - py - 1;
            return dx * dx + dy * dy <= r * r;
        }
        if px >= w - r && py < r {
            // top-right corner
            let dx = px - (w - r);
            let dy = r - py - 1;
            return dx * dx + dy * dy <= r * r;
        }
        if px < r && py >= h - r {
            // bottom-left corner
            let dx = r - px - 1;
            let dy = py - (h - r);
            return dx * dx + dy * dy <= r * r;
        }
        if px >= w - r && py >= h - r {
            // bottom-right corner
            let dx = px - (w - r);
            let dy = py - (h - r);
            return dx * dx + dy * dy <= r * r;
        }
        true
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, opa: u8, radius: u16) {
        use crate::types::Fixed;

        let screen = Rect::new(0, 0, self.width, self.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let area_w = area.w.to_int() as u16;
        let area_h = area.h.to_int() as u16;
        let r = radius.min(area_w / 2).min(area_h / 2);

        // Iterate over integer pixels covered by draw_area
        let px_x0 = draw_area.x.to_int();
        let px_y0 = draw_area.y.to_int();
        let px_x1 = (draw_area.x + draw_area.w).to_int();
        let px_y1 = (draw_area.y + draw_area.h).to_int();

        for py in px_y0..px_y1 {
            // Vertical coverage: how much of this pixel row is inside area?
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y_top = pixel_top.max(area.y);
            let cov_y_bot = pixel_bot.min(area.y + area.h);
            let cov_y = ((cov_y_bot - cov_y_top).raw().clamp(0, 256)) as u16;

            for px in px_x0..px_x1 {
                // Rounded rect check (integer level)
                if r > 0
                    && !Self::is_in_rounded_rect(
                        px - area.x.to_int(),
                        py - area.y.to_int(),
                        area_w,
                        area_h,
                        r,
                    )
                {
                    continue;
                }

                // Horizontal coverage
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let cov_x_left = pixel_left.max(area.x);
                let cov_x_right = pixel_right.min(area.x + area.w);
                let cov_x = ((cov_x_right - cov_x_left).raw().clamp(0, 256)) as u16;

                // Combined coverage (0..256 * 0..256 -> 0..255)
                let cov = (cov_x as u32 * cov_y as u32 / 256).min(255) as u8;
                let final_opa = (opa as u16 * cov as u16 / 255) as u8;
                if final_opa > 0 {
                    self.put_pixel(px, py, color, final_opa);
                }
            }
        }
    }

    fn draw_border(
        &mut self,
        area: &Rect,
        clip: &Rect,
        color: &Color,
        width: u16,
        radius: u16,
        opa: u8,
    ) {
        let screen = Rect::new(0, 0, self.width, self.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let area_x = area.x.to_int();
        let area_y = area.y.to_int();
        let area_w = area.w.to_int() as u16;
        let area_h = area.h.to_int() as u16;
        let da_x = draw_area.x.to_int();
        let da_y = draw_area.y.to_int();
        let da_w = draw_area.w.to_int();
        let da_h = draw_area.h.to_int();

        let r = radius.min(area_w / 2).min(area_h / 2);
        let bw = width as i32;

        for row in 0..da_h {
            let y = da_y + row;
            let py = y - area_y;
            for col in 0..da_w {
                let x = da_x + col;
                let px = x - area_x;

                // Must be inside outer rounded rect
                if r > 0 && !Self::is_in_rounded_rect(px, py, area_w, area_h, r) {
                    continue;
                }

                // Must be outside inner rect (border region only)
                let inner_r = if r as i32 > bw {
                    (r as i32 - bw) as u16
                } else {
                    0
                };
                let inner_w = if area_w as i32 > 2 * bw {
                    area_w - 2 * width
                } else {
                    0
                };
                let inner_h = if area_h as i32 > 2 * bw {
                    area_h - 2 * width
                } else {
                    0
                };

                let ipx = px - bw;
                let ipy = py - bw;

                if ipx >= 0
                    && ipy >= 0
                    && ipx < inner_w as i32
                    && ipy < inner_h as i32
                    && (inner_r == 0
                        || Self::is_in_rounded_rect(ipx, ipy, inner_w, inner_h, inner_r))
                {
                    continue;
                }

                self.put_pixel(x, y, color, opa);
            }
        }
    }
    fn draw_label(
        &mut self,
        pos: &crate::types::Point,
        text: &[u8],
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        use super::font::{CHAR_H, CHAR_W, glyph};
        let s = self.scale as i32;
        let clip_x = clip.x.to_int();
        let clip_y = clip.y.to_int();
        let clip_x2 = clip_x + clip.w.to_int();
        let clip_y2 = clip_y + clip.h.to_int();
        let mut cx = pos.x;
        let cy = pos.y;
        for &ch in text {
            let bitmap = glyph(ch);
            for row in 0..CHAR_H as i32 {
                let byte = bitmap[row as usize];
                for col in 0..CHAR_W as i32 {
                    if byte & (0x80 >> col) != 0 {
                        for dy in 0..s {
                            for dx in 0..s {
                                let px = cx + col * s + dx;
                                let py = cy + row * s + dy;
                                if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                                    self.put_pixel(px, py, color, opa);
                                }
                            }
                        }
                    }
                }
            }
            cx += CHAR_W as i32 * s;
        }
    }

    fn blit_rgba(&mut self, pos: &Point, data: &[u8], width: u16, height: u16, clip: &Rect) {
        let s = self.scale as i32;
        let clip_x = clip.x.to_int();
        let clip_y = clip.y.to_int();
        let clip_x2 = clip_x + clip.w.to_int();
        let clip_y2 = clip_y + clip.h.to_int();
        for row in 0..height as i32 {
            for col in 0..width as i32 {
                let src_idx = ((row * width as i32 + col) * 4) as usize;
                if src_idx + 3 >= data.len() {
                    break;
                }
                let a = data[src_idx + 3] as u16;
                if a == 0 {
                    continue;
                }
                for dy in 0..s {
                    for dx in 0..s {
                        let px = pos.x + col * s + dx;
                        let py = pos.y + row * s + dy;
                        if px < clip_x || px >= clip_x2 || py < clip_y || py >= clip_y2 {
                            continue;
                        }
                        let dst_idx = ((py as u32 * self.width + px as u32) * 4) as usize;
                        if dst_idx + 3 >= self.buf.len() {
                            continue;
                        }
                        if a == 255 {
                            self.buf[dst_idx] = data[src_idx];
                            self.buf[dst_idx + 1] = data[src_idx + 1];
                            self.buf[dst_idx + 2] = data[src_idx + 2];
                            self.buf[dst_idx + 3] = 255;
                        } else {
                            let inv = 255 - a;
                            self.buf[dst_idx] = ((data[src_idx] as u16 * a
                                + self.buf[dst_idx] as u16 * inv)
                                / 255) as u8;
                            self.buf[dst_idx + 1] = ((data[src_idx + 1] as u16 * a
                                + self.buf[dst_idx + 1] as u16 * inv)
                                / 255) as u8;
                            self.buf[dst_idx + 2] = ((data[src_idx + 2] as u16 * a
                                + self.buf[dst_idx + 2] as u16 * inv)
                                / 255) as u8;
                            self.buf[dst_idx + 3] = 255;
                        }
                    }
                }
            }
        }
    }
}

impl Renderer for SwRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                opa,
                radius,
            } => {
                self.fill_rect(area, clip, color, *opa, *radius);
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
            } => {
                self.draw_border(area, clip, color, *width, *radius, *opa);
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
            } => {
                self.draw_label(pos, text, clip, color, *opa);
            }
            DrawCommand::Blit {
                pos,
                data,
                width,
                height,
            } => {
                self.blit_rgba(pos, data, *width, *height, clip);
            }
            _ => {}
        }
    }

    fn flush(&mut self) {}
}
