/// On-disk discriminants are byte-stable once shipped. Naming and byte-order
/// match `mirui::ColorFormat` so reader output is zero-copy on the mirui side.
/// Discriminants are partitioned by family: `byte >> 4` identifies the family
/// (1 = indexed, 2 = alpha-only, 3 = luma, 4 = RGB565, 5 = RGB888, 6 = 32-bit),
/// leaving the rest of each 16-byte run open for same-family extensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum ColorFormat {
    I1 = 0x10,
    I2 = 0x11,
    I4 = 0x12,
    I8 = 0x13,

    A1 = 0x20,
    A2 = 0x21,
    A4 = 0x22,
    A8 = 0x23,

    L8 = 0x30,

    RGB565 = 0x40,
    RGB565Swapped = 0x41,
    RGB565A8 = 0x42,

    RGB888 = 0x50,

    XRGB8888 = 0x60,
    RGBA8888 = 0x61,
    BGRA8888 = 0x62,
}

/// Sentinel byte stored in the CHUNK header `primary_color_format` slot when
/// the chunk has no fixed pixel format (e.g. a VECTOR chunk).
pub const PRIMARY_FORMAT_NONE: u8 = 0xFF;

impl ColorFormat {
    pub const fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x10 => Self::I1,
            0x11 => Self::I2,
            0x12 => Self::I4,
            0x13 => Self::I8,
            0x20 => Self::A1,
            0x21 => Self::A2,
            0x22 => Self::A4,
            0x23 => Self::A8,
            0x30 => Self::L8,
            0x40 => Self::RGB565,
            0x41 => Self::RGB565Swapped,
            0x42 => Self::RGB565A8,
            0x50 => Self::RGB888,
            0x60 => Self::XRGB8888,
            0x61 => Self::RGBA8888,
            0x62 => Self::BGRA8888,
            _ => return None,
        })
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    pub const fn palette_entries(self) -> Option<u32> {
        match self {
            Self::I1 => Some(2),
            Self::I2 => Some(4),
            Self::I4 => Some(16),
            Self::I8 => Some(256),
            _ => None,
        }
    }

    /// Number of bits used by one pixel in the main plane.
    ///
    /// Formats with an extra plane report only their main-plane depth.
    pub const fn bits_per_pixel(self) -> u8 {
        match self {
            Self::I1 | Self::A1 => 1,
            Self::I2 | Self::A2 => 2,
            Self::I4 | Self::A4 => 4,
            Self::I8 | Self::A8 | Self::L8 => 8,
            Self::RGB565 | Self::RGB565Swapped | Self::RGB565A8 => 16,
            Self::RGB888 => 24,
            Self::XRGB8888 | Self::RGBA8888 | Self::BGRA8888 => 32,
        }
    }

    /// Smallest valid main-plane stride for one row of `width` pixels.
    pub const fn minimum_stride(self, width: u32) -> Option<u32> {
        let bits = width as u64 * self.bits_per_pixel() as u64;
        let bytes = bits.div_ceil(8);
        if bytes > u32::MAX as u64 {
            None
        } else {
            Some(bytes as u32)
        }
    }

    /// FLAT extra bytes; RGB565A8 intentionally uses the v1 no-padding alpha
    /// plane, so `stride` is ignored.
    pub const fn extra_size(self, width: u32, height: u32, _stride: u32) -> Option<u32> {
        match self {
            Self::I1 | Self::I2 | Self::I4 | Self::I8 => {
                let entries = match self {
                    Self::I1 => 2,
                    Self::I2 => 4,
                    Self::I4 => 16,
                    Self::I8 => 256,
                    _ => 0,
                };
                Some(entries * 4)
            }
            Self::RGB565A8 => match width.checked_mul(height) {
                Some(n) => Some(n),
                None => None,
            },
            _ => Some(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_byte_values() {
        for byte in 0u8..=255 {
            if let Some(fmt) = ColorFormat::from_u8(byte) {
                assert_eq!(fmt.to_u8(), byte, "from_u8({byte:#x}) round-trips");
            }
        }
    }

    #[test]
    fn unknown_byte_is_none() {
        assert!(ColorFormat::from_u8(0x00).is_none());
        assert!(ColorFormat::from_u8(0x14).is_none());
        assert!(ColorFormat::from_u8(0x24).is_none());
        assert!(ColorFormat::from_u8(0x63).is_none());
        assert!(ColorFormat::from_u8(0xFF).is_none());
    }

    #[test]
    fn palette_entries_match_bpp() {
        assert_eq!(ColorFormat::I1.palette_entries(), Some(2));
        assert_eq!(ColorFormat::I2.palette_entries(), Some(4));
        assert_eq!(ColorFormat::I4.palette_entries(), Some(16));
        assert_eq!(ColorFormat::I8.palette_entries(), Some(256));
        assert_eq!(ColorFormat::RGB565.palette_entries(), None);
    }

    #[test]
    fn extra_size_for_indexed_formats() {
        assert_eq!(ColorFormat::I1.extra_size(64, 64, 8), Some(8));
        assert_eq!(ColorFormat::I2.extra_size(64, 64, 16), Some(16));
        assert_eq!(ColorFormat::I4.extra_size(64, 64, 32), Some(64));
        assert_eq!(ColorFormat::I8.extra_size(64, 64, 64), Some(1024));
    }

    #[test]
    fn extra_size_for_rgb565a8_is_alpha_plane() {
        assert_eq!(ColorFormat::RGB565A8.extra_size(16, 16, 32), Some(256));
    }

    #[test]
    fn extra_size_for_simple_formats_is_zero() {
        assert_eq!(ColorFormat::RGB565.extra_size(100, 50, 200), Some(0));
        assert_eq!(ColorFormat::RGBA8888.extra_size(1, 1, 4), Some(0));
        assert_eq!(ColorFormat::BGRA8888.extra_size(2, 2, 8), Some(0));
        assert_eq!(ColorFormat::A8.extra_size(8, 8, 8), Some(0));
    }

    #[test]
    fn minimum_stride_covers_every_format_family() {
        assert_eq!(ColorFormat::I1.minimum_stride(9), Some(2));
        assert_eq!(ColorFormat::A1.minimum_stride(8), Some(1));
        assert_eq!(ColorFormat::I2.minimum_stride(5), Some(2));
        assert_eq!(ColorFormat::A2.minimum_stride(4), Some(1));
        assert_eq!(ColorFormat::I4.minimum_stride(3), Some(2));
        assert_eq!(ColorFormat::A4.minimum_stride(2), Some(1));
        assert_eq!(ColorFormat::I8.minimum_stride(7), Some(7));
        assert_eq!(ColorFormat::A8.minimum_stride(7), Some(7));
        assert_eq!(ColorFormat::L8.minimum_stride(7), Some(7));
        assert_eq!(ColorFormat::RGB565.minimum_stride(7), Some(14));
        assert_eq!(ColorFormat::RGB565Swapped.minimum_stride(7), Some(14));
        assert_eq!(ColorFormat::RGB565A8.minimum_stride(7), Some(14));
        assert_eq!(ColorFormat::RGB888.minimum_stride(7), Some(21));
        assert_eq!(ColorFormat::XRGB8888.minimum_stride(7), Some(28));
        assert_eq!(ColorFormat::RGBA8888.minimum_stride(7), Some(28));
        assert_eq!(ColorFormat::BGRA8888.minimum_stride(7), Some(28));
        assert_eq!(ColorFormat::A8.minimum_stride(u32::MAX), Some(u32::MAX));
        assert_eq!(ColorFormat::RGBA8888.minimum_stride(u32::MAX), None);
        assert_eq!(ColorFormat::RGB565.minimum_stride(u32::MAX), None);
        assert_eq!(ColorFormat::RGB888.minimum_stride(u32::MAX), None);
    }

    #[test]
    fn bits_per_pixel_describes_the_main_plane() {
        assert_eq!(ColorFormat::I1.bits_per_pixel(), 1);
        assert_eq!(ColorFormat::A2.bits_per_pixel(), 2);
        assert_eq!(ColorFormat::I4.bits_per_pixel(), 4);
        assert_eq!(ColorFormat::L8.bits_per_pixel(), 8);
        assert_eq!(ColorFormat::RGB565.bits_per_pixel(), 16);
        assert_eq!(ColorFormat::RGB565A8.bits_per_pixel(), 16);
        assert_eq!(ColorFormat::RGB888.bits_per_pixel(), 24);
        assert_eq!(ColorFormat::RGBA8888.bits_per_pixel(), 32);
    }
}
