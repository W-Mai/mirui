use super::SwRenderer;
use super::rect_fill::{fill_axis_aligned, rect_pixel_coverage, rounded_rect_coverage};
use crate::types::{Color, Fixed, Rect};

impl SwRenderer<'_> {
    pub(super) fn stroke_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.physical_draw_clip(*clip);
        let width = width * self.viewport.scale();
        if width <= Fixed::ZERO || opa == 0 {
            return;
        }
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
        let bw = width;
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        let inner_r = (r - bw).max(Fixed::ZERO);
        let inner_w = (area.w - bw * 2).max(Fixed::ZERO);
        let inner_h = (area.h - bw * 2).max(Fixed::ZERO);

        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target_w = self.target.width as usize;

        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        if r == Fixed::ZERO && clip_mask.is_none() && area.is_aligned() && bw.is_integer() {
            let base_opa = opa_norm.map01(255).to_int() as u8;
            let effective_opa = ((u16::from(color.a) * u16::from(base_opa)) / 255) as u8;
            let x0 = (area.x + bw).ceil().to_int().clamp(px_x0, px_x1);
            let y0 = (area.y + bw).ceil().to_int().clamp(px_y0, px_y1);
            let x1 = (area.x + bw + inner_w).ceil().to_int().clamp(px_x0, px_x1);
            let y1 = (area.y + bw + inner_h).ceil().to_int().clamp(px_y0, px_y1);
            if x0 >= x1 || y0 >= y1 {
                fill_axis_aligned(
                    &mut target,
                    px_x0,
                    px_y0,
                    px_x1,
                    px_y1,
                    color,
                    effective_opa,
                );
            } else {
                for (left, top, right, bottom) in [
                    (px_x0, px_y0, px_x1, y0),
                    (px_x0, y1, px_x1, px_y1),
                    (px_x0, y0, x0, y1),
                    (x1, y0, px_x1, y1),
                ] {
                    fill_axis_aligned(&mut target, left, top, right, bottom, color, effective_opa);
                }
            }
            return;
        }
        let inner = Rect::new(area.x + bw, area.y + bw, inner_w, inner_h);
        let inner_x0 = inner.x.ceil().to_int().clamp(px_x0, px_x1);
        let inner_x1 = (inner.x + inner.w).floor().to_int().clamp(px_x0, px_x1);
        let inner_y0 = inner.y.ceil().to_int();
        let inner_y1 = (inner.y + inner.h).floor().to_int();
        // Pixels wholly inside the inner rect and outside its corner boxes
        // have full inner coverage, so the border leaves them untouched.
        // Skipping them turns a full-area scan into a scan of the ring.
        let straight_y0 = (inner.y + inner_r).ceil().to_int();
        let straight_y1 = (inner.y + inner.h - inner_r).ceil().to_int();
        let cross_x0 = (inner.x + inner_r)
            .ceil()
            .to_int()
            .clamp(inner_x0, inner_x1);
        let cross_x1 = (inner.x + inner.w - inner_r)
            .ceil()
            .to_int()
            .clamp(inner_x0, inner_x1);
        for py in px_y0..px_y1 {
            let row_mask_off = py as usize * target_w;
            let (skip_start, skip_end) = if py < inner_y0 || py >= inner_y1 {
                (px_x1, px_x1)
            } else if py >= straight_y0 && py < straight_y1 {
                (inner_x0, inner_x1)
            } else {
                (cross_x0, cross_x1)
            };
            let spans = if skip_start < skip_end {
                [(px_x0, skip_start), (skip_end, px_x1)]
            } else {
                [(px_x0, px_x1), (px_x1, px_x1)]
            };
            for (start, end) in spans {
                for px in start..end {
                    let rel_x = Fixed::from_int(px) - area.x;
                    let rel_y = Fixed::from_int(py) - area.y;

                    let outer_cov = rect_pixel_coverage(*area, px, py)
                        * rounded_rect_coverage(rel_x, rel_y, area.w, area.h, r);
                    if outer_cov == Fixed::ZERO {
                        continue;
                    }

                    let inner_rel_x = rel_x - bw;
                    let inner_rel_y = rel_y - bw;
                    let inner_cov = rect_pixel_coverage(inner, px, py)
                        * rounded_rect_coverage(
                            inner_rel_x,
                            inner_rel_y,
                            inner_w,
                            inner_h,
                            inner_r,
                        );

                    let border_cov = (outer_cov - inner_cov).max(Fixed::ZERO);
                    let base_opa = (border_cov * opa_norm).map01(255).to_int() as u8;
                    if base_opa == 0 {
                        continue;
                    }
                    let final_opa = match clip_mask {
                        Some(m) => {
                            let clip_alpha = m[row_mask_off + px as usize];
                            if clip_alpha == 0 {
                                continue;
                            }
                            ((base_opa as u16 * clip_alpha as u16 + 127) / 255) as u8
                        }
                        None => base_opa,
                    };
                    if final_opa > 0 {
                        target.blend_pixel(
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
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/rect_stroke.rs"]
mod tests;
