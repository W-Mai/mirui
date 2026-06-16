use alloc::vec::Vec;

use crate::crc32;
use crate::error::ParseError;
use crate::format::PixelFormat;
use crate::header::{
    FILE_HEADER_LEN, FLAT_HEADER_LEN, FileHeader, Layout, VERSION_MAJOR, VERSION_MINOR,
};

/// Borrows pixel data from the input buffer; no allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatImage<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    /// Main pixel stream (`stride * height` bytes); for RGB565A8 this is
    /// just the RGB565 stream and the A8 plane lives in `extra`.
    pub main: &'a [u8],
    /// Palette for indexed formats, A8 plane for RGB565A8, `None` otherwise.
    pub extra: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatImageInput<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: PixelFormat,
    pub main: &'a [u8],
    pub extra: Option<&'a [u8]>,
}

pub fn parse_flat(buf: &[u8]) -> Result<FlatImage<'_>, ParseError> {
    let file = FileHeader::parse(buf)?;
    if file.layout != Layout::Flat {
        return Err(ParseError::UnknownLayout(file.layout.to_u8()));
    }
    if buf.len() < FLAT_HEADER_LEN {
        return Err(ParseError::Truncated);
    }

    // Reserved bytes must be zero per spec.
    if buf[9] != 0 || buf[10] != 0 || buf[11] != 0 {
        return Err(ParseError::ReservedNonZero);
    }

    let format_byte = buf[8];
    let format =
        PixelFormat::from_u8(format_byte).ok_or(ParseError::UnknownColorFormat(format_byte))?;
    let width = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let height = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let stride = u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);

    let stored_crc = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    let actual_crc = crc32(&buf[..24]);
    if stored_crc != actual_crc {
        return Err(ParseError::HeaderCrcMismatch {
            expected: stored_crc,
            actual: actual_crc,
        });
    }

    let main_size = (stride as usize)
        .checked_mul(height as usize)
        .ok_or(ParseError::DimensionOverflow)?;
    let extra_size = format
        .extra_size(width, height, stride)
        .ok_or(ParseError::DimensionOverflow)? as usize;

    let total_needed = FLAT_HEADER_LEN
        .checked_add(main_size)
        .and_then(|n| n.checked_add(extra_size))
        .ok_or(ParseError::DimensionOverflow)?;
    if buf.len() < total_needed {
        return Err(ParseError::Truncated);
    }

    let main = &buf[FLAT_HEADER_LEN..FLAT_HEADER_LEN + main_size];
    let extra = if extra_size > 0 {
        Some(&buf[FLAT_HEADER_LEN + main_size..FLAT_HEADER_LEN + main_size + extra_size])
    } else {
        None
    };

    Ok(FlatImage {
        width,
        height,
        stride,
        format,
        main,
        extra,
    })
}

pub fn encode_flat(input: &FlatImageInput<'_>) -> Vec<u8> {
    let main_size = (input.stride as usize) * (input.height as usize);
    let expected_extra_size = input
        .format
        .extra_size(input.width, input.height, input.stride)
        .unwrap_or(0) as usize;

    debug_assert_eq!(
        input.main.len(),
        main_size,
        "main buffer size mismatches stride*height"
    );
    debug_assert_eq!(
        input.extra.map(|e| e.len()).unwrap_or(0),
        expected_extra_size,
        "extra buffer size mismatches format-derived size",
    );

    let total_size = FLAT_HEADER_LEN + main_size + expected_extra_size;
    let mut out = Vec::with_capacity(total_size);
    out.resize(FLAT_HEADER_LEN, 0);

    let file = FileHeader {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        layout: Layout::Flat,
        flags: 0,
    };
    let mut prefix = [0u8; FILE_HEADER_LEN];
    file.write_into(&mut prefix);
    out[0..FILE_HEADER_LEN].copy_from_slice(&prefix);

    out[8] = input.format.to_u8();
    out[12..16].copy_from_slice(&input.width.to_le_bytes());
    out[16..20].copy_from_slice(&input.height.to_le_bytes());
    out[20..24].copy_from_slice(&input.stride.to_le_bytes());

    let crc = crc32(&out[..24]);
    out[24..28].copy_from_slice(&crc.to_le_bytes());

    out.extend_from_slice(input.main);
    if let Some(extra) = input.extra {
        out.extend_from_slice(extra);
    }

    debug_assert_eq!(out.len(), total_size);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    static RGB565_2X1: [u8; 4] = [0x00, 0xF8, 0xE0, 0x07];

    fn rgb565_2x1_input() -> FlatImageInput<'static> {
        FlatImageInput {
            width: 2,
            height: 1,
            stride: 4,
            format: PixelFormat::Rgb565,
            main: &RGB565_2X1,
            extra: None,
        }
    }

    #[test]
    fn flat_round_trip_rgb565() {
        let input = rgb565_2x1_input();
        let encoded = encode_flat(&input);
        let parsed = parse_flat(&encoded).unwrap();
        assert_eq!(parsed.width, 2);
        assert_eq!(parsed.height, 1);
        assert_eq!(parsed.stride, 4);
        assert_eq!(parsed.format, PixelFormat::Rgb565);
        assert_eq!(parsed.main, input.main);
        assert!(parsed.extra.is_none());
    }

    #[test]
    fn flat_round_trip_indexed_with_palette() {
        // I4: 4-bit indices, 16-entry palette, 8 pixels in 4 bytes
        static PIXELS: [u8; 4] = [0x12, 0x34, 0x56, 0x78];
        static PALETTE: [u8; 64] = [0xFF; 64];

        let input = FlatImageInput {
            width: 8,
            height: 1,
            stride: 4,
            format: PixelFormat::I4,
            main: &PIXELS,
            extra: Some(&PALETTE),
        };
        let encoded = encode_flat(&input);
        let parsed = parse_flat(&encoded).unwrap();
        assert_eq!(parsed.format, PixelFormat::I4);
        assert_eq!(parsed.main, &PIXELS);
        assert_eq!(parsed.extra, Some(&PALETTE[..]));
    }

    #[test]
    fn flat_round_trip_rgb565a8() {
        // 4x2 image: RGB565 main is stride*h = 8*2 = 16, alpha plane is w*h = 8
        let main = vec![0xAAu8; 16];
        let alpha = vec![0x80u8; 8];

        let input = FlatImageInput {
            width: 4,
            height: 2,
            stride: 8,
            format: PixelFormat::Rgb565a8,
            main: &main,
            extra: Some(&alpha),
        };
        let encoded = encode_flat(&input);
        let parsed = parse_flat(&encoded).unwrap();
        assert_eq!(parsed.format, PixelFormat::Rgb565a8);
        assert_eq!(parsed.main, main.as_slice());
        assert_eq!(parsed.extra, Some(alpha.as_slice()));
    }

    #[test]
    fn flat_zero_copy_main_borrow() {
        let input = rgb565_2x1_input();
        let encoded = encode_flat(&input);
        let parsed = parse_flat(&encoded).unwrap();
        // Parsed `main` must be a slice into `encoded`, not a fresh Vec.
        let main_ptr = parsed.main.as_ptr();
        let buf_start = encoded.as_ptr();
        let offset = main_ptr as usize - buf_start as usize;
        assert_eq!(offset, FLAT_HEADER_LEN);
    }

    #[test]
    fn truncated_buffer_is_rejected() {
        let input = rgb565_2x1_input();
        let encoded = encode_flat(&input);
        let truncated = &encoded[..encoded.len() - 1];
        assert!(matches!(parse_flat(truncated), Err(ParseError::Truncated)));
    }

    #[test]
    fn header_crc_mismatch_is_rejected() {
        let input = rgb565_2x1_input();
        let mut encoded = encode_flat(&input);
        // Flip a header bit before the CRC field — CRC will mismatch
        encoded[12] ^= 0xFF;
        assert!(matches!(
            parse_flat(&encoded),
            Err(ParseError::HeaderCrcMismatch { .. })
        ));
    }

    #[test]
    fn unknown_color_format_is_rejected() {
        let input = rgb565_2x1_input();
        let mut encoded = encode_flat(&input);
        encoded[8] = 0x99;
        // Recompute CRC so we hit the format check, not the CRC check
        let crc = crc32(&encoded[..24]);
        encoded[24..28].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_flat(&encoded),
            Err(ParseError::UnknownColorFormat(0x99))
        ));
    }

    #[test]
    fn reserved_bytes_must_be_zero() {
        let input = rgb565_2x1_input();
        let mut encoded = encode_flat(&input);
        encoded[9] = 0x42;
        let crc = crc32(&encoded[..24]);
        encoded[24..28].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_flat(&encoded),
            Err(ParseError::ReservedNonZero)
        ));
    }

    #[test]
    fn wrong_layout_is_rejected() {
        let input = rgb565_2x1_input();
        let mut encoded = encode_flat(&input);
        encoded[6] = 0x01;
        // Recompute CRC after layout change so we hit the layout check
        let crc = crc32(&encoded[..24]);
        encoded[24..28].copy_from_slice(&crc.to_le_bytes());
        assert!(matches!(
            parse_flat(&encoded),
            Err(ParseError::UnknownLayout(0x01))
        ));
    }
}
