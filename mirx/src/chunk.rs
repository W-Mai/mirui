use alloc::vec;
use alloc::vec::Vec;

use crate::crc32;
use crate::error::ParseError;
use crate::format::ColorFormat;
use crate::header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkEntry, ChunkFileHeader, FILE_HEADER_LEN,
    FileHeader, ImageChunkHeader, Layout, VERSION_MAJOR, VERSION_MINOR, chunk_type,
};

/// Borrows the chunk table and IMAGE chunk pixel data from the input buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkFile<'a> {
    pub header: ChunkFileHeader,
    pub entries: Vec<ChunkEntry>,
    /// `None` does not mean "no chunks": callers can still walk `entries`
    /// to resolve non-IMAGE chunks manually.
    pub primary_image: Option<ImageChunk<'a>>,
}

/// Only raw (`compress = 0`) IMAGE chunks are decoded; compressed chunks
/// surface as [`ParseError::UnsupportedCompression`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageChunk<'a> {
    pub width: u32,
    pub height: u32,
    pub format: ColorFormat,
    pub stride: u32,
    /// Main pixel stream; for RGB565A8 this is the RGB565 stream and the
    /// A8 plane lives in `extra`.
    pub data: &'a [u8],
    /// RGB565A8 A8 plane, `None` for other formats.
    pub extra: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageChunkInput<'a> {
    pub width: u32,
    pub height: u32,
    pub format: ColorFormat,
    pub stride: u32,
    pub main: &'a [u8],
    pub extra: Option<&'a [u8]>,
}

pub fn parse_chunk(buf: &[u8]) -> Result<ChunkFile<'_>, ParseError> {
    let file = FileHeader::parse(buf)?;
    if file.layout != Layout::Chunk {
        return Err(ParseError::UnknownLayout(file.layout.to_u8()));
    }
    if buf.len() < CHUNK_FILE_HEADER_LEN {
        return Err(ParseError::Truncated);
    }

    if buf[10] != 0 || buf[11] != 0 || buf[23] != 0 {
        return Err(ParseError::ReservedNonZero);
    }
    if buf[36..40].iter().any(|b| *b != 0) {
        return Err(ParseError::ReservedNonZero);
    }

    let chunk_count = u16::from_le_bytes([buf[8], buf[9]]);
    let chunk_table_offset = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let file_size = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let primary_chunk_type = u16::from_le_bytes([buf[20], buf[21]]);
    let primary_color_format = buf[22];
    let primary_width = u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);
    let primary_height = u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]);
    let primary_stride = u32::from_le_bytes([buf[32], buf[33], buf[34], buf[35]]);

    let stored_crc = u32::from_le_bytes([buf[40], buf[41], buf[42], buf[43]]);
    let actual_crc = crc32(&buf[..40]);
    if stored_crc != actual_crc {
        return Err(ParseError::HeaderCrcMismatch {
            expected: stored_crc,
            actual: actual_crc,
        });
    }

    let header = ChunkFileHeader {
        file,
        chunk_count,
        chunk_table_offset,
        file_size,
        primary_chunk_type,
        primary_color_format,
        primary_width,
        primary_height,
        primary_stride,
        header_crc32: stored_crc,
    };

    let table_start = chunk_table_offset as usize;
    let table_bytes_needed = (chunk_count as usize)
        .checked_mul(CHUNK_TABLE_ENTRY_LEN)
        .ok_or(ParseError::DimensionOverflow)?;
    let table_end = table_start
        .checked_add(table_bytes_needed)
        .ok_or(ParseError::DimensionOverflow)?;
    if buf.len() < table_end {
        return Err(ParseError::Truncated);
    }

    let mut entries = Vec::with_capacity(chunk_count as usize);
    for i in 0..chunk_count as usize {
        let entry_start = table_start + i * CHUNK_TABLE_ENTRY_LEN;
        let e = &buf[entry_start..entry_start + CHUNK_TABLE_ENTRY_LEN];
        if e[12..16].iter().any(|b| *b != 0) {
            return Err(ParseError::ReservedNonZero);
        }
        let entry = ChunkEntry {
            chunk_type: u16::from_le_bytes([e[0], e[1]]),
            chunk_flags: u16::from_le_bytes([e[2], e[3]]),
            chunk_offset: u32::from_le_bytes([e[4], e[5], e[6], e[7]]),
            chunk_size: u32::from_le_bytes([e[8], e[9], e[10], e[11]]),
        };
        if entry.chunk_type != chunk_type::IMAGE
            && entry.chunk_type != chunk_type::META
            && entry.is_critical()
        {
            return Err(ParseError::UnknownCriticalChunk(entry.chunk_type));
        }
        entries.push(entry);
    }

    // Locate and parse the primary IMAGE chunk if its declared chunk_type
    // matches the hint. Other chunks are surfaced via `entries` only.
    let primary_image = entries
        .iter()
        .find(|e| e.chunk_type == chunk_type::IMAGE && e.chunk_type == primary_chunk_type)
        .map(|e| parse_image_chunk(buf, e))
        .transpose()?;

    Ok(ChunkFile {
        header,
        entries,
        primary_image,
    })
}

fn parse_image_chunk<'a>(buf: &'a [u8], entry: &ChunkEntry) -> Result<ImageChunk<'a>, ParseError> {
    let chunk_start = entry.chunk_offset as usize;
    let chunk_end = chunk_start
        .checked_add(entry.chunk_size as usize)
        .ok_or(ParseError::DimensionOverflow)?;
    if buf.len() < chunk_end || entry.chunk_size < ImageChunkHeader::SIZE as u32 {
        return Err(ParseError::Truncated);
    }

    let h = &buf[chunk_start..chunk_start + ImageChunkHeader::SIZE];
    if h[10] != 0 || h[11] != 0 {
        return Err(ParseError::ReservedNonZero);
    }
    if h[28..32].iter().any(|b| *b != 0) {
        return Err(ParseError::ReservedNonZero);
    }

    let width = u32::from_le_bytes([h[0], h[1], h[2], h[3]]);
    let height = u32::from_le_bytes([h[4], h[5], h[6], h[7]]);
    let format_byte = h[8];
    let compress = h[9];
    let stride = u32::from_le_bytes([h[12], h[13], h[14], h[15]]);
    let data_offset = u32::from_le_bytes([h[16], h[17], h[18], h[19]]);
    let data_size = u32::from_le_bytes([h[20], h[21], h[22], h[23]]);
    let extra_data_size = u32::from_le_bytes([h[24], h[25], h[26], h[27]]);

    if compress != 0 {
        return Err(ParseError::UnsupportedCompression(compress));
    }

    let format =
        ColorFormat::from_u8(format_byte).ok_or(ParseError::UnknownColorFormat(format_byte))?;

    let main_size = data_size
        .checked_sub(extra_data_size)
        .ok_or(ParseError::DimensionOverflow)? as usize;
    let abs_data_start = chunk_start + data_offset as usize;
    let abs_data_end = abs_data_start
        .checked_add(data_size as usize)
        .ok_or(ParseError::DimensionOverflow)?;
    if buf.len() < abs_data_end {
        return Err(ParseError::Truncated);
    }
    let data = &buf[abs_data_start..abs_data_start + main_size];
    let extra = if extra_data_size > 0 {
        Some(
            &buf[abs_data_start + main_size..abs_data_start + main_size + extra_data_size as usize],
        )
    } else {
        None
    };

    Ok(ImageChunk {
        width,
        height,
        format,
        stride,
        data,
        extra,
    })
}

/// Emits one raw IMAGE chunk; multi-chunk files need a different writer.
pub fn encode_chunk_image(image: &ImageChunkInput<'_>) -> Vec<u8> {
    let main_size = image.main.len();
    let extra_size = image.extra.map(|e| e.len()).unwrap_or(0);
    let data_size = main_size + extra_size;

    let chunk_table_offset = CHUNK_FILE_HEADER_LEN as u32;
    let chunk_start = chunk_table_offset as usize + CHUNK_TABLE_ENTRY_LEN;
    let raw_data_start = chunk_start + ImageChunkHeader::SIZE;
    // Align data start to 4 bytes so on-disk pixel data is amenable to
    // word-sized loads on architectures that fault on misaligned reads.
    let aligned_data_start = (raw_data_start + 3) & !3;
    let pad = aligned_data_start - raw_data_start;
    let data_offset = (ImageChunkHeader::SIZE + pad) as u32;
    let chunk_size = ImageChunkHeader::SIZE + pad + data_size;
    let file_size = (aligned_data_start + data_size) as u32;

    let mut out = vec![0u8; file_size as usize];

    let file_header = FileHeader {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        layout: Layout::Chunk,
        flags: 0,
    };
    let mut prefix = [0u8; FILE_HEADER_LEN];
    file_header.write_into(&mut prefix);
    out[0..FILE_HEADER_LEN].copy_from_slice(&prefix);

    out[8..10].copy_from_slice(&1u16.to_le_bytes());
    out[12..16].copy_from_slice(&chunk_table_offset.to_le_bytes());
    out[16..20].copy_from_slice(&file_size.to_le_bytes());
    out[20..22].copy_from_slice(&chunk_type::IMAGE.to_le_bytes());
    out[22] = image.format.to_u8();
    out[24..28].copy_from_slice(&image.width.to_le_bytes());
    out[28..32].copy_from_slice(&image.height.to_le_bytes());
    out[32..36].copy_from_slice(&image.stride.to_le_bytes());
    let crc = crc32(&out[..40]);
    out[40..44].copy_from_slice(&crc.to_le_bytes());

    let entry_off = chunk_table_offset as usize;
    out[entry_off..entry_off + 2].copy_from_slice(&chunk_type::IMAGE.to_le_bytes());
    out[entry_off + 2..entry_off + 4].copy_from_slice(&0u16.to_le_bytes());
    out[entry_off + 4..entry_off + 8].copy_from_slice(&(chunk_start as u32).to_le_bytes());
    out[entry_off + 8..entry_off + 12].copy_from_slice(&(chunk_size as u32).to_le_bytes());

    out[chunk_start..chunk_start + 4].copy_from_slice(&image.width.to_le_bytes());
    out[chunk_start + 4..chunk_start + 8].copy_from_slice(&image.height.to_le_bytes());
    out[chunk_start + 8] = image.format.to_u8();
    out[chunk_start + 9] = 0;
    out[chunk_start + 12..chunk_start + 16].copy_from_slice(&image.stride.to_le_bytes());
    out[chunk_start + 16..chunk_start + 20].copy_from_slice(&data_offset.to_le_bytes());
    out[chunk_start + 20..chunk_start + 24].copy_from_slice(&(data_size as u32).to_le_bytes());
    out[chunk_start + 24..chunk_start + 28].copy_from_slice(&(extra_size as u32).to_le_bytes());

    out[aligned_data_start..aligned_data_start + main_size].copy_from_slice(image.main);
    if let Some(extra) = image.extra {
        out[aligned_data_start + main_size..aligned_data_start + main_size + extra_size]
            .copy_from_slice(extra);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn chunk_round_trip_rgb565() {
        let pixels = vec![0u8; 32]; // 4x4 RGB565 = stride 8, 4 rows
        let input = ImageChunkInput {
            width: 4,
            height: 4,
            format: ColorFormat::RGB565,
            stride: 8,
            main: &pixels,
            extra: None,
        };
        let encoded = encode_chunk_image(&input);
        let parsed = parse_chunk(&encoded).unwrap();
        assert_eq!(parsed.header.chunk_count, 1);
        assert_eq!(parsed.header.primary_chunk_type, chunk_type::IMAGE);
        assert_eq!(
            parsed.header.primary_color_format,
            ColorFormat::RGB565.to_u8()
        );
        assert_eq!(parsed.header.primary_width, 4);
        assert_eq!(parsed.header.primary_height, 4);
        assert_eq!(parsed.header.primary_stride, 8);

        let img = parsed.primary_image.unwrap();
        assert_eq!(img.width, 4);
        assert_eq!(img.height, 4);
        assert_eq!(img.format, ColorFormat::RGB565);
        assert_eq!(img.stride, 8);
        assert_eq!(img.data, pixels.as_slice());
        assert!(img.extra.is_none());
    }

    #[test]
    fn chunk_round_trip_rgb565a8() {
        let main = vec![0xAAu8; 32]; // 4x4 RGB565
        let alpha = vec![0x80u8; 16]; // 4x4 A8
        let input = ImageChunkInput {
            width: 4,
            height: 4,
            format: ColorFormat::RGB565A8,
            stride: 8,
            main: &main,
            extra: Some(&alpha),
        };
        let encoded = encode_chunk_image(&input);
        let parsed = parse_chunk(&encoded).unwrap();
        let img = parsed.primary_image.unwrap();
        assert_eq!(img.data, main.as_slice());
        assert_eq!(img.extra, Some(alpha.as_slice()));
    }

    #[test]
    fn chunk_zero_copy_data_borrow() {
        let pixels = vec![0u8; 8];
        let input = ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &pixels,
            extra: None,
        };
        let encoded = encode_chunk_image(&input);
        let parsed = parse_chunk(&encoded).unwrap();
        let img = parsed.primary_image.unwrap();
        let img_ptr = img.data.as_ptr();
        let buf_start = encoded.as_ptr();
        let offset = img_ptr as usize - buf_start as usize;
        // Pixel data sits after CHUNK header (44) + chunk table (16) + IMAGE
        // inner header (32) = 92, padded up to a multiple of 4 = 92 already.
        assert_eq!(offset, 92);
    }

    #[test]
    fn chunk_truncated_buffer_is_rejected() {
        let pixels = vec![0u8; 8];
        let input = ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &pixels,
            extra: None,
        };
        let encoded = encode_chunk_image(&input);
        let truncated = &encoded[..encoded.len() - 1];
        assert!(matches!(parse_chunk(truncated), Err(ParseError::Truncated)));
    }

    #[test]
    fn chunk_header_crc_mismatch_is_rejected() {
        let pixels = vec![0u8; 8];
        let input = ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &pixels,
            extra: None,
        };
        let mut encoded = encode_chunk_image(&input);
        encoded[24] ^= 0xFF; // tweak primary_width hint before CRC field
        assert!(matches!(
            parse_chunk(&encoded),
            Err(ParseError::HeaderCrcMismatch { .. })
        ));
    }

    #[test]
    fn chunk_unsupported_compression_is_rejected() {
        let pixels = vec![0u8; 8];
        let input = ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &pixels,
            extra: None,
        };
        let mut encoded = encode_chunk_image(&input);
        // IMAGE chunk inner header starts at offset 44 + 16 = 60; compress
        // byte sits at 60 + 9 = 69.
        encoded[69] = 1;
        assert!(matches!(
            parse_chunk(&encoded),
            Err(ParseError::UnsupportedCompression(1))
        ));
    }

    #[test]
    fn chunk_extra_size_exceeding_data_size_is_rejected() {
        // Crafts a CHUNK file whose inner IMAGE header has
        // extra_data_size > data_size. The inner header is not covered
        // by the file-level CRC, so an attacker can flip these bytes
        // freely; the parser must reject before the subtraction.
        let pixels = vec![0u8; 8];
        let input = ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &pixels,
            extra: None,
        };
        let mut encoded = encode_chunk_image(&input);
        // Inner header offset 24..28 = extra_data_size (u32 LE). Set it
        // bigger than data_size (offset 20..24) — here data_size = 8.
        encoded[84..88].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            parse_chunk(&encoded),
            Err(ParseError::DimensionOverflow)
        ));
    }
}
