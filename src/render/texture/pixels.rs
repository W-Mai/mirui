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
        let packed = self.pack(color).to_le_bytes();
        match self {
            Self::RGB565 | Self::RGB565Swapped => {
                bytes[offset..offset + 2].copy_from_slice(&packed[..2]);
            }
            Self::RGB888 => {
                bytes[offset..offset + 3].copy_from_slice(&packed[..3]);
            }
            Self::RGBA8888 | Self::BGRA8888 => {
                bytes[offset..offset + 4].copy_from_slice(&packed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_rgb_blending_matches_all_scalar_channel_inputs() {
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
                    assert_eq!(
                        blend_opaque(&a, &b, alpha),
                        Color::rgb(scalar(a.r, b.r), scalar(a.g, b.g), scalar(a.b, b.b))
                    );
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
