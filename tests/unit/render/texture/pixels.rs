use super::*;

#[test]
fn solid_spans_match_scalar_writes_for_all_formats_modes_and_tails() {
    for format in [
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
        ColorFormat::RGB888,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
    ] {
        for mode in [AlphaMode::Opaque, AlphaMode::Blend] {
            for alpha in [0, 1, 17, 127, 128, 254, 255] {
                for length in 0..=33 {
                    for prefix in 0..=4 {
                        let color = Color::rgba(19, 254, 87, 41);
                        let mut actual = [0_u8; 148];
                        for (i, byte) in actual.iter_mut().enumerate() {
                            *byte = (i as u8).wrapping_mul(71).wrapping_add(9);
                        }
                        let mut expected = actual;
                        let end = prefix + length * format.bytes_per_pixel();
                        format.blend_solid_span(&mut actual[prefix..end], &color, alpha, mode);
                        for offset in (prefix..end).step_by(format.bytes_per_pixel()) {
                            format.blend_pixel(&mut expected, offset, &color, alpha, mode);
                        }
                        assert_eq!(
                            actual, expected,
                            "{format:?} {mode:?} {alpha} {length} {prefix}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn opaque_blending_kernels_match_all_scalar_channel_inputs() {
    for alpha in 0..=255_u8 {
        for source in 0..=255_u8 {
            for destination in 0..=255_u8 {
                let a = Color::rgba(source, source ^ 0x55, source.rotate_left(3), source ^ alpha);
                let b = Color::rgba(
                    destination,
                    destination ^ 0xAA,
                    destination.rotate_right(2),
                    destination ^ alpha,
                );
                let scalar = |s: u8, d: u8| {
                    let sum = u32::from(s) * u32::from(alpha)
                        + u32::from(d) * (255 - u32::from(alpha))
                        + 127;
                    ((sum + (sum >> 8)) >> 8) as u8
                };
                let expected = Color::rgb(scalar(a.r, b.r), scalar(a.g, b.g), scalar(a.b, b.b));
                assert_eq!(blend_opaque(&a, &b, alpha), expected);
                let mut rgba = [b.r, b.g, b.b, b.a];
                blend_constant_bytes::<4>(&mut rgba, [a.r, a.g, a.b], alpha);
                assert_eq!(rgba, [expected.r, expected.g, expected.b, 255]);
                let mut rgb = [b.r, b.g, b.b];
                blend_constant_bytes::<3>(&mut rgb, [a.r, a.g, a.b], alpha);
                assert_eq!(rgb, [expected.r, expected.g, expected.b]);
            }
        }
    }
}

#[test]
fn packed_writes_preserve_surrounding_bytes_in_every_format() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        for channel in 0..=255 {
            let color = Color::rgba(channel, 255 - channel, channel / 2, channel);
            for offset in 0..=4 {
                let mut bytes = [0xa5; 8];
                format.write_pixel(&mut bytes, offset, &color);
                let end = offset + format.bytes_per_pixel();
                assert!(bytes[..offset].iter().all(|byte| *byte == 0xa5));
                assert!(bytes[end..].iter().all(|byte| *byte == 0xa5));
                assert_eq!(
                    &bytes[offset..end],
                    &format.pack(&color).to_le_bytes()[..format.bytes_per_pixel()]
                );
                let decoded = format.read_pixel(&bytes, offset);
                match format {
                    ColorFormat::RGBA8888 | ColorFormat::BGRA8888 => assert_eq!(decoded, color),
                    ColorFormat::RGB888 => assert_eq!(decoded, Color { a: 255, ..color }),
                    ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
                        assert_eq!(decoded.a, 255);
                        assert!(decoded.r.abs_diff(color.r) <= 7);
                        assert!(decoded.g.abs_diff(color.g) <= 3);
                        assert!(decoded.b.abs_diff(color.b) <= 7);
                    }
                }
            }
        }
    }
}

#[test]
fn every_rgb565_encoding_roundtrips_in_both_byte_orders() {
    for encoded in 0..=u16::MAX {
        let little = encoded.to_le_bytes();
        let big = encoded.to_be_bytes();
        let color = ColorFormat::RGB565.read_pixel(&little, 0);
        assert_eq!(color, ColorFormat::RGB565Swapped.read_pixel(&big, 0));
        let mut bytes = [0; 2];
        ColorFormat::RGB565.write_pixel(&mut bytes, 0, &color);
        assert_eq!(bytes, little);
        ColorFormat::RGB565Swapped.write_pixel(&mut bytes, 0, &color);
        assert_eq!(bytes, big);
    }
}
