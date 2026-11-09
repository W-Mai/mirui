use super::{AlphaMode, ColorFormat, blend_straight_source_over};
use crate::types::Color;

fn blend_opaque(source: &Color, destination: &Color, alpha: u8) -> Color {
    let alpha = u32::from(alpha);
    let inverse = 255 - alpha;
    let source_rb = u32::from(source.r) | (u32::from(source.b) << 16);
    let destination_rb = u32::from(destination.r) | (u32::from(destination.b) << 16);
    // Each 16-bit lane remains <= 65406 through the original rounding formula.
    let rb_sum = source_rb * alpha + destination_rb * inverse + 0x007F_007F;
    let rb = (rb_sum + ((rb_sum >> 8) & 0x00FF_00FF)) >> 8;
    let g_sum = u32::from(source.g) * alpha + u32::from(destination.g) * inverse + 127;
    let g = ((g_sum + (g_sum >> 8)) >> 8) as u8;
    Color::rgb(rb as u8, g, (rb >> 16) as u8)
}

#[inline(always)]
pub(super) fn pixel_offset(
    width: u16,
    height: u16,
    stride: usize,
    format: ColorFormat,
    x: i32,
    y: i32,
) -> Option<usize> {
    if x < 0 || y < 0 || x >= i32::from(width) || y >= i32::from(height) {
        return None;
    }
    Some(y as usize * stride + x as usize * format.bytes_per_pixel())
}

impl ColorFormat {
    /// Whether arbitrary-alpha fixed-source rows use the byte-channel kernel.
    pub(crate) fn can_batch_solid(self, mode: AlphaMode) -> bool {
        mode == AlphaMode::Opaque && matches!(self, Self::RGBA8888 | Self::BGRA8888 | Self::RGB888)
    }

    /// Blend complete pixels with a fixed source and effective alpha.
    pub(crate) fn blend_solid_span(
        self,
        bytes: &mut [u8],
        color: &Color,
        alpha: u8,
        mode: AlphaMode,
    ) {
        if alpha == 0 {
            return;
        }
        if self.can_batch_solid(mode) || alpha == 255 {
            match self {
                Self::RGBA8888 => {
                    blend_constant_bytes::<4>(bytes, [color.r, color.g, color.b], alpha);
                    return;
                }
                Self::BGRA8888 => {
                    blend_constant_bytes::<4>(bytes, [color.b, color.g, color.r], alpha);
                    return;
                }
                Self::RGB888 => {
                    blend_constant_bytes::<3>(bytes, [color.r, color.g, color.b], alpha);
                    return;
                }
                Self::RGB565 | Self::RGB565Swapped => {}
            }
        }
        let size = self.bytes_per_pixel();
        for pixel in bytes.chunks_exact_mut(size) {
            self.blend_pixel(pixel, 0, color, alpha, mode);
        }
    }

    #[inline]
    pub(crate) fn blend_pixel(
        self,
        bytes: &mut [u8],
        offset: usize,
        color: &Color,
        alpha: u8,
        alpha_mode: AlphaMode,
    ) {
        if alpha == 0 {
            return;
        }
        if alpha == 255 {
            self.write_pixel(bytes, offset, &Color { a: 255, ..*color });
            return;
        }
        let previous = self.read_pixel(bytes, offset);
        let result = match alpha_mode {
            AlphaMode::Opaque => blend_opaque(color, &previous, alpha),
            AlphaMode::Blend => blend_straight_source_over(color, &previous, alpha),
        };
        self.write_pixel(bytes, offset, &result);
    }

    #[inline(always)]
    pub(super) fn read_pixel(self, bytes: &[u8], offset: usize) -> Color {
        match self {
            Self::RGBA8888 => Color::rgba(
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ),
            Self::BGRA8888 => Color::rgba(
                bytes[offset + 2],
                bytes[offset + 1],
                bytes[offset],
                bytes[offset + 3],
            ),
            Self::RGB888 => Color::rgb(bytes[offset], bytes[offset + 1], bytes[offset + 2]),
            Self::RGB565 | Self::RGB565Swapped => {
                let pair = [bytes[offset], bytes[offset + 1]];
                let pixel = if self == Self::RGB565 {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                };
                let r = (pixel >> 11) as u8;
                let g = ((pixel >> 5) & 0x3f) as u8;
                let b = (pixel & 0x1f) as u8;
                Color::rgb(
                    (r << 3) | (r >> 2),
                    (g << 2) | (g >> 4),
                    (b << 3) | (b >> 2),
                )
            }
        }
    }

    #[inline(always)]
    pub(super) fn write_pixel(self, bytes: &mut [u8], offset: usize, color: &Color) {
        match self {
            Self::RGB565 | Self::RGB565Swapped => {
                let packed = self.pack(color).to_le_bytes();
                bytes[offset..offset + 2].copy_from_slice(&packed[..2]);
            }
            Self::RGB888 => {
                bytes[offset..offset + 3].copy_from_slice(&[color.r, color.g, color.b]);
            }
            Self::RGBA8888 => {
                bytes[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
            Self::BGRA8888 => {
                bytes[offset..offset + 4].copy_from_slice(&[color.b, color.g, color.r, color.a]);
            }
        }
    }
}

fn blend_constant_bytes<const BPP: usize>(bytes: &mut [u8], color: [u8; 3], alpha: u8) {
    if alpha == 255 {
        for pixel in bytes.chunks_exact_mut(BPP) {
            pixel[..3].copy_from_slice(&color);
            if BPP == 4 {
                pixel[3] = 255;
            }
        }
        return;
    }
    let alpha = u16::from(alpha);
    let inverse = 255 - alpha;
    let source = color.map(|channel| u16::from(channel) * alpha + 127);
    for pixel in bytes.chunks_exact_mut(BPP) {
        for channel in 0..3 {
            let sum = u16::from(pixel[channel]) * inverse + source[channel];
            pixel[channel] = ((sum + (sum >> 8)) >> 8) as u8;
        }
        if BPP == 4 {
            pixel[3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
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
                    let a =
                        Color::rgba(source, source ^ 0x55, source.rotate_left(3), source ^ alpha);
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
}
