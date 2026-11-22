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
