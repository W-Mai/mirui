use super::*;
use crate::test_support::tracked_allocations;

#[test]
fn rgba_integer_blits_match_straight_alpha_pixel_blending_without_allocating() {
    for (scale, format) in [1, 2].into_iter().flat_map(|scale| {
        [
            ColorFormat::RGBA8888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
        ]
        .map(|format| (scale, format))
    }) {
        for destination_alpha in [0, 96, 255] {
            for source_alpha in [
                Some(0),
                Some(1),
                Some(64),
                Some(128),
                Some(254),
                Some(255),
                None,
            ] {
                for cropped in [false, true] {
                    let mut source_bytes = [0u8; 3 * 16];
                    let mut source = Texture::new(&mut source_bytes, 3, 3, ColorFormat::RGBA8888)
                        .expect("valid pixel storage");
                    source.stride = 16;
                    for y in 0..3 {
                        for x in 0..3 {
                            source.pixels_mut().unwrap().set_pixel(
                                x,
                                y,
                                &Color::rgba(
                                    (17 + x * 73) as u8,
                                    (31 + y * 61) as u8,
                                    (47 + (x + y) * 29) as u8,
                                    source_alpha.unwrap_or([0, 128, 255][(x + y) as usize % 3]),
                                ),
                            );
                        }
                    }
                    let mut reference_bytes = [0xa5u8; 6 * 28];
                    {
                        let mut initial = Texture::new(&mut reference_bytes, 6, 6, format)
                            .expect("valid pixel storage");
                        initial.stride = 28;
                        for y in 0..6 {
                            for x in 0..6 {
                                initial.pixels_mut().unwrap().set_pixel(
                                    x,
                                    y,
                                    &Color::rgba(
                                        (233 - x * 19) as u8,
                                        (101 + y * 17) as u8,
                                        (11 + x * 23 + y * 13) as u8,
                                        destination_alpha,
                                    ),
                                );
                            }
                        }
                    }
                    let mut actual_bytes = reference_bytes;
                    let mut reference = Texture::new(&mut reference_bytes, 6, 6, format)
                        .expect("valid pixel storage");
                    reference.stride = 28;
                    reference.alpha_mode = AlphaMode::Blend;
                    let mut actual =
                        Texture::new(&mut actual_bytes, 6, 6, format).expect("valid pixel storage");
                    actual.stride = 28;
                    actual.alpha_mode = AlphaMode::Blend;
                    let clip_start = if cropped { 1 + scale } else { 0 };
                    let clip_end = if cropped { 1 + 2 * scale } else { 6 };
                    for y in 1..1 + 2 * scale {
                        for x in 1..1 + 2 * scale {
                            if x < clip_start || y < clip_start || x >= clip_end || y >= clip_end {
                                continue;
                            }
                            let color = source
                                .read_pixels()
                                .unwrap()
                                .get_pixel(1 + (x - 1) / scale, 1 + (y - 1) / scale);
                            reference
                                .pixels_mut()
                                .unwrap()
                                .blend_pixel_int(x, y, &color, color.a);
                        }
                    }
                    let allocations = tracked_allocations(|| {
                        let blit = if scale == 1 {
                            blit_1to1_fast
                        } else {
                            blit_2to2_fast
                        };
                        blit(
                            &mut actual.pixels_mut().unwrap(),
                            &source.read_pixels().unwrap(),
                            1,
                            1,
                            2,
                            2,
                            1,
                            1,
                            clip_start,
                            clip_start,
                            clip_end,
                            clip_end,
                        );
                    });
                    assert_eq!(allocations, 0);
                    assert_eq!(
                        actual.read_pixels().unwrap().bytes(),
                        reference.read_pixels().unwrap().bytes(),
                        "format={format:?}, scale={scale}, source_alpha={source_alpha:?}, destination_alpha={destination_alpha}, cropped={cropped}"
                    );
                }
            }
        }
    }
}

#[test]
fn integer_blits_match_pixel_blending_in_every_destination_format() {
    for format in [
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
        ColorFormat::RGB888,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
    ] {
        for alpha_mode in [AlphaMode::Opaque, AlphaMode::Blend] {
            for scale in [1, 2] {
                for source_alpha in 0..=255 {
                    let source_color = Color::rgba(201, 37, 9, source_alpha);
                    let destination_color = Color::rgba(17, 101, 233, 96);
                    let mut source_bytes = [0u8; 4];
                    let mut source = Texture::new(&mut source_bytes, 1, 1, ColorFormat::RGBA8888)
                        .expect("valid pixel storage");
                    source.pixels_mut().unwrap().set_pixel(0, 0, &source_color);
                    let mut destination_bytes = [0u8; 16];
                    let mut destination = Texture::new(&mut destination_bytes, 2, 2, format)
                        .expect("valid pixel storage");
                    destination.alpha_mode = alpha_mode;
                    for y in 0..2 {
                        for x in 0..2 {
                            destination
                                .pixels_mut()
                                .unwrap()
                                .set_pixel(x, y, &destination_color);
                        }
                    }
                    let mut expected_bytes = [0u8; 16];
                    let mut reference = Texture::new(&mut expected_bytes, 2, 2, format).unwrap();
                    reference.alpha_mode = alpha_mode;
                    for y in 0..2 {
                        for x in 0..2 {
                            reference
                                .pixels_mut()
                                .unwrap()
                                .set_pixel(x, y, &destination_color);
                            if x < scale && y < scale {
                                reference.pixels_mut().unwrap().blend_pixel_int(
                                    x,
                                    y,
                                    &source_color,
                                    source_alpha,
                                );
                            }
                        }
                    }
                    let blit = if scale == 1 {
                        blit_1to1_fast
                    } else {
                        blit_2to2_fast
                    };
                    blit(
                        &mut destination.pixels_mut().unwrap(),
                        &source.read_pixels().unwrap(),
                        0,
                        0,
                        1,
                        1,
                        0,
                        0,
                        0,
                        0,
                        2,
                        2,
                    );
                    assert_eq!(
                        destination.read_pixels().unwrap().bytes(),
                        reference.read_pixels().unwrap().bytes(),
                        "{format:?}, {alpha_mode:?}, scale={scale}, alpha={source_alpha}"
                    );
                }
            }
        }
    }
}
