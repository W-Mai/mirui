/// Pixel format byte stored at offset 8 in the FLAT header and at offset 8
/// in the IMAGE chunk inner header. Values are byte-stable on disk; once a
/// format ships with a given value, the value is frozen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PixelFormat {
    L8 = 0x10,
    I1 = 0x11,
    I2 = 0x12,
    I4 = 0x13,
    I8 = 0x14,
    A1 = 0x15,
    A2 = 0x16,
    A4 = 0x17,
    A8 = 0x18,
    Rgb565 = 0x20,
    Rgb565a8 = 0x21,
    Rgb888 = 0x22,
    Xrgb8888 = 0x23,
    Argb8888 = 0x24,
}

/// Sentinel byte used in the CHUNK header `primary_color_format` slot when
/// the chunk has no fixed pixel format (e.g. a VECTOR chunk).
pub const PRIMARY_FORMAT_NONE: u8 = 0xFF;

impl PixelFormat {
    pub const fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x10 => Self::L8,
            0x11 => Self::I1,
            0x12 => Self::I2,
            0x13 => Self::I4,
            0x14 => Self::I8,
            0x15 => Self::A1,
            0x16 => Self::A2,
            0x17 => Self::A4,
            0x18 => Self::A8,
            0x20 => Self::Rgb565,
            0x21 => Self::Rgb565a8,
            0x22 => Self::Rgb888,
            0x23 => Self::Xrgb8888,
            0x24 => Self::Argb8888,
            _ => return None,
        })
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// Number of palette entries for indexed formats; `None` for everything else.
    pub const fn palette_entries(self) -> Option<u32> {
        match self {
            Self::I1 => Some(2),
            Self::I2 => Some(4),
            Self::I4 => Some(16),
            Self::I8 => Some(256),
            _ => None,
        }
    }

    /// FLAT-mode `extra` region size in bytes for a given format and (when
    /// relevant) image dimensions / main-region stride. Returns `None` on
    /// arithmetic overflow.
    ///
    /// - Indexed (I1/I2/I4/I8): palette = `entries * 4` (ARGB8888 entries).
    /// - RGB565A8: `stride_alpha * height` where `stride_alpha` is derived
    ///   from the main stride alignment unit (`stride / 2`, rounded up to
    ///   `width`). The simplification used here matches the spec's "no
    ///   padding" case: `stride_alpha = width`.
    /// - All other formats: `0`.
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
            Self::Rgb565a8 => match width.checked_mul(height) {
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
            if let Some(fmt) = PixelFormat::from_u8(byte) {
                assert_eq!(fmt.to_u8(), byte, "from_u8({byte:#x}) round-trips");
            }
        }
    }

    #[test]
    fn unknown_byte_is_none() {
        assert!(PixelFormat::from_u8(0x00).is_none());
        assert!(PixelFormat::from_u8(0x19).is_none());
        assert!(PixelFormat::from_u8(0x25).is_none());
        assert!(PixelFormat::from_u8(0xFF).is_none());
    }

    #[test]
    fn palette_entries_match_bpp() {
        assert_eq!(PixelFormat::I1.palette_entries(), Some(2));
        assert_eq!(PixelFormat::I2.palette_entries(), Some(4));
        assert_eq!(PixelFormat::I4.palette_entries(), Some(16));
        assert_eq!(PixelFormat::I8.palette_entries(), Some(256));
        assert_eq!(PixelFormat::Rgb565.palette_entries(), None);
    }

    #[test]
    fn extra_size_for_indexed_formats() {
        assert_eq!(PixelFormat::I1.extra_size(64, 64, 8), Some(8));
        assert_eq!(PixelFormat::I2.extra_size(64, 64, 16), Some(16));
        assert_eq!(PixelFormat::I4.extra_size(64, 64, 32), Some(64));
        assert_eq!(PixelFormat::I8.extra_size(64, 64, 64), Some(1024));
    }

    #[test]
    fn extra_size_for_rgb565a8_is_alpha_plane() {
        assert_eq!(PixelFormat::Rgb565a8.extra_size(16, 16, 32), Some(256));
    }

    #[test]
    fn extra_size_for_simple_formats_is_zero() {
        assert_eq!(PixelFormat::Rgb565.extra_size(100, 50, 200), Some(0));
        assert_eq!(PixelFormat::Argb8888.extra_size(1, 1, 4), Some(0));
        assert_eq!(PixelFormat::A8.extra_size(8, 8, 8), Some(0));
    }
}
