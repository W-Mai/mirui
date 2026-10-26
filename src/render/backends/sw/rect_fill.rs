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

        for py in px_y0..px_y1 {
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
                        target.set_pixel(px, py, color);
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
                        // px/py are integers — skip blend_pixel's is_integer dispatch.
                        target.blend_pixel_int(px, py, color, final_opa);
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
mod corner_check {
    extern crate std;
    use super::*;
    use crate::render::canvas::Canvas;
    use std::string::String;
    use std::vec::Vec;

    #[test]
    fn rectangular_overwrites_preserve_pixel_bytes_and_row_padding() {
        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            for width in [1, 2, 3, 7, 15, 16, 17, 31, 33] {
                for padding in [0, 1, 5] {
                    for color in [Color::rgba(0, 0, 0, 0), Color::rgba(213, 37, 129, 177)] {
                        let stride = 37 * format.bytes_per_pixel() + padding;
                        let storage_len = stride * 4 + 37 * format.bytes_per_pixel();
                        let mut actual = std::vec![61; storage_len + 2];
                        let mut expected = actual.clone();
                        for (top, height) in [(1, 1), (1, 3), (2, 3)] {
                            {
                                let mut target =
                                    Texture::new(&mut actual[1..1 + storage_len], 37, 5, format)
                                        .unwrap()
                                        .with_stride(stride)
                                        .unwrap();
                                let mut pixels = target.pixels_mut().unwrap();
                                assert_eq!(
                                    crate::test_support::tracked_allocations(|| overwrite_rect(
                                        &mut pixels,
                                        2,
                                        top,
                                        2 + width,
                                        top + height,
                                        &color,
                                    )),
                                    0
                                );
                                let mut target =
                                    Texture::new(&mut expected[1..1 + storage_len], 37, 5, format)
                                        .unwrap()
                                        .with_stride(stride)
                                        .unwrap();
                                let mut pixels = target.pixels_mut().unwrap();
                                for y in top..top + height {
                                    for x in 2..2 + width {
                                        pixels.set_pixel(x, y, &color);
                                    }
                                }
                            }
                            assert_eq!(
                                actual, expected,
                                "{format:?}, {width}, {height}, {padding}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn separable_pixel_coverage_matches_rectangle_intersection() {
        for x in [-5, -1, 0, 1, 5] {
            for y in [-5, -1, 0, 1, 5] {
                for width in [-2, 0, 1, 3, 9] {
                    for height in [-2, 0, 1, 3, 9] {
                        let area = Rect::new(
                            Fixed::from_ratio(x, 2),
                            Fixed::from_ratio(y, 2),
                            Fixed::from_ratio(width, 2),
                            Fixed::from_ratio(height, 2),
                        );
                        for py in -4..8 {
                            for px in -4..8 {
                                let expected = area
                                    .intersect(&Rect::new(px, py, 1, 1))
                                    .map_or(Fixed::ZERO, |covered| covered.w * covered.h);
                                assert_eq!(
                                    rect_pixel_coverage(area, px, py),
                                    expected,
                                    "area={area:?}, pixel=({px},{py})"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn render_circle(w: i32, h: i32, r: i32) -> Vec<Vec<u8>> {
        let mut buf = std::vec![0u8; (w as usize) * (h as usize) * 4];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(0, 0, w, h);
        let clip = Rect::new(0, 0, w, h);
        backend.fill_rect(
            &rect,
            &clip,
            &Color::rgb(255, 255, 255),
            Fixed::from_int(r),
            255,
        );
        let mut out = std::vec![std::vec![0u8; w as usize]; h as usize];
        for py in 0..h {
            for px in 0..w {
                out[py as usize][px as usize] =
                    backend.target.read_pixels().unwrap().get_pixel(px, py).r;
            }
        }
        out
    }

    fn ascii(grid: &[Vec<u8>]) -> String {
        let mut s = String::from("\n");
        for row in grid {
            for &a in row {
                s.push_str(if a > 200 {
                    "##"
                } else if a > 100 {
                    ".."
                } else if a > 0 {
                    "::"
                } else {
                    "  "
                });
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn dump_32x32_r16() {
        let g = render_circle(32, 32, 16);
        std::eprintln!("{}", ascii(&g));
    }

    #[test]
    fn dump_14x14_r7() {
        let g = render_circle(14, 14, 7);
        std::eprintln!("{}", ascii(&g));
    }

    #[test]
    fn dump_50x50_r25() {
        let g = render_circle(50, 50, 25);
        std::eprintln!("{}", ascii(&g));
    }

    #[test]
    fn dump_8x8_r4() {
        let g = render_circle(8, 8, 4);
        std::eprintln!("{}", ascii(&g));
    }

    #[test]
    fn perf_64x64_r32() {
        // Render-time sanity: 64×64 r=32 takes a stable upper bound across
        // 1000 reps. Catches the >100× regression we hit when sqrt was on
        // every pixel; healthy is ~50 µs/frame on a desktop release build.
        use std::time::Instant;
        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(0, 0, 64, 64);
        let clip = Rect::new(0, 0, 64, 64);
        let t0 = Instant::now();
        for _ in 0..1000 {
            backend.fill_rect(
                &rect,
                &clip,
                &Color::rgb(255, 255, 255),
                Fixed::from_int(32),
                255,
            );
        }
        let elapsed = t0.elapsed();
        let per_frame_us = elapsed.as_secs_f64() * 1e6 / 1000.0;
        std::eprintln!("64x64 r=32: {per_frame_us:.2} µs/frame");
        assert!(
            per_frame_us < 5000.0,
            "corner render too slow: {per_frame_us:.2} µs/frame"
        );
    }

    #[test]
    fn shape_symmetric_horizontal() {
        let g = render_circle(32, 32, 16);
        for (y, row) in g.iter().enumerate() {
            for x in 0..16 {
                let l = row[x];
                let r = row[31 - x];
                assert!(l.abs_diff(r) <= 2, "row {y} x={x}: left {l} vs right {r}",);
            }
        }
    }

    #[test]
    fn shape_symmetric_vertical() {
        let g = render_circle(32, 32, 16);
        for (y, (top, bottom)) in g.iter().zip(g.iter().rev()).take(16).enumerate() {
            for (x, (&t, &b)) in top.iter().zip(bottom).enumerate() {
                assert!(t.abs_diff(b) <= 2, "col {x} y={y}: top {t} vs bot {b}",);
            }
        }
    }

    fn count_full_in_row(row: &[u8]) -> usize {
        row.iter().filter(|&&a| a > 200).count()
    }

    fn render_circle_at(
        w: i32,
        h: i32,
        ox: Fixed,
        oy: Fixed,
        fw: Fixed,
        fh: Fixed,
        r: Fixed,
    ) -> Vec<Vec<u8>> {
        let mut buf = std::vec![0u8; (w as usize) * (h as usize) * 4];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect {
            x: ox,
            y: oy,
            w: fw,
            h: fh,
        };
        let clip = Rect::new(0, 0, w, h);
        backend.fill_rect(&rect, &clip, &Color::rgb(255, 255, 255), r, 255);
        let mut out = std::vec![std::vec![0u8; w as usize]; h as usize];
        for py in 0..h {
            for px in 0..w {
                out[py as usize][px as usize] =
                    backend.target.read_pixels().unwrap().get_pixel(px, py).r;
            }
        }
        out
    }

    #[test]
    fn fractional_origin_rounded_inner_still_solid() {
        // demo-widgets layout often produces fractional area origins
        // (flex SpaceBetween / Center halves a px). Verify the inner
        // straight zone stays fully opaque even when area.x and area.y
        // carry a half-pixel offset.
        let half = Fixed::ONE / 2;
        let ox = Fixed::from_int(2) + half;
        let oy = Fixed::from_int(3) + half;
        let g = render_circle_at(
            48,
            32,
            ox,
            oy,
            Fixed::from_int(40),
            Fixed::from_int(24),
            Fixed::from_int(6),
        );
        // r_int = ceil(6) = 6.
        // mid_x0 = ceil(2.5 + 6) = 9. mid_x1 = (2.5 + 40 - 6).to_int() = 36.
        // mid_y0 = ceil(3.5 + 6) = 10. mid_y1 = (3.5 + 24 - 6).to_int() = 21.
        for (y, row) in g.iter().enumerate().take(21).skip(10) {
            for (x, &alpha) in row.iter().enumerate().take(36).skip(9) {
                assert_eq!(
                    alpha, 255,
                    "fractional-origin inner pixel ({x},{y}) not solid: alpha={alpha}"
                );
            }
        }
        // Fractional edges must still anti-alias — top row of the
        // fractional area must be partially transparent (cov_y < 1).
        for (x, &edge) in g[3].iter().enumerate().take(36).skip(9) {
            assert!(
                edge > 0 && edge < 255,
                "fractional top-edge pixel ({x},3) should AA but got {edge}"
            );
        }
    }

    #[test]
    fn aligned_rounded_widget_inner_solid() {
        // Typical widget: 40×24 r=6. Exercises the axis-aligned + r>0
        // fast path (inner cross + four corner bboxes). The inner cross
        // (away from any corner bbox) must be fully solid (alpha 255).
        let g = render_circle(40, 24, 6);
        // r_px is ceil(6) = 6, so corner bboxes are [0..6) × [0..6) etc.
        // Inner cross x/y range fully solid: x∈[6, 34), y∈[6, 18).
        for (y, row) in g.iter().enumerate().take(18).skip(6) {
            for (x, &alpha) in row.iter().enumerate().take(34).skip(6) {
                assert_eq!(alpha, 255, "inner pixel ({x},{y}) not solid: alpha={alpha}");
            }
        }
        for y in [0, 5, 18, 23] {
            for (x, &alpha) in g[y].iter().enumerate().take(34).skip(6) {
                assert_eq!(
                    alpha, 255,
                    "straight band pixel ({x},{y}) not solid: alpha={alpha}"
                );
            }
        }
        for (y, row) in g.iter().enumerate().take(18).skip(6) {
            for x in [0, 5, 34, 39] {
                assert_eq!(
                    row[x], 255,
                    "straight band pixel ({x},{y}) not solid: alpha={}",
                    row[x]
                );
            }
        }
    }

    #[test]
    fn aligned_rounded_widget_corner_curve() {
        // 40×24 r=6: top-left corner bbox [0..6) × [0..6). Pixel (0,0)
        // must be transparent (it's the curve's outer extreme), pixel
        // (5,5) must be fully solid (it's on the inside of the curve).
        let g = render_circle(40, 24, 6);
        assert_eq!(g[0][0], 0, "TL corner outer pixel should be empty");
        assert_eq!(g[0][39], 0, "TR corner outer pixel should be empty");
        assert_eq!(g[23][0], 0, "BL corner outer pixel should be empty");
        assert_eq!(g[23][39], 0, "BR corner outer pixel should be empty");
        assert_eq!(g[5][5], 255, "TL corner inner pixel should be solid");
        assert_eq!(g[5][34], 255, "TR corner inner pixel should be solid");
        assert_eq!(g[18][5], 255, "BL corner inner pixel should be solid");
        assert_eq!(g[18][34], 255, "BR corner inner pixel should be solid");
    }

    #[test]
    fn aligned_rounded_is_4way_symmetric() {
        // 40×40 r=8 hits the axis-aligned + r>0 fast path. Output must
        // be 4-way symmetric within ±2 alpha steps so corner SDF logic
        // can't quietly pick up a per-corner asymmetry from clipping
        // arithmetic.
        let g = render_circle(40, 40, 8);
        for y in 0..20 {
            for x in 0..20 {
                let tl = g[y][x];
                let tr = g[y][39 - x];
                let bl = g[39 - y][x];
                let br = g[39 - y][39 - x];
                assert!(
                    tl.abs_diff(tr) <= 2 && tl.abs_diff(bl) <= 2 && tl.abs_diff(br) <= 2,
                    "asymmetry at ({x},{y}): TL={tl} TR={tr} BL={bl} BR={br}"
                );
            }
        }
    }

    #[test]
    fn shape_top_row_narrower_than_middle() {
        // Catches the "circle looks like a flat-top pill" regression: the
        // top row of a 32×32 r=16 circle must be visually narrower than
        // the body, by enough that the curvature is perceivable (≥4 px
        // on each side).
        let g = render_circle(32, 32, 16);
        let top = count_full_in_row(&g[0]);
        let mid = count_full_in_row(&g[16]);
        assert!(
            top < mid,
            "top={top} mid={mid} — top row wider/equal to middle"
        );
        assert!(
            mid >= top + 8,
            "top={top} mid={mid} — corner curvature too flat (mid-top={})",
            mid - top
        );
    }

    fn render_with_color(w: i32, h: i32, r: Fixed, color: Color, opa: u8) -> Vec<Vec<Color>> {
        let mut buf = std::vec![0u8; (w as usize) * (h as usize) * 4];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(0, 0, w, h);
        let clip = Rect::new(0, 0, w, h);
        backend.fill_rect(&rect, &clip, &color, r, opa);
        let mut out = std::vec![std::vec![Color::rgba(0,0,0,0); w as usize]; h as usize];
        for py in 0..h {
            for px in 0..w {
                out[py as usize][px as usize] =
                    backend.target.read_pixels().unwrap().get_pixel(px, py);
            }
        }
        out
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "The fixture varies target size, subpixel geometry, and paint independently."
    )]
    fn render_at_with_color(
        w: i32,
        h: i32,
        ox: Fixed,
        oy: Fixed,
        fw: Fixed,
        fh: Fixed,
        r: Fixed,
        color: Color,
        opa: u8,
    ) -> Vec<Vec<Color>> {
        let mut buf = std::vec![0u8; (w as usize) * (h as usize) * 4];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect {
            x: ox,
            y: oy,
            w: fw,
            h: fh,
        };
        let clip = Rect::new(0, 0, w, h);
        backend.fill_rect(&rect, &clip, &color, r, opa);
        let mut out = std::vec![std::vec![Color::rgba(0,0,0,0); w as usize]; h as usize];
        for py in 0..h {
            for px in 0..w {
                out[py as usize][px as usize] =
                    backend.target.read_pixels().unwrap().get_pixel(px, py);
            }
        }
        out
    }

    #[test]
    fn rounded_fill_respects_color_alpha_fast_path() {
        // Half-transparent colour at opa=255 must blend at ~half RGB
        // against an empty canvas (color.a folded into the alpha sent
        // to blend_pixel_int / fill_axis_aligned).
        // 40×24 r=6 + integer origin → axis-aligned + r>0 fast path.
        let half = Color::rgba(255, 255, 255, 128);
        let g = render_with_color(40, 24, Fixed::from_int(6), half, 255);
        // Inner-cross pixel — hits fill_axis_aligned RGBA8888 splat.
        let inner = g[12][20];
        assert!(
            (110..=145).contains(&inner.r),
            "inner pixel r {} should be ~128, not 255",
            inner.r
        );
    }

    #[test]
    fn rounded_fill_respects_color_alpha_aa_loop() {
        // Force the general aa_loop by using a fractional origin: that
        // skips the axis-aligned fast paths and exercises the corner
        // SDF + edge AA branches.
        let half = Color::rgba(255, 255, 255, 128);
        let half_px = Fixed::ONE / 2;
        let g = render_at_with_color(
            48,
            32,
            Fixed::from_int(2) + half_px,
            Fixed::from_int(3) + half_px,
            Fixed::from_int(40),
            Fixed::from_int(24),
            Fixed::from_int(6),
            half,
            255,
        );
        // (15, 15) sits in the straight zone (mid_x=[9,36), mid_y=[10,21))
        // and hits the aa_loop short-circuit set_pixel — but only when
        // opa_full is true. With color.a < 255, opa_full must be false,
        // and the pixel falls through to the Fixed multiply path that
        // writes color.r * effective_opa onto the buffer.
        let inner = g[15][20];
        assert!(
            (110..=145).contains(&inner.r),
            "aa_loop straight-zone pixel r {} should be ~128, not 255",
            inner.r
        );
        // (4, 4) sits inside a corner bbox (mid_x0=9, mid_y0=10), so
        // corner_cov is computed via the SDF and final_opa is
        // cov × effective_opa. Where cov ≈ 1 (well inside the corner
        // disk), r should still be ~128, not 255.
        let corner = g[8][8];
        assert!(
            (90..=145).contains(&corner.r),
            "aa_loop SDF-corner pixel r {} should be near 128, not 255",
            corner.r
        );
    }

    #[test]
    fn rounded_fill_respects_color_alpha_general_loop() {
        // A circle (2r==w==h) doesn't satisfy the inner-cross condition
        // and falls through to the general aa_loop. Different code
        // path from the fractional case above.
        let half = Color::rgba(255, 255, 255, 128);
        let g = render_with_color(32, 32, Fixed::from_int(16), half, 255);
        // Centre of the circle — fully covered, just the alpha fold matters.
        let center = g[16][16];
        assert!(
            (110..=145).contains(&center.r),
            "circle centre r {} should be ~128, not 255",
            center.r
        );
    }

    #[test]
    fn rounded_fill_clip_covers_only_corner() {
        // 40×24 r=6 with clip = (0,0,3,24) leaves only the TL corner
        // bbox visible. Two of the inner-cross fill_axis_aligned calls
        // collapse to negative-width sub-rects; the entry guard must
        // catch that before the usize cast underflows.
        let mut buf = std::vec![0u8; 40 * 24 * 4];
        let tex =
            Texture::new(&mut buf, 40, 24, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(0, 0, 40, 24);
        let clip = Rect::new(0, 0, 3, 24);
        backend.fill_rect(
            &rect,
            &clip,
            &Color::rgb(255, 255, 255),
            Fixed::from_int(6),
            255,
        );
        // The visible vertical strip x∈[0,3) must still draw. (1, 8) is
        // in the middle slab band (away from corners); guard must not
        // have eaten it together with the empty top/bottom bands.
        assert_eq!(
            backend.target.read_pixels().unwrap().get_pixel(1, 8).a,
            255,
            "clipped middle slab pixel (1,8) should be drawn"
        );
        // (2, 4) is inside the TL corner bbox AND inside the clip;
        // SDF coverage should write at least a partial pixel here.
        let corner = backend.target.read_pixels().unwrap().get_pixel(2, 4);
        assert!(
            corner.a > 0,
            "TL corner pixel (2,4) should be at least partially drawn, got a={}",
            corner.a
        );
        // Outside clip — must remain untouched.
        assert_eq!(
            backend.target.read_pixels().unwrap().get_pixel(20, 12).a,
            0,
            "pixel outside clip should not be written"
        );
    }

    #[test]
    fn fractional_square_fill_matches_pixel_coverage_across_formats_and_clips() {
        use crate::render::texture::AlphaMode;

        for format in [
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ] {
            for alpha_mode in [AlphaMode::Blend, AlphaMode::Opaque] {
                for origin in [
                    Fixed::from_ratio(-1, 4),
                    Fixed::HALF,
                    Fixed::from_ratio(11, 4),
                ] {
                    for extent in [
                        Fixed::HALF,
                        Fixed::from_ratio(5, 2),
                        Fixed::from_ratio(23, 4),
                    ] {
                        let area = Rect {
                            x: origin,
                            y: Fixed::from_ratio(5, 4),
                            w: extent,
                            h: Fixed::from_ratio(21, 4),
                        };
                        for clip in [
                            Rect::new(0, 0, 12, 11),
                            Rect::new(1, 2, 3, 3),
                            Rect::new(0, 0, 1, 11),
                            Rect::new(7, 0, 5, 11),
                        ] {
                            let size = 12 * 11 * format.bytes_per_pixel();
                            let mut actual = std::vec![17; size];
                            let mut expected = actual.clone();
                            let mut target = Texture::new(&mut actual, 12, 11, format).unwrap();
                            target.alpha_mode = alpha_mode;
                            let mut reference =
                                Texture::new(&mut expected, 12, 11, format).unwrap();
                            reference.alpha_mode = alpha_mode;
                            let color = Color::rgb(231, 123, 45);
                            let mut renderer = SwRenderer::new(target);
                            let allocations = crate::test_support::tracked_allocations(|| {
                                renderer.fill_rect(&area, &clip, &color, Fixed::ZERO, 255);
                            });
                            assert_eq!(allocations, 0);
                            if let Some(draw) = area.intersect(&clip) {
                                let (x0, y0, x1, y1) = draw.pixel_bounds();
                                let mut pixels = reference.pixels_mut().unwrap();
                                for y in y0..y1 {
                                    for x in x0..x1 {
                                        let cx = (Fixed::from_int(x + 1).min(area.x + area.w)
                                            - Fixed::from_int(x).max(area.x))
                                        .clamp(Fixed::ZERO, Fixed::ONE);
                                        let cy = (Fixed::from_int(y + 1).min(area.y + area.h)
                                            - Fixed::from_int(y).max(area.y))
                                        .clamp(Fixed::ZERO, Fixed::ONE);
                                        if cx == Fixed::ONE && cy == Fixed::ONE {
                                            pixels.set_pixel(x, y, &color);
                                        } else {
                                            let opacity = (cx * cy).map01(255).to_int() as u8;
                                            if opacity > 0 {
                                                pixels.blend_pixel_int(x, y, &color, opacity);
                                            }
                                        }
                                    }
                                }
                            }
                            assert_eq!(
                                renderer.target.read_pixels().unwrap().bytes(),
                                reference.read_pixels().unwrap().bytes(),
                                "format={format:?}, origin={origin:?}, extent={extent:?}, clip={clip:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn fill_axis_aligned_blend_mode_honours_caller_folded_opa() {
        let mut buf = std::vec![0u8; 4 * 4 * 4];
        let mut tex =
            Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
        tex.alpha_mode = crate::render::texture::AlphaMode::Blend;
        let folded = ((128u16 * 128) / 255) as u8;
        super::fill_axis_aligned(
            &mut tex.pixels_mut().unwrap(),
            0,
            0,
            4,
            4,
            &Color::rgba(255, 0, 0, 128),
            folded,
        );
        let p = tex.read_pixels().unwrap().get_pixel(2, 2);
        let expected = folded as i32;
        assert!(
            (p.a as i32 - expected).abs() <= 2,
            "Blend fallback must use folded opa, expected ~{expected}, got {}",
            p.a,
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod clip_mask_check {
    extern crate std;
    use super::*;
    use crate::render::canvas::Canvas;
    use crate::render::path::Path;
    use crate::render::raster::FillRule;
    use crate::types::{Point, Transform};

    fn circle_path(cx: i32, cy: i32, r: i32) -> Path {
        let k = (r as f32 * 0.552_284_8) as i32;
        let mut path = Path::new();
        let cx_f = Fixed::from_int(cx);
        let cy_f = Fixed::from_int(cy);
        let r_f = Fixed::from_int(r);
        let k_f = Fixed::from_int(k);
        path.move_to(Point {
            x: cx_f + r_f,
            y: cy_f,
        });
        path.cubic_to(
            Point {
                x: cx_f + r_f,
                y: cy_f + k_f,
            },
            Point {
                x: cx_f + k_f,
                y: cy_f + r_f,
            },
            Point {
                x: cx_f,
                y: cy_f + r_f,
            },
        );
        path.cubic_to(
            Point {
                x: cx_f - k_f,
                y: cy_f + r_f,
            },
            Point {
                x: cx_f - r_f,
                y: cy_f + k_f,
            },
            Point {
                x: cx_f - r_f,
                y: cy_f,
            },
        );
        path.cubic_to(
            Point {
                x: cx_f - r_f,
                y: cy_f - k_f,
            },
            Point {
                x: cx_f - k_f,
                y: cy_f - r_f,
            },
            Point {
                x: cx_f,
                y: cy_f - r_f,
            },
        );
        path.cubic_to(
            Point {
                x: cx_f + k_f,
                y: cy_f - r_f,
            },
            Point {
                x: cx_f + r_f,
                y: cy_f - k_f,
            },
            Point {
                x: cx_f + r_f,
                y: cy_f,
            },
        );
        path.close();
        path
    }

    #[test]
    fn fill_rect_inside_circular_clip_masks_outside_pixels() {
        let w = 32;
        let h = 32;
        let mut buf = std::vec![0u8; (w * h * 4) as usize];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, w, h);

        let circle = circle_path(16, 16, 10);
        backend.push_clip(&circle, &Transform::IDENTITY, FillRule::EvenOdd);

        backend.fill_rect(
            &Rect::new(0, 0, w, h),
            &clip,
            &Color::rgb(255, 0, 0),
            Fixed::ZERO,
            255,
        );

        backend.pop_clip();

        let center = backend.target.read_pixels().unwrap().get_pixel(16, 16);
        assert_eq!(
            center.r, 255,
            "center pixel (inside circle) should be filled red"
        );

        let corner = backend.target.read_pixels().unwrap().get_pixel(0, 0);
        assert_eq!(
            corner.r, 0,
            "corner pixel (outside circle) must be untouched, got r={}",
            corner.r
        );

        let edge = backend.target.read_pixels().unwrap().get_pixel(31, 16);
        assert_eq!(
            edge.r, 0,
            "right-edge pixel (outside circle) must be untouched, got r={}",
            edge.r
        );
    }

    #[test]
    fn fill_rect_no_clip_stack_unchanged_behavior() {
        let w = 8;
        let h = 8;
        let mut buf = std::vec![0u8; (w * h * 4) as usize];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, w, h);

        backend.fill_rect(
            &Rect::new(0, 0, w, h),
            &clip,
            &Color::rgb(255, 0, 0),
            Fixed::ZERO,
            255,
        );

        assert_eq!(backend.target.read_pixels().unwrap().get_pixel(0, 0).r, 255);
        assert_eq!(backend.target.read_pixels().unwrap().get_pixel(7, 7).r, 255);
    }
}
