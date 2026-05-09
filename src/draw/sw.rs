use crate::types::{Color, Fixed, Point, Rect};

use super::command::DrawCommand;
use super::renderer::Renderer;

pub struct SwRenderer<'a> {
    buf: &'a mut [u8],
    width: u32,
    height: u32,
    pub scale: Fixed,
}

impl<'a> SwRenderer<'a> {
    pub fn new(buf: &'a mut [u8], width: u32, height: u32) -> Self {
        Self {
            buf,
            width,
            height,
            scale: Fixed::ONE,
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

    /// Compute coverage (0..Fixed::ONE) for a pixel at (px, py) relative to rect origin
    /// in a rounded rect of size (w, h) with corner radius r.
    /// Returns Fixed::ONE for fully inside, Fixed::ZERO for fully outside,
    /// and a fractional value for edge pixels (anti-aliased).
    fn rounded_rect_coverage(px: Fixed, py: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Fixed {
        if r == Fixed::ZERO {
            return Fixed::ONE;
        }

        // Determine which corner (if any) this pixel is in
        let (cx, cy) = if px < r && py < r {
            (r, r) // top-left
        } else if px >= w - r && py < r {
            (w - r, r) // top-right
        } else if px < r && py >= h - r {
            (r, h - r) // bottom-left
        } else if px >= w - r && py >= h - r {
            (w - r, h - r) // bottom-right
        } else {
            return Fixed::ONE; // not in a corner region
        };

        // Distance from pixel center to corner center
        let dx = px - cx + Fixed::from_raw(128); // +0.5 for pixel center
        let dy = py - cy + Fixed::from_raw(128);
        let dist_sq = dx * dx + dy * dy;
        let r_sq = r * r;

        if dist_sq <= r_sq {
            // Fully inside the rounded corner
            Fixed::ONE
        } else {
            // Anti-alias: compute how far outside we are (in pixels)
            // Use linear falloff over 1px transition band
            let dist = dist_sq.sqrt();
            let overshoot = dist - r;
            if overshoot >= Fixed::ONE {
                Fixed::ZERO
            } else {
                Fixed::ONE - overshoot
            }
        }
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, opa: u8, radius: Fixed) {
        let screen = Rect::new(0, 0, self.width, self.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);

        // Fast path: integer-aligned rect with no rounded corners
        let is_aligned = area.is_aligned() && r == Fixed::ZERO;

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();

        if is_aligned {
            for py in px_y0..px_y1 {
                for px in px_x0..px_x1 {
                    self.put_pixel(px, py, color, opa);
                }
            }
            return;
        }

        for py in px_y0..px_y1 {
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y_top = pixel_top.max(area.y);
            let cov_y_bot = pixel_bot.min(area.y + area.h);
            let cov_y = ((cov_y_bot - cov_y_top).raw().clamp(0, 256)) as u16;

            for px in px_x0..px_x1 {
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let cov_x_left = pixel_left.max(area.x);
                let cov_x_right = pixel_right.min(area.x + area.w);
                let cov_x = ((cov_x_right - cov_x_left).raw().clamp(0, 256)) as u16;

                // Edge coverage
                let edge_cov = (cov_x as u32 * cov_y as u32 / 256).min(255) as u8;

                // Corner AA coverage
                let corner_cov = if r > Fixed::ZERO {
                    let rel_x = Fixed::from_int(px) - area.x;
                    let rel_y = Fixed::from_int(py) - area.y;
                    Self::rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r)
                } else {
                    Fixed::ONE
                };

                let cov = (edge_cov as u16 * (corner_cov.raw().clamp(0, 256) as u16) / 256) as u8;
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
        width: Fixed,
        radius: Fixed,
        opa: u8,
    ) {
        let screen = Rect::new(0, 0, self.width, self.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let bw = width;

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();

        let inner_r = (r - bw).max(Fixed::ZERO);
        let inner_w = (area.w - bw * 2).max(Fixed::ZERO);
        let inner_h = (area.h - bw * 2).max(Fixed::ZERO);

        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                let rel_x = Fixed::from_int(px) - area.x;
                let rel_y = Fixed::from_int(py) - area.y;

                // Outer coverage
                let outer_cov = Self::rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r);
                if outer_cov == Fixed::ZERO {
                    continue;
                }

                // Inner coverage (hole)
                let inner_rel_x = rel_x - bw;
                let inner_rel_y = rel_y - bw;
                let inner_cov = if inner_rel_x >= Fixed::ZERO
                    && inner_rel_y >= Fixed::ZERO
                    && inner_rel_x < inner_w
                    && inner_rel_y < inner_h
                {
                    Self::rounded_rect_coverage(inner_rel_x, inner_rel_y, inner_w, inner_h, inner_r)
                } else {
                    Fixed::ZERO
                };

                // Border coverage = outer - inner
                let border_cov = (outer_cov - inner_cov).max(Fixed::ZERO);
                let cov = (border_cov.raw().clamp(0, 256)) as u8;
                let final_opa = (opa as u16 * cov as u16 / 255) as u8;
                if final_opa > 0 {
                    self.put_pixel(px, py, color, final_opa);
                }
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
        let s = self.scale.to_int();
        let (clip_x, clip_y, clip_x2, clip_y2) = clip.pixel_bounds();
        let (mut cx, cy) = pos.floor();
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
        let s = self.scale.to_int();
        let (clip_x, clip_y, clip_x2, clip_y2) = clip.pixel_bounds();
        let (base_x, base_y) = pos.floor();
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
                        let px = base_x + col * s + dx;
                        let py = base_y + row * s + dy;
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
            DrawCommand::Blit { pos, texture } => {
                self.blit_rgba(
                    pos,
                    texture.buf.as_slice(),
                    texture.width,
                    texture.height,
                    clip,
                );
            }
            _ => {}
        }
    }

    fn flush(&mut self) {}
}
