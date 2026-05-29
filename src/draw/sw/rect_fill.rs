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
        // Fold color.a into opa so every downstream blend treats the
        // colour as opaque RGB. Mirrors what blend_pixel does internally
        // before delegating to blend_pixel_int. The fold happens before
        // coverage instead of after, which can shift a half-transparent
        // fully-covered pixel by ±1 alpha vs the pre-eb98ecc path; the
        // difference is below visual threshold and inside the existing
        // ±2 symmetry tolerance.
        let effective_opa = ((color.a as u16 * opa as u16) / 255) as u8;
        let opa_norm =
            Fixed::from_int(effective_opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));

        if area.is_aligned() && r == Fixed::ZERO {
            crate::trace_span!("sw.fill_aligned");
            fill_axis_aligned(
                &mut self.target,
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
                    &mut self.target,
                    inner_x0.max(px_x0),
                    area_y0.max(px_y0),
                    inner_x1.min(px_x1),
                    inner_y0.min(px_y1),
                    color,
                    effective_opa,
                );
                fill_axis_aligned(
                    &mut self.target,
                    inner_x0.max(px_x0),
                    inner_y1.max(px_y0),
                    inner_x1.min(px_x1),
                    area_y1.min(px_y1),
                    color,
                    effective_opa,
                );
                fill_axis_aligned(
                    &mut self.target,
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
                                self.target.blend_pixel_int(px, py, color, final_opa);
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
                        &mut self.target,
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
                            &mut self.target,
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
                            &mut self.target,
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

            for px in px_x0..px_x1 {
                let pixel_left = Fixed::from_int(px);
                let pixel_right = Fixed::from_int(px + 1);
                let col_full_x = pixel_left >= area.x && pixel_right <= area_x_end;
                let in_mid_x = px >= mid_x0 && px < mid_x1;

                // Hot path: straight-zone fully-opaque pixel — skip
                // every Fixed multiply and the SDF call.
                if row_full_y && col_full_x && in_mid_y && in_mid_x && opa_full {
                    self.target.set_pixel(px, py, color);
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

                let final_opa = (cov_x * cov_y * corner_cov * opa_norm).map01(255).to_int() as u8;
                if final_opa > 0 {
                    // px/py are integers — skip blend_pixel's is_integer dispatch.
                    self.target.blend_pixel_int(px, py, color, final_opa);
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
    target: &mut Texture,
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

fn fill_axis_aligned(
    target: &mut Texture,
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
    let blend_aware = target.alpha_mode == crate::draw::texture::AlphaMode::Blend && color.a < 255;
    if opa == 255 && !blend_aware {
        let bpp = target.format.bytes_per_pixel();
        let stride = target.stride;
        let row_px = (px_x1 - px_x0) as usize;
        let row_bytes = row_px * bpp;
        let buf = target.buf.as_mut_slice();
        match target.format {
            ColorFormat::RGBA8888 => {
                fill_first_row_then_replicate::<4>(
                    buf,
                    stride,
                    px_x0,
                    px_y0,
                    px_y1,
                    row_bytes,
                    [color.r, color.g, color.b, color.a],
                );
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
                fill_first_row_then_replicate::<2>(
                    buf, stride, px_x0, px_y0, px_y1, row_bytes, pixel,
                );
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
        let effective_opa = if blend_aware { color.a } else { opa };
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                target.blend_pixel_int(px, py, color, effective_opa);
            }
        }
    }
}

/// Fast-path opaque solid fill: write one scanline, then memcpy
/// it into each subsequent row. Avoids both per-pixel
/// `copy_from_slice` bounds-check overhead and any heap alloc for a
/// repeating-pattern buffer.
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
    for py in (px_y0 + 1)..px_y1 {
        let dst_start = py as usize * stride + px_x0 as usize * BPP;
        let (lo, hi) = buf.split_at_mut(dst_start);
        let src = &lo[first_start..first_start + row_bytes];
        hi[..row_bytes].copy_from_slice(src);
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
    use crate::draw::canvas::Canvas;
    use std::string::String;
    use std::vec::Vec;

    fn render_circle(w: i32, h: i32, r: i32) -> Vec<Vec<u8>> {
        let mut buf = std::vec![0u8; (w as usize) * (h as usize) * 4];
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888);
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
                out[py as usize][px as usize] = backend.target.get_pixel(px, py).r;
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
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
        for y in 0..16 {
            for x in 0..32 {
                let t = g[y][x];
                let b = g[31 - y][x];
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
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888);
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
                out[py as usize][px as usize] = backend.target.get_pixel(px, py).r;
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
        for y in 10..21 {
            for x in 9..36 {
                assert_eq!(
                    g[y][x], 255,
                    "fractional-origin inner pixel ({x},{y}) not solid: alpha={}",
                    g[y][x]
                );
            }
        }
        // Fractional edges must still anti-alias — top row of the
        // fractional area must be partially transparent (cov_y < 1).
        for x in 9..36 {
            let edge = g[3][x];
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
        for y in 6..18 {
            for x in 6..34 {
                assert_eq!(
                    g[y][x], 255,
                    "inner pixel ({x},{y}) not solid: alpha={}",
                    g[y][x]
                );
            }
        }
        for y in [0, 5, 18, 23] {
            for x in 6..34 {
                assert_eq!(
                    g[y][x], 255,
                    "straight band pixel ({x},{y}) not solid: alpha={}",
                    g[y][x]
                );
            }
        }
        for y in 6..18 {
            for x in [0, 5, 34, 39] {
                assert_eq!(
                    g[y][x], 255,
                    "straight band pixel ({x},{y}) not solid: alpha={}",
                    g[y][x]
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
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let rect = Rect::new(0, 0, w, h);
        let clip = Rect::new(0, 0, w, h);
        backend.fill_rect(&rect, &clip, &color, r, opa);
        let mut out = std::vec![std::vec![Color::rgba(0,0,0,0); w as usize]; h as usize];
        for py in 0..h {
            for px in 0..w {
                out[py as usize][px as usize] = backend.target.get_pixel(px, py);
            }
        }
        out
    }

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
        let tex = Texture::new(&mut buf, w as u16, h as u16, ColorFormat::RGBA8888);
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
                out[py as usize][px as usize] = backend.target.get_pixel(px, py);
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
        let tex = Texture::new(&mut buf, 40, 24, ColorFormat::RGBA8888);
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
            backend.target.get_pixel(1, 8).a,
            255,
            "clipped middle slab pixel (1,8) should be drawn"
        );
        // (2, 4) is inside the TL corner bbox AND inside the clip;
        // SDF coverage should write at least a partial pixel here.
        let corner = backend.target.get_pixel(2, 4);
        assert!(
            corner.a > 0,
            "TL corner pixel (2,4) should be at least partially drawn, got a={}",
            corner.a
        );
        // Outside clip — must remain untouched.
        assert_eq!(
            backend.target.get_pixel(20, 12).a,
            0,
            "pixel outside clip should not be written"
        );
    }
}
