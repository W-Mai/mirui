use super::ColorFormat;
use crate::types::Color;

impl ColorFormat {
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
        let len = self.bytes_per_pixel();
        bytes[offset..offset + len].copy_from_slice(&packed[..len]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                let mut bytes = [0xa5; 8];
                format.write_pixel(&mut bytes, 2, &color);
                let end = 2 + format.bytes_per_pixel();
                assert_eq!(&bytes[..2], &[0xa5; 2]);
                assert!(bytes[end..].iter().all(|byte| *byte == 0xa5));
                assert_eq!(
                    &bytes[2..end],
                    &format.pack(&color).to_le_bytes()[..format.bytes_per_pixel()]
                );
                let decoded = format.read_pixel(&bytes, 2);
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
