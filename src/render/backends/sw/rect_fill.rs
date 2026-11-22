use super::SwRenderer;
#[cfg(test)]
use crate::render::texture::Texture;
use crate::render::texture::{ColorFormat, TexturePixelsMut};
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
        let phys_clip = self.physical_draw_clip(*clip);
        let radius = radius * self.viewport.scale();
        let area = &phys_area;
        let clip = &phys_clip;

        let screen = Rect::new(0, 0, self.target.width(), self.target.height());
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let r = radius.min(area.w / 2).min(area.h / 2);
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let effective_opa = ((color.a as u16 * opa as u16) / 255) as u8;
        let opa_norm =
            Fixed::from_int(effective_opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        if clip_mask.is_some() {
            self.fill_rect_masked(
                area,
                px_x0,
                px_y0,
                px_x1,
                px_y1,
                r,
                color,
                effective_opa,
                opa_norm,
            );
            return;
        }

        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        if area.is_aligned() && r == Fixed::ZERO {
            crate::trace_span!("sw.fill_aligned");
            fill_axis_aligned(
                &mut target,
                px_x0,
                px_y0,
                px_x1,
                px_y1,
                color,
                effective_opa,
            );
            return;
        }

        // Axis-aligned + rounded: solid fill the inner cross, only run
        // SDF coverage on the four r×r corner bboxes. r is ceil'd so
        // the SDF region fully covers the curved boundary.
        if area.is_aligned() && r > Fixed::ZERO {
            let r_px = r.ceil().to_int();
            let area_x0 = area.x.to_int();
            let area_y0 = area.y.to_int();
            let area_x1 = (area.x + area.w).to_int();
            let area_y1 = (area.y + area.h).to_int();
            // Inner-cross requires the area to be wider/taller than 2r;
            // otherwise the four corner bboxes overlap and there's no
            // straight section to short-circuit.
            if area_x1 - area_x0 > 2 * r_px && area_y1 - area_y0 > 2 * r_px {
                crate::trace_span!("sw.fill_aligned_rounded");
                let inner_x0 = area_x0 + r_px;
                let inner_x1 = area_x1 - r_px;
                let inner_y0 = area_y0 + r_px;
                let inner_y1 = area_y1 - r_px;
                fill_axis_aligned(
                    &mut target,
                    inner_x0.max(px_x0),
                    area_y0.max(px_y0),
                    inner_x1.min(px_x1),
                    inner_y0.min(px_y1),
                    color,
                    effective_opa,
                );
                fill_axis_aligned(
                    &mut target,
                    inner_x0.max(px_x0),
                    inner_y1.max(px_y0),
                    inner_x1.min(px_x1),
                    area_y1.min(px_y1),
                    color,
                    effective_opa,
                );
                fill_axis_aligned(
                    &mut target,
                    area_x0.max(px_x0),
                    inner_y0.max(px_y0),
                    area_x1.min(px_x1),
                    inner_y1.min(px_y1),
                    color,
                    effective_opa,
                );
                let corners = [
                    (area_x0, area_y0, inner_x0, inner_y0), // TL
                    (inner_x1, area_y0, area_x1, inner_y0), // TR
                    (area_x0, inner_y1, inner_x0, area_y1), // BL
                    (inner_x1, inner_y1, area_x1, area_y1), // BR
                ];
                for (cx0, cy0, cx1, cy1) in corners {
                    let bx0 = cx0.max(px_x0);
                    let by0 = cy0.max(px_y0);
                    let bx1 = cx1.min(px_x1);
                    let by1 = cy1.min(px_y1);
                    for py in by0..by1 {
                        for px in bx0..bx1 {
                            let cov = rounded_rect_coverage(
                                Fixed::from_int(px) - area.x,
                                Fixed::from_int(py) - area.y,
                                area.w,
                                area.h,
                                r,
                            );
                            let final_opa = (cov * opa_norm).map01(255).to_int() as u8;
                            if final_opa > 0 {
                                target.blend_pixel_int(px, py, color, final_opa);
                            }
                        }
                    }
                }
                return;
            }
        }

        // Y-fractional fast path: a row scrolled to a sub-pixel Y
        // offset is X-axis-aligned with a real integer-aligned middle
        // band, so split the fill into top AA strip + memcpy mid +
        // bottom AA strip and skip per-pixel coverage on the bulk.
        let x_is_aligned = area.x.fract() == Fixed::ZERO && area.w.fract() == Fixed::ZERO;
        if x_is_aligned && r == Fixed::ZERO && area.h >= Fixed::from_int(3) {
            let area_x_end_int = (area.x + area.w).to_int();
            let area_x_int = area.x.to_int();
            let mid_y0_aligned = area.y.ceil().to_int();
            let mid_y1_aligned = (area.y + area.h).floor().to_int();
            if mid_y1_aligned > mid_y0_aligned {
                let clip_x0 = area_x_int.max(px_x0);
                let clip_x1 = area_x_end_int.min(px_x1);
                let clip_mid_y0 = mid_y0_aligned.max(px_y0);
                let clip_mid_y1 = mid_y1_aligned.min(px_y1);
                if clip_x1 > clip_x0 && clip_mid_y1 > clip_mid_y0 {
                    crate::trace_span!("sw.fill_aa_split_mid");
                    fill_axis_aligned(
                        &mut target,
                        clip_x0,
                        clip_mid_y0,
                        clip_x1,
                        clip_mid_y1,
                        color,
                        effective_opa,
                    );
                    let aa_top_y0 = px_y0;
                    let aa_top_y1 = mid_y0_aligned.min(px_y1);
                    let aa_bot_y0 = mid_y1_aligned.max(px_y0);
                    let aa_bot_y1 = px_y1;
                    if aa_top_y1 > aa_top_y0 {
                        crate::trace_span!("sw.fill_aa_split_top");
                        fill_aa_band(
                            &mut target,
                            *area,
                            aa_top_y0,
                            aa_top_y1,
                            px_x0,
                            px_x1,
                            color,
                            effective_opa,
                            opa_norm,
                        );
                    }
                    if aa_bot_y1 > aa_bot_y0 {
                        crate::trace_span!("sw.fill_aa_split_bot");
                        fill_aa_band(
                            &mut target,
                            *area,
                            aa_bot_y0,
                            aa_bot_y1,
                            px_x0,
                            px_x1,
                            color,
                            effective_opa,
                            opa_norm,
                        );
                    }
                    return;
                }
            }
        }

        crate::trace_span!("sw.fill_aa_loop");
        let area_x_end = area.x + area.w;
        let area_y_end = area.y + area.h;
        // Pixels strictly inside the corner-free zone don't need an SDF
        // call; ceil/floor on the Fixed side gives the integer column /
        // row range. When r == 0 the whole interior is corner-free.
        let r_int = if r > Fixed::ZERO {
            r.ceil()
        } else {
            Fixed::ZERO
        };
        let mid_x0 = (area.x + r_int).ceil().to_int();
        let mid_x1 = (area_x_end - r_int).to_int();
        let mid_y0 = (area.y + r_int).ceil().to_int();
        let mid_y1 = (area_y_end - r_int).to_int();
        let has_corners = r > Fixed::ZERO;
        let opa_full = effective_opa == 255;
        let inner_x0 = mid_x0.max(px_x0);
        let inner_x1 = mid_x1.min(px_x1);
        let inner_y0 = mid_y0.max(px_y0);
        let inner_y1 = mid_y1.min(px_y1);
        let bulk_interior = !has_corners && opa_full && inner_x0 < inner_x1 && inner_y0 < inner_y1;
        if bulk_interior {
            overwrite_rect(&mut target, inner_x0, inner_y0, inner_x1, inner_y1, color);
        }

        // The pixel bounds already intersect the target; reuse that admission for each row.
        let format = target.format();
        let stride = target.stride();
        let pixel_bytes = format.bytes_per_pixel();
        let alpha_mode = target.alpha_mode();
        let pixels = target.bytes_mut();
        for py in px_y0..px_y1 {
            let row_start = py as usize * stride;
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y = if pixel_top >= area.y && pixel_bot <= area_y_end {
                Fixed::ONE
            } else {
                (pixel_bot.min(area_y_end) - pixel_top.max(area.y))
                    .max(Fixed::ZERO)
                    .min(Fixed::ONE)
            };
            let row_full_y = cov_y == Fixed::ONE;
            let in_mid_y = py >= mid_y0 && py < mid_y1;

            let (left_end, right_start) = if bulk_interior && py >= inner_y0 && py < inner_y1 {
                (inner_x0, inner_x1)
            } else {
                (px_x1, px_x1)
            };
            for columns in [px_x0..left_end, right_start..px_x1] {
                for px in columns {
                    let pixel_left = Fixed::from_int(px);
                    let pixel_right = Fixed::from_int(px + 1);
                    let col_full_x = pixel_left >= area.x && pixel_right <= area_x_end;
                    let in_mid_x = px >= mid_x0 && px < mid_x1;

                    // Hot path: straight-zone fully-opaque pixel — skip
                    // every Fixed multiply and the SDF call.
                    if row_full_y && col_full_x && in_mid_y && in_mid_x && opa_full {
                        format.blend_pixel(
                            pixels,
                            row_start + px as usize * pixel_bytes,
                            color,
                            255,
                            alpha_mode,
                        );
                        continue;
                    }

                    let cov_x = if col_full_x {
                        Fixed::ONE
                    } else {
                        (pixel_right.min(area_x_end) - pixel_left.max(area.x))
                            .max(Fixed::ZERO)
                            .min(Fixed::ONE)
                    };
                    let corner_cov = if has_corners && !(in_mid_y && in_mid_x) {
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

                    let final_opa =
                        (cov_x * cov_y * corner_cov * opa_norm).map01(255).to_int() as u8;
                    if final_opa > 0 {
                        format.blend_pixel(
                            pixels,
                            row_start + px as usize * pixel_bytes,
                            color,
                            final_opa,
                            alpha_mode,
                        );
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn fill_rect_masked(
        &mut self,
        area: &Rect,
        px_x0: i32,
        px_y0: i32,
        px_x1: i32,
        py_y1: i32,
        r: Fixed,
        color: &Color,
        _effective_opa: u8,
        opa_norm: Fixed,
    ) {
        let Some(clip_mask) = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice()) else {
            return;
        };
        let target_w = self.target.width() as usize;
        let area_x_end = area.x + area.w;
        let area_y_end = area.y + area.h;
        let has_corners = r > Fixed::ZERO;

        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for py in px_y0..py_y1 {
            let pixel_top = Fixed::from_int(py);
            let pixel_bot = Fixed::from_int(py + 1);
            let cov_y = if pixel_top >= area.y && pixel_bot <= area_y_end {
                Fixed::ONE
            } else {
                (pixel_bot.min(area_y_end) - pixel_top.max(area.y))
                    .max(Fixed::ZERO)
                    .min(Fixed::ONE)
            };
            if cov_y <= Fixed::ZERO {
                continue;
            }
            let row_mask_off = py as usize * target_w;
            for px in px_x0..px_x1 {
                let clip_alpha = clip_mask[row_mask_off + px as usize];
                if clip_alpha == 0 {
                    continue;
                }
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let cov_x = if pixel_left >= area.x && pixel_right <= area_x_end {
                    Fixed::ONE
                } else {
                    (pixel_right.min(area_x_end) - pixel_left.max(area.x))
                        .max(Fixed::ZERO)
                        .min(Fixed::ONE)
                };
                if cov_x <= Fixed::ZERO {
                    continue;
                }
                let corner_cov = if has_corners {
                    let dx = Fixed::from_int(px) - area.x;
                    let dy = Fixed::from_int(py) - area.y;
                    if dx >= r && dx <= area.w - r && dy >= r && dy <= area.h - r {
                        Fixed::ONE
                    } else {
                        rounded_rect_coverage(dx, dy, area.w, area.h, r)
                    }
                } else {
                    Fixed::ONE
                };
                let cov = cov_x * cov_y * corner_cov * opa_norm;
                let base_alpha = cov.map01(255).to_int() as u8;
                if base_alpha == 0 {
                    continue;
                }
                let final_alpha = ((base_alpha as u16 * clip_alpha as u16 + 127) / 255) as u8;
                if final_alpha > 0 {
                    target.blend_pixel_int(px, py, color, final_alpha);
                }
            }
        }
    }
}

/// AA loop over `[y0, y1)`, used by the X-axis-aligned split. Caller
/// has verified there are no rounded corners; X coverage is implicitly
/// 1.0 so only Y coverage and `effective_opa` contribute.
#[allow(clippy::too_many_arguments)]
fn fill_aa_band(
    target: &mut TexturePixelsMut<'_>,
    area: Rect,
    y0: i32,
    y1: i32,
    x0: i32,
    x1: i32,
    color: &Color,
    effective_opa: u8,
    opa_norm: Fixed,
) {
    let area_y_end = area.y + area.h;
    for py in y0..y1 {
        let pixel_top = Fixed::from_int(py);
        let pixel_bot = Fixed::from_int(py + 1);
        let cov_y = if pixel_top >= area.y && pixel_bot <= area_y_end {
            Fixed::ONE
        } else {
            (pixel_bot.min(area_y_end) - pixel_top.max(area.y))
                .max(Fixed::ZERO)
                .min(Fixed::ONE)
        };
        if cov_y <= Fixed::ZERO {
            continue;
        }
        let final_opa = if cov_y == Fixed::ONE && effective_opa == 255 {
            255
        } else {
            (cov_y * opa_norm).map01(255).to_int() as u8
        };
        if final_opa == 0 {
            continue;
        }
        for px in x0..x1 {
            target.blend_pixel_int(px, py, color, final_opa);
        }
    }
}

pub(super) fn fill_axis_aligned(
    target: &mut TexturePixelsMut<'_>,
    px_x0: i32,
    px_y0: i32,
    px_x1: i32,
    px_y1: i32,
    color: &Color,
    opa: u8,
) {
    // Caller may pass an empty sub-rect when clip only overlaps one of
    // the rounded fast path's bands; the negative width otherwise
    // underflows `(px_x1 - px_x0) as usize` and panics on slice access.
    if px_x1 <= px_x0 || px_y1 <= px_y0 {
        return;
    }
    // Blend mode + partially transparent source must accumulate
    // dst.a via source-over per pixel; the memcpy fast path below
    // would clobber the destination's existing alpha and break the
    // silhouette for any downstream sampler.
    let blend_aware =
        target.alpha_mode() == crate::render::texture::AlphaMode::Blend && color.a < 255;
    if opa == 255 && !blend_aware {
        overwrite_rect(target, px_x0, px_y0, px_x1, px_y1, color);
    } else {
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                target.blend_pixel_int(px, py, color, opa);
            }
        }
    }
}

/// Replaces an already clipped pixel rectangle, including source alpha.
pub(super) fn overwrite_rect(
    target: &mut TexturePixelsMut<'_>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: &Color,
) {
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let format = target.format();
    let stride = target.stride();
    let row_bytes = (x1 - x0) as usize * format.bytes_per_pixel();
    let pixel = format.pack(color).to_le_bytes();
    let buf = target.bytes_mut();
    match format {
        ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
            fill_first_row_then_replicate::<2>(
                buf,
                stride,
                x0,
                y0,
                y1,
                row_bytes,
                [pixel[0], pixel[1]],
            );
        }
        ColorFormat::RGB888 => {
            fill_first_row_then_replicate::<3>(
                buf,
                stride,
                x0,
                y0,
                y1,
                row_bytes,
                [pixel[0], pixel[1], pixel[2]],
            );
        }
        ColorFormat::RGBA8888 | ColorFormat::BGRA8888 => {
            fill_first_row_then_replicate::<4>(buf, stride, x0, y0, y1, row_bytes, pixel);
        }
    }
}

#[inline]
fn fill_first_row_then_replicate<const BPP: usize>(
    buf: &mut [u8],
    stride: usize,
    px_x0: i32,
    px_y0: i32,
    px_y1: i32,
    row_bytes: usize,
    pixel: [u8; BPP],
) {
    if px_y0 >= px_y1 {
        return;
    }
    let first_start = px_y0 as usize * stride + px_x0 as usize * BPP;
    let first_row = &mut buf[first_start..first_start + row_bytes];
    for chunk in first_row.chunks_exact_mut(BPP) {
        chunk.copy_from_slice(&pixel);
    }
    let remaining_rows = (px_y1 - px_y0 - 1) as usize;
    if remaining_rows == 0 {
        return;
    }
    let end = first_start + remaining_rows * stride + row_bytes;
    let (head, tail) = buf[..end].split_at_mut(first_start + stride);
    let source = &head[first_start..first_start + row_bytes];
    for row in tail.chunks_mut(stride) {
        row[..row_bytes].copy_from_slice(source);
    }
}

pub(super) fn rect_pixel_coverage(rect: Rect, x: i32, y: i32) -> Fixed {
    pixel_axis_coverage(rect.x, rect.x + rect.w, x)
        * pixel_axis_coverage(rect.y, rect.y + rect.h, y)
}

pub(super) fn pixel_axis_coverage(start: Fixed, end: Fixed, pixel: i32) -> Fixed {
    let pixel = Fixed::from_int(pixel);
    let left = pixel.max(start);
    let right = (pixel + Fixed::ONE).min(end);
    if left < right {
        right - left
    } else {
        Fixed::ZERO
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

    // 4×4 supersample on the AA boundary; single-sample looks flat-topped
    // because all the curvature collapses into one pixel row. Inside r-1
    // and outside r+1 short-circuit so only the ~2-px ring pays the 16
    // samples.
    let dx_pc = px - cx + Fixed::ONE / 2;
    let dy_pc = py - cy + Fixed::ONE / 2;
    let dist_sq = dx_pc * dx_pc + dy_pc * dy_pc;
    let r_sq = r * r;
    let r_inner = r - Fixed::ONE;
    if r_inner > Fixed::ZERO {
        let r_inner_sq = r_inner * r_inner;
        if dist_sq <= r_inner_sq {
            return Fixed::ONE;
        }
    }
    let r_outer = r + Fixed::ONE;
    let r_outer_sq = r_outer * r_outer;
    if dist_sq >= r_outer_sq {
        return Fixed::ZERO;
    }
    let mut hits: i32 = 0;
    let step = Fixed::ONE / 4;
    let half_step = step / 2;
    let base_x = px - cx + half_step;
    let base_y = py - cy + half_step;
    for sy in 0..4 {
        let dy = base_y + step * Fixed::from_int(sy);
        for sx in 0..4 {
            let dx = base_x + step * Fixed::from_int(sx);
            if dx * dx + dy * dy <= r_sq {
                hits += 1;
            }
        }
    }
    Fixed::from_int(hits) / 16
}

#[cfg(all(test, feature = "std"))]
#[path = "../../../../tests/unit/render/backends/sw/rect_fill/corner_check.rs"]
mod corner_check;

#[cfg(all(test, feature = "std"))]
#[path = "../../../../tests/unit/render/backends/sw/rect_fill/clip_mask_check.rs"]
mod clip_mask_check;
