//! Packed RGBA rows for the existing nearest-neighbor sampling paths.
use crate::render::sampling::SampleAxis;
use crate::render::texture::{AlphaMode, ColorFormat, TexturePixels, TexturePixelsMut};
use crate::types::{Color, Fixed, Rect};

fn accepts(
    dst: &TexturePixelsMut<'_>,
    src: &TexturePixels<'_>,
    rect: (i32, i32, i32, i32),
) -> bool {
    let (x, y, w, h) = rect;
    dst.format() == ColorFormat::RGBA8888
        && src.format() == ColorFormat::RGBA8888
        && x >= 0
        && y >= 0
        && w > 0
        && h > 0
        && i64::from(x) + i64::from(w) <= i64::from(src.width())
        && i64::from(y) + i64::from(h) <= i64::from(src.height())
}

#[inline]
fn over(dst: &mut [u8], src: &[u8], alpha: u8, mode: AlphaMode) {
    if alpha == 255 {
        dst.copy_from_slice(&[src[0], src[1], src[2], 255]);
    } else if alpha != 0 {
        ColorFormat::RGBA8888.blend_pixel(
            dst,
            0,
            &Color::rgba(src[0], src[1], src[2], src[3]),
            alpha,
            mode,
        );
    }
}

pub(super) fn try_dda(
    dst: &mut TexturePixelsMut<'_>,
    src: &TexturePixels<'_>,
    rect: (i32, i32, u16, u16),
    destination: (i32, i32, i32, i32),
    clip: (i32, i32, i32, i32),
    opacity: u8,
) -> bool {
    let (sx, sy, sw, sh) = rect;
    let (dx, dy, dw, dh) = destination;
    if dw <= 0 || dh <= 0 || !accepts(dst, src, (sx, sy, i32::from(sw), i32::from(sh))) {
        return false;
    }
    if opacity == 0 {
        return true;
    }
    let (cx0, cy0, cx1, cy1) = clip;
    let x0 = i64::from(dx).max(i64::from(cx0)).max(0);
    let y0 = i64::from(dy).max(i64::from(cy0)).max(0);
    let x1 = (i64::from(dx) + i64::from(dw))
        .min(i64::from(cx1))
        .min(i64::from(dst.width()));
    let y1 = (i64::from(dy) + i64::from(dh))
        .min(i64::from(cy1))
        .min(i64::from(dst.height()));
    if x0 >= x1 || y0 >= y1 {
        return true;
    }
    let xs = crate::render::sampling::dda_step(sw, dw);
    let ys = crate::render::sampling::dda_step(sh, dh);
    let first_x = (x0 - i64::from(dx)) as u32;
    let mut y_acc = ((y0 - i64::from(dy)) as u32).wrapping_mul(ys);
    let src_stride = src.stride();
    let dst_stride = dst.stride();
    let mode = dst.alpha_mode();
    let source = src.bytes();
    let target = dst.bytes_mut();
    let reuse_rows = opacity == 255 && ys < 1 << 16;
    let mut previous_row = None;
    for y in y0..y1 {
        let row = (sy as usize + (y_acc >> 16) as usize) * src_stride + sx as usize * 4;
        y_acc = y_acc.wrapping_add(ys);
        let pixels = &source[row..row + usize::from(sw) * 4];
        let begin = y as usize * dst_stride + x0 as usize * 4;
        let end = y as usize * dst_stride + x1 as usize * 4;
        if reuse_rows {
            match previous_row {
                Some((source_row, completed_row, opaque)) if source_row == row => {
                    if opaque {
                        let (completed, destination) = target.split_at_mut(begin);
                        destination[..end - begin].copy_from_slice(
                            &completed[completed_row..completed_row + end - begin],
                        );
                        continue;
                    }
                }
                _ => {
                    previous_row = Some((row, begin, pixels.chunks_exact(4).all(|p| p[3] == 255)));
                }
            }
        }
        let mut x_acc = first_x.wrapping_mul(xs);
        for pixel in target[begin..end].chunks_exact_mut(4) {
            let index = (x_acc >> 16) as usize * 4;
            let color = &pixels[index..index + 4];
            x_acc = x_acc.wrapping_add(xs);
            let alpha = (u16::from(color[3]) * u16::from(opacity) / 255) as u8;
            over(pixel, color, alpha, mode);
        }
    }
    true
}

pub(super) fn try_fractional(
    dst: &mut TexturePixelsMut<'_>,
    src: &TexturePixels<'_>,
    rect: (i32, i32, i32, i32),
    area: Rect,
    clip: Rect,
    opacity: u8,
) -> bool {
    if area.w <= Fixed::ZERO || area.h <= Fixed::ZERO || !accepts(dst, src, rect) {
        return false;
    }
    if opacity == 0 {
        return true;
    }
    let Some(draw) = area
        .intersect(&clip)
        .and_then(|r| r.intersect(&Rect::new(0, 0, dst.width(), dst.height())))
    else {
        return true;
    };
    let (x0, y0, x1, y1) = draw.pixel_bounds();
    let (sx, sy, sw, sh) = rect;
    let first_x = SampleAxis::new(x0, area.x, area.w, sw);
    let mut ys = SampleAxis::new(y0, area.y, area.h, sh);
    let right = area.x + area.w;
    let bottom = area.y + area.h;
    let left_full = area.x.ceil().to_int();
    let right_full = right.floor().to_int();
    let source_stride = src.stride();
    let target_stride = dst.stride();
    let mode = dst.alpha_mode();
    let source = src.bytes();
    let target = dst.bytes_mut();
    let batch_pixels = mode == AlphaMode::Opaque && Fixed::from_int(sw) < area.w;
    for y in y0..y1 {
        let source_y = ys.next(sh);
        let coverage_y = super::rect_fill::pixel_axis_coverage(area.y, bottom, y);
        if (Fixed::from_int(i32::from(opacity)) * coverage_y).to_int() == 0 {
            continue;
        }
        let row = (sy + source_y) as usize * source_stride + sx as usize * 4;
        let pixels = &source[row..row + sw as usize * 4];
        let begin = y as usize * target_stride + x0 as usize * 4;
        let end = y as usize * target_stride + x1 as usize * 4;
        let mut xs = first_x;
        if batch_pixels {
            let target_row = &mut target[begin..end];
            let mut x = x0;
            while x < x1 {
                let interior = x >= left_full && x < right_full;
                let limit = if interior {
                    (x1.min(right_full) - x) as u32
                } else {
                    1
                };
                let (sample, count) = xs.next_run(sw, limit);
                let index = sample as usize * 4;
                let color = Color::rgba(
                    pixels[index],
                    pixels[index + 1],
                    pixels[index + 2],
                    pixels[index + 3],
                );
                let mut alpha = ((u16::from(color.a) * u16::from(opacity) + 127) / 255) as u8;
                let coverage = if interior {
                    coverage_y
                } else {
                    super::rect_fill::pixel_axis_coverage(area.x, right, x) * coverage_y
                };
                if coverage != Fixed::ONE {
                    alpha = (Fixed::from_int(i32::from(alpha)) * coverage).to_int() as u8;
                }
                let offset = (x - x0) as usize * 4;
                ColorFormat::RGBA8888.blend_solid_span(
                    &mut target_row[offset..offset + count as usize * 4],
                    &color,
                    alpha,
                    mode,
                );
                x += count as i32;
            }
            continue;
        }
        for (i, pixel) in target[begin..end].chunks_exact_mut(4).enumerate() {
            let index = xs.next(sw) as usize * 4;
            let color = &pixels[index..index + 4];
            let mut alpha = ((u16::from(color[3]) * u16::from(opacity) + 127) / 255) as u8;
            let x = x0 + i as i32;
            if coverage_y != Fixed::ONE || x < left_full || x >= right_full {
                let coverage = super::rect_fill::pixel_axis_coverage(area.x, right, x) * coverage_y;
                alpha = (Fixed::from_int(i32::from(alpha)) * coverage).to_int() as u8;
            }
            over(pixel, color, alpha, mode);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::command::CompositeMode;
    use crate::render::texture::Texture;

    #[test]
    fn scaled_row_reuse_preserves_background_alpha_pitch_and_clipping() {
        let mask = [255_u8; 12 * 9];
        for mixed_alpha in [false, true] {
            let mut bytes = [0xAC_u8; 32 * 4];
            for y in 0..4 {
                for x in 0..6 {
                    let i = y * 32 + x * 4;
                    bytes[i..i + 4].copy_from_slice(&[
                        (17 * x + 31 * y) as u8,
                        (113 + 3 * x + 5 * y) as u8,
                        (19 * x + 7 * y) as u8,
                        if mixed_alpha && y == 2 {
                            [0, 1, 96, 128, 254, 255][x]
                        } else {
                            255
                        },
                    ]);
                }
            }
            let mut texture = Texture::from_ref(&bytes, 6, 4, ColorFormat::RGBA8888).unwrap();
            texture.stride = 32;
            let source = texture.read_pixels().unwrap();
            for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
                for opacity in [127, 254, 255] {
                    for rect in [(0, 0, 6, 4), (1, 1, 4, 2)] {
                        for destination in [
                            (0, 0, 12, 9),
                            (-3, -2, 13, 10),
                            (3, 2, 6, 5),
                            (0, 0, 6, 4),
                            (2, 3, 3, 2),
                        ] {
                            for clip in [(0, 0, 12, 9), (1, 2, 11, 8), (-5, -6, 19, 20)] {
                                let mut actual = [0xB7_u8; 56 * 9 + 8];
                                for y in 0..9 {
                                    for x in 0..12 {
                                        let i = y * 56 + x * 4;
                                        actual[i..i + 4].copy_from_slice(&[
                                            (19 * x) as u8,
                                            (23 * y) as u8,
                                            (7 * x + 17 * y) as u8,
                                            (11 * x + 13 * y) as u8,
                                        ]);
                                    }
                                }
                                let mut expected = actual;
                                {
                                    let mut fast =
                                        Texture::new(&mut actual, 12, 9, ColorFormat::RGBA8888)
                                            .unwrap();
                                    fast.stride = 56;
                                    fast.alpha_mode = mode;
                                    let mut slow =
                                        Texture::new(&mut expected, 12, 9, ColorFormat::RGBA8888)
                                            .unwrap();
                                    slow.stride = 56;
                                    slow.alpha_mode = mode;
                                    assert_eq!(
                                        crate::test_support::tracked_allocations(|| {
                                            assert!(try_dda(
                                                &mut fast.pixels_mut().unwrap(),
                                                &source,
                                                rect,
                                                destination,
                                                clip,
                                                opacity,
                                            ));
                                        }),
                                        0
                                    );
                                    let (sx, sy, sw, sh) = rect;
                                    let (dx, dy, dw, dh) = destination;
                                    let (x0, y0, x1, y1) = clip;
                                    super::super::blit_fast::blit_dda(
                                        &mut slow.pixels_mut().unwrap(),
                                        &source,
                                        sx,
                                        sy,
                                        sw,
                                        sh,
                                        dx,
                                        dy,
                                        dw,
                                        dh,
                                        x0.max(0),
                                        y0.max(0),
                                        x1.min(12),
                                        y1.min(9),
                                        opacity,
                                        Some(&mask),
                                    );
                                }
                                assert_eq!(
                                    actual, expected,
                                    "{mixed_alpha} {opacity} {rect:?} {destination:?} {clip:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn packed_rows_match_masked_generic_sampling_with_stride_alpha_clipping_and_invalid_sources() {
        let mut bytes = [0_u8; 32 * 7];
        for y in 0..7 {
            for x in 0..6 {
                let i = y * 32 + x * 4;
                bytes[i..i + 4].copy_from_slice(&[
                    (17 * x + 31 * y) as u8,
                    (113 + 3 * x + 5 * y) as u8,
                    (19 * x + 7 * y) as u8,
                    [0, 1, 96, 128, 254, 255][(x + y) % 6],
                ]);
            }
        }
        let mut texture = Texture::from_ref(&bytes, 6, 7, ColorFormat::RGBA8888).unwrap();
        texture.stride = 32;
        let source = texture.read_pixels().unwrap();
        let mask = [255_u8; 12 * 9];
        for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
            for opacity in [0, 1, 127, 254, 255] {
                for source_rect in [(1, 1, 4, 3), (-1, 0, 4, 3), (4, 5, 4, 3)] {
                    for (dx, dy, dw, dh) in [
                        (0, 0, 12, 9),
                        (-3, -2, 13, 10),
                        (3, 2, 6, 5),
                        (15, 0, 8, 7),
                        (0, 12, 8, 7),
                    ] {
                        let mut actual = [61_u8; 56 * 9 + 8];
                        let mut expected = actual;
                        {
                            let mut fast =
                                Texture::new(&mut actual, 12, 9, ColorFormat::RGBA8888).unwrap();
                            fast.stride = 56;
                            fast.alpha_mode = mode;
                            let mut slow =
                                Texture::new(&mut expected, 12, 9, ColorFormat::RGBA8888).unwrap();
                            slow.stride = 56;
                            slow.alpha_mode = mode;
                            let (sx, sy, sw, sh) = source_rect;
                            let allocations = crate::test_support::tracked_allocations(|| {
                                super::super::blit_fast::blit_dda(
                                    &mut fast.pixels_mut().unwrap(),
                                    &source,
                                    sx,
                                    sy,
                                    sw,
                                    sh,
                                    dx,
                                    dy,
                                    dw,
                                    dh,
                                    1,
                                    1,
                                    11,
                                    8,
                                    opacity,
                                    None,
                                )
                            });
                            assert_eq!(allocations, 0);
                            super::super::blit_fast::blit_dda(
                                &mut slow.pixels_mut().unwrap(),
                                &source,
                                sx,
                                sy,
                                sw,
                                sh,
                                dx,
                                dy,
                                dw,
                                dh,
                                1,
                                1,
                                11,
                                8,
                                opacity,
                                Some(&mask),
                            );
                        }
                        assert_eq!(
                            actual,
                            expected,
                            "DDA {opacity} {source_rect:?} {:?}",
                            (dx, dy, dw, dh)
                        );
                    }
                    for n in [-320, -128, 0, 77, 256] {
                        let mut actual = [61_u8; 56 * 9 + 8];
                        let mut expected = actual;
                        {
                            let mut fast =
                                Texture::new(&mut actual, 12, 9, ColorFormat::RGBA8888).unwrap();
                            fast.stride = 56;
                            fast.alpha_mode = mode;
                            let mut slow =
                                Texture::new(&mut expected, 12, 9, ColorFormat::RGBA8888).unwrap();
                            slow.stride = 56;
                            slow.alpha_mode = mode;
                            let (sx, sy, sw, sh) = source_rect;
                            let rect = (sx, sy, i32::from(sw), i32::from(sh));
                            let area = Rect::new(
                                Fixed::from_ratio(n, 256),
                                Fixed::from_ratio(n + 64, 256),
                                Fixed::from_ratio(1341, 256),
                                Fixed::from_ratio(1207, 256),
                            );
                            let clip = Rect::new(1, 1, 10, 7);
                            assert_eq!(
                                crate::test_support::tracked_allocations(|| {
                                    super::super::blit_fractional::blit_fractional(
                                        &mut fast.pixels_mut().unwrap(),
                                        &source,
                                        rect,
                                        area,
                                        clip,
                                        opacity,
                                        Fixed::ZERO,
                                        CompositeMode::SourceOver,
                                        None,
                                    )
                                }),
                                0
                            );
                            super::super::blit_fractional::blit_fractional(
                                &mut slow.pixels_mut().unwrap(),
                                &source,
                                rect,
                                area,
                                clip,
                                opacity,
                                Fixed::ZERO,
                                CompositeMode::SourceOver,
                                Some(&mask),
                            );
                        }
                        assert_eq!(actual, expected, "fractional {opacity} {source_rect:?} {n}");
                    }
                }
            }
        }
    }
}
