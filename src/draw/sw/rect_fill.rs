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
            crate::trace_span!("sw.fill_aligned");
            fill_axis_aligned(&mut self.target, px_x0, px_y0, px_x1, px_y1, color, opa);
            return;
        }

        crate::trace_span!("sw.fill_aa_loop");
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
        for py in px_y0..px_y1 {
            for px in px_x0..px_x1 {
                target.blend_pixel_int(px, py, color, opa);
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
}
