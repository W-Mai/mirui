use super::SwRenderer;
use crate::draw::texture::{ColorFormat, Texture};
use crate::types::{Color, Fixed, Rect};

impl SwRenderer<'_> {
    pub(super) fn fill_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let radius = radius * self.viewport.scale();
        let area = &phys_area;
        let clip = &phys_clip;

        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        if area.is_aligned() && r == Fixed::ZERO {
            fill_axis_aligned(&mut self.target, px_x0, px_y0, px_x1, px_y1, color, opa);
            return;
        }

        for py in px_y0..px_y1 {
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y = (pixel_bot.min(area.y + area.h) - pixel_top.max(area.y))
                .max(Fixed::ZERO)
                .min(Fixed::ONE);

            for px in px_x0..px_x1 {
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let cov_x = (pixel_right.min(area.x + area.w) - pixel_left.max(area.x))
                    .max(Fixed::ZERO)
                    .min(Fixed::ONE);

                let corner_cov = if r > Fixed::ZERO {
                    rounded_rect_coverage(
                        Fixed::from_int(px) - area.x,
                        Fixed::from_int(py) - area.y,
                        area.w,
                        area.h,
                        r,
                    )
                } else {
                    Fixed::ONE
                };

                let final_opa = (cov_x * cov_y * corner_cov * opa_norm).map01(255).to_int() as u8;
                if final_opa > 0 {
                    self.target.blend_pixel(
                        Fixed::from_int(px),
                        Fixed::from_int(py),
                        color,
                        final_opa,
                    );
                }
            }
        }
    }
}

fn fill_axis_aligned(
    target: &mut Texture,
    px_x0: i32,
    px_y0: i32,
    px_x1: i32,
    px_y1: i32,
    color: &Color,
    opa: u8,
) {
    if opa == 255 {
        let bpp = target.format.bytes_per_pixel();
        let stride = target.stride;
        let buf = target.buf.as_mut_slice();
        match target.format {
            ColorFormat::ARGB8888 => {
                let pixel = [color.r, color.g, color.b, color.a];
                for py in px_y0..px_y1 {
                    let row_start = py as usize * stride + px_x0 as usize * bpp;
                    for px in 0..(px_x1 - px_x0) as usize {
                        let i = row_start + px * 4;
                        buf[i..i + 4].copy_from_slice(&pixel);
                    }
                }
            }
            ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
                let px16 = ((color.r as u16 >> 3) << 11)
                    | ((color.g as u16 >> 2) << 5)
                    | (color.b as u16 >> 3);
                let pixel = if target.format == ColorFormat::RGB565Swapped {
                    [(px16 >> 8) as u8, px16 as u8]
                } else {
                    [px16 as u8, (px16 >> 8) as u8]
                };
                for py in px_y0..px_y1 {
                    let row_start = py as usize * stride + px_x0 as usize * bpp;
                    for px in 0..(px_x1 - px_x0) as usize {
                        let i = row_start + px * 2;
                        buf[i..i + 2].copy_from_slice(&pixel);
                    }
                }
            }
            _ => {
                for py in px_y0..px_y1 {
                    for px in px_x0..px_x1 {
                        target.set_pixel(px, py, color);
                    }
                }
            }
        }
    } else {
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                target.blend_pixel_int(px, py, color, opa);
            }
        }
    }
}

pub(super) fn rounded_rect_coverage(px: Fixed, py: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Fixed {
    if r == Fixed::ZERO {
        return Fixed::ONE;
    }

    let (cx, cy) = if px < r && py < r {
        (r, r)
    } else if px >= w - r && py < r {
        (w - r, r)
    } else if px < r && py >= h - r {
        (r, h - r)
    } else if px >= w - r && py >= h - r {
        (w - r, h - r)
    } else {
        return Fixed::ONE;
    };

    let dx = px - cx + Fixed::ONE / 2;
    let dy = py - cy + Fixed::ONE / 2;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq <= r * r {
        Fixed::ONE
    } else {
        let dist = dist_sq.sqrt();
        let overshoot = dist - r;
        if overshoot >= Fixed::ONE {
            Fixed::ZERO
        } else {
            Fixed::ONE - overshoot
        }
    }
}
