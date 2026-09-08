use alloc::vec;
use alloc::vec::Vec;

use crate::crc32;
#[cfg(test)]
use crate::error::ParseError;
#[cfg(test)]
use crate::format::ColorFormat;
use crate::header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, FILE_HEADER_LEN, FileHeader, Layout,
    VERSION_MAJOR, VERSION_MINOR,
};
#[cfg(test)]
use crate::header::{ChunkEntry, ChunkFileHeader, chunk_type};

/// Borrows the chunk table and IMAGE chunk pixel data from the input buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub struct ChunkFile<'a> {
    pub header: ChunkFileHeader,
    pub entries: Vec<ChunkEntry>,
    /// `None` does not mean "no chunks": callers can still walk `entries`
    /// to resolve non-IMAGE chunks manually.
    pub primary_image: Option<ImageChunk<'a>>,
}

#[cfg(test)]
impl<'a> ChunkFile<'a> {
    /// First chunk payload of `chunk_type` referencing the original buffer
    /// `buf`. Returns `None` if no entry of that type exists or the entry's
    /// declared offset/size falls outside the buffer.
    pub fn chunk_payload(&self, buf: &'a [u8], chunk_type: u16) -> Option<&'a [u8]> {
        let entry = self.entries.iter().find(|e| e.chunk_type == chunk_type)?;
        let start = entry.chunk_offset as usize;
        let end = start.checked_add(entry.chunk_size as usize)?;
        buf.get(start..end)
    }

    /// Every chunk payload of `chunk_type`, in table order. Lets a
    /// caller pick among multiple same-typed chunks (e.g. several FONT
    /// faces) by inspecting each payload. Entries
    /// whose declared range falls outside `buf` are skipped.
    pub fn chunk_payloads(
        &self,
        buf: &'a [u8],
        chunk_type: u16,
    ) -> impl Iterator<Item = &'a [u8]> + '_ {
        self.entries
            .iter()
            .filter(move |e| e.chunk_type == chunk_type)
            .filter_map(move |e| {
                let start = e.chunk_offset as usize;
                let end = start.checked_add(e.chunk_size as usize)?;
                buf.get(start..end)
            })
    }
}

/// Only RAW IMAGE payloads with a packed surface are decoded; other payloads
/// surface as [`ParseError::InvalidImage`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub struct ImageChunk<'a> {
    pub width: u32,
    pub height: u32,
    pub format: ColorFormat,
    pub stride: u32,
    /// Main pixel stream; for RGB565A8 this is the RGB565 stream and the
    /// A8 plane lives in `extra`.
    pub data: &'a [u8],
    /// Inline indexed color table or RGB565A8 alpha plane.
    pub extra: Option<&'a [u8]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct ImageChunkInput<'a> {
    pub width: u32,
    pub height: u32,
    pub format: ColorFormat,
    pub stride: u32,
    pub main: &'a [u8],
    pub extra: Option<&'a [u8]>,
}

#[cfg(test)]
pub fn parse_chunk(buf: &[u8]) -> Result<ChunkFile<'_>, ParseError> {
    let file = FileHeader::parse(buf)?;
    if file.layout != Layout::Chunk {
        return Err(ParseError::UnknownLayout(file.layout.to_u8()));
    }
    if buf.len() < CHUNK_FILE_HEADER_LEN {
        return Err(ParseError::Truncated);
    }

    if buf[10] != 0 || buf[11] != 0 {
        return Err(ParseError::ReservedNonZero);
    }
    if buf[36..40].iter().any(|b| *b != 0) {
        return Err(ParseError::ReservedNonZero);
    }

    let chunk_count = u16::from_le_bytes([buf[8], buf[9]]);
    let chunk_table_offset = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let file_size = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let primary_chunk_type = u16::from_le_bytes([buf[20], buf[21]]);
    let primary_sample_layout = u16::from_le_bytes([buf[22], buf[23]]);
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
        primary_sample_layout,
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
            && entry.chunk_type != chunk_type::FONT
            && entry.chunk_type != chunk_type::META
            && entry.chunk_type != chunk_type::VECTOR
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

#[cfg(test)]
fn parse_image_chunk<'a>(buf: &'a [u8], entry: &ChunkEntry) -> Result<ImageChunk<'a>, ParseError> {
    let start = entry.chunk_offset as usize;
    let end = start
        .checked_add(entry.chunk_size as usize)
        .ok_or(ParseError::DimensionOverflow)?;
    let payload = buf.get(start..end).ok_or(ParseError::Truncated)?;
    let image = crate::ImageView::open_payload_at(payload, entry.chunk_offset)
        .map_err(ParseError::InvalidImage)?;
    Ok(ImageChunk {
        width: image.width(),
        height: image.height(),
        format: image.format(),
        stride: image.stride(),
        data: image.main(),
        extra: image.extra(),
    })
}

#[cfg(test)]
pub(crate) fn encode_chunk_image(image: &ImageChunkInput<'_>) -> Vec<u8> {
    use crate::image::ImageSource;
    use crate::wire::{write_u16_le, write_u32_le};
    let mut asset = crate::ImageAsset::new(
        image.width,
        image.height,
        image.format,
        image.stride,
        alloc::borrow::Cow::Borrowed(image.main),
    );
    if let Some(extra) = image.extra {
        asset = asset.with_extra(alloc::borrow::Cow::Borrowed(extra));
    }
    let surface = asset.view().expect("valid RAW IMAGE input");
    let payload_size = surface
        .encoded_len()
        .expect("IMAGE payload length fits wire fields");
    let chunk_start = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN;
    let file_size = chunk_start
        .checked_add(payload_size)
        .expect("IMAGE file size fits usize");
    let wire_file_size = u32::try_from(file_size).expect("IMAGE file size fits u32");
    let mut out = vec![0; file_size];
    let file_header = FileHeader {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        layout: Layout::Chunk,
        flags: 0,
    };
    let mut prefix = [0; FILE_HEADER_LEN];
    file_header.write_into(&mut prefix);
    out[..FILE_HEADER_LEN].copy_from_slice(&prefix);
    write_u16_le(&mut out, 8, 1);
    write_u32_le(&mut out, 12, CHUNK_FILE_HEADER_LEN as u32);
    write_u32_le(&mut out, 16, wire_file_size);
    write_u16_le(&mut out, 20, chunk_type::IMAGE);
    write_u16_le(&mut out, 22, surface.surface().sample_layout().raw());
    write_u32_le(&mut out, 24, image.width);
    write_u32_le(&mut out, 28, image.height);
    write_u32_le(&mut out, 32, image.stride);
    let crc = crc32::compute(&out[..40]);
    write_u32_le(&mut out, 40, crc);
    write_u16_le(&mut out, CHUNK_FILE_HEADER_LEN, chunk_type::IMAGE);
    write_u32_le(&mut out, CHUNK_FILE_HEADER_LEN + 4, chunk_start as u32);
    write_u32_le(&mut out, CHUNK_FILE_HEADER_LEN + 8, payload_size as u32);
    surface
        .encode_into(&mut out[chunk_start..])
        .expect("exact IMAGE output capacity");
    out
}

#[cfg(test)]
pub(crate) fn encode_chunk_generic(chunk_type: u16, flags: u16, payload: &[u8]) -> Vec<u8> {
    let chunk_table_offset = CHUNK_FILE_HEADER_LEN as u32;
    let chunk_start = chunk_table_offset as usize + CHUNK_TABLE_ENTRY_LEN;
    let chunk_size = payload.len();
    let file_size = chunk_start + chunk_size;

    let mut out = vec![0u8; file_size];

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
    out[16..20].copy_from_slice(&(file_size as u32).to_le_bytes());
    out[20..22].copy_from_slice(&chunk_type.to_le_bytes());
    let crc = crc32(&out[..40]);
    out[40..44].copy_from_slice(&crc.to_le_bytes());

    let entry_off = chunk_table_offset as usize;
    out[entry_off..entry_off + 2].copy_from_slice(&chunk_type.to_le_bytes());
    out[entry_off + 2..entry_off + 4].copy_from_slice(&flags.to_le_bytes());
    out[entry_off + 4..entry_off + 8].copy_from_slice(&(chunk_start as u32).to_le_bytes());
    out[entry_off + 8..entry_off + 12].copy_from_slice(&(chunk_size as u32).to_le_bytes());

    out[chunk_start..chunk_start + chunk_size].copy_from_slice(payload);

    out
}

/// Multi-chunk file: writes `chunks` (each `(chunk_type, flags,
/// payload)`) into one CHUNK-layout buffer, table then payloads. The
/// reader resolves them through [`crate::Reader::chunks`]. Primary header fields stay zeroed —
/// a multi-chunk file (e.g. several FONT representations) has no single
/// primary.
pub fn encode_chunks(chunks: &[(u16, u16, &[u8])]) -> Vec<u8> {
    let count = chunks.len();
    let chunk_table_offset = CHUNK_FILE_HEADER_LEN;
    let payloads_start = chunk_table_offset + count * CHUNK_TABLE_ENTRY_LEN;
    let total_payload: usize = chunks.iter().map(|(_, _, p)| p.len()).sum();
    let file_size = payloads_start + total_payload;

    let mut out = vec![0u8; file_size];

    let file_header = FileHeader {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        layout: Layout::Chunk,
        flags: 0,
    };
    let mut prefix = [0u8; FILE_HEADER_LEN];
    file_header.write_into(&mut prefix);
    out[0..FILE_HEADER_LEN].copy_from_slice(&prefix);

    out[8..10].copy_from_slice(&(count as u16).to_le_bytes());
    out[12..16].copy_from_slice(&(chunk_table_offset as u32).to_le_bytes());
    out[16..20].copy_from_slice(&(file_size as u32).to_le_bytes());
    // Primary chunk type points at the first chunk so a reader has a
    // hint, but no primary IMAGE decode happens for non-IMAGE types.
    if let Some((first_type, _, _)) = chunks.first() {
        out[20..22].copy_from_slice(&first_type.to_le_bytes());
    }
    let crc = crc32(&out[..40]);
    out[40..44].copy_from_slice(&crc.to_le_bytes());

    let mut payload_cursor = payloads_start;
    for (i, (chunk_type, flags, payload)) in chunks.iter().enumerate() {
        let entry_off = chunk_table_offset + i * CHUNK_TABLE_ENTRY_LEN;
        out[entry_off..entry_off + 2].copy_from_slice(&chunk_type.to_le_bytes());
        out[entry_off + 2..entry_off + 4].copy_from_slice(&flags.to_le_bytes());
        out[entry_off + 4..entry_off + 8].copy_from_slice(&(payload_cursor as u32).to_le_bytes());
        out[entry_off + 8..entry_off + 12].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        out[payload_cursor..payload_cursor + payload.len()].copy_from_slice(payload);
        payload_cursor += payload.len();
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
            parsed.header.primary_sample_layout,
            crate::image::SampleLayout::RGB565.raw()
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
        let payload_offset = parsed.entries[0].chunk_offset as usize;
        let payload = &encoded[payload_offset..];
        assert_eq!(
            offset,
            payload_offset + crate::image::test_support::data_offset(payload)
        );
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
    fn raw_chunk_access_rejects_coded_sections() {
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
        encoded[60 + crate::media::MEDIA_HEADER_LEN] =
            crate::media::MediaSectionKind::CODINGS.raw() as u8;
        crate::image::test_support::refresh_crc(&mut encoded[60..]);
        assert!(matches!(
            parse_chunk(&encoded),
            Err(ParseError::InvalidImage(crate::ImagePayloadError::Media(
                crate::image::RawImageViewError::UnexpectedSection(
                    crate::media::MediaSectionKind::CODINGS
                )
            )))
        ));
    }

    #[test]
    fn chunk_rejects_out_of_bounds_image_sections() {
        let mut encoded = encode_chunk_image(&ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::RGB565,
            stride: 4,
            main: &[0; 8],
            extra: None,
        });
        // DATA is the second directory entry. Its byte range must stay inside
        // the payload even when an attacker recomputes the checksum.
        let data_size = 60 + crate::media::MEDIA_HEADER_LEN + crate::media::MEDIA_SECTION_LEN + 8;
        encoded[data_size..data_size + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        crate::image::test_support::refresh_crc(&mut encoded[60..]);
        assert!(matches!(
            parse_chunk(&encoded),
            Err(ParseError::InvalidImage(crate::ImagePayloadError::Media(
                crate::image::RawImageViewError::Media(_)
            )))
        ));
    }

    #[test]
    fn generic_chunk_round_trips_font_payload() {
        let payload: Vec<u8> = (0u8..32).collect();
        let encoded = encode_chunk_generic(chunk_type::FONT, ChunkEntry::FLAG_CRITICAL, &payload);
        let parsed = parse_chunk(&encoded).unwrap();
        assert_eq!(parsed.entries.len(), 1);
        let entry = parsed.entries[0];
        assert_eq!(entry.chunk_type, chunk_type::FONT);
        assert!(entry.is_critical());
        assert!(parsed.primary_image.is_none());
        let slice = parsed.chunk_payload(&encoded, chunk_type::FONT).unwrap();
        assert_eq!(slice, payload.as_slice());
    }

    #[test]
    fn encode_chunks_round_trips_multiple_font_chunks() {
        let a: Vec<u8> = (0u8..16).collect();
        let b: Vec<u8> = (100u8..140).collect();
        let c: Vec<u8> = vec![0xEE; 8];
        let encoded = encode_chunks(&[
            (chunk_type::FONT, ChunkEntry::FLAG_CRITICAL, &a),
            (chunk_type::FONT, ChunkEntry::FLAG_CRITICAL, &b),
            (chunk_type::META, 0, &c),
        ]);
        let parsed = parse_chunk(&encoded).unwrap();
        assert_eq!(parsed.entries.len(), 3);

        // Both FONT faces come back in table order.
        let fonts: Vec<&[u8]> = parsed.chunk_payloads(&encoded, chunk_type::FONT).collect();
        assert_eq!(fonts.len(), 2);
        assert_eq!(fonts[0], a.as_slice());
        assert_eq!(fonts[1], b.as_slice());

        // chunk_payload still returns the first match.
        assert_eq!(
            parsed.chunk_payload(&encoded, chunk_type::FONT).unwrap(),
            a.as_slice()
        );
        assert_eq!(
            parsed.chunk_payload(&encoded, chunk_type::META).unwrap(),
            c.as_slice()
        );
    }

    #[test]
    fn encode_chunks_empty_is_valid_header() {
        let encoded = encode_chunks(&[]);
        let parsed = parse_chunk(&encoded).unwrap();
        assert_eq!(parsed.entries.len(), 0);
    }

    #[test]
    fn critical_vector_chunk_is_accepted() {
        let payload: Vec<u8> = (0u8..24).collect();
        let encoded = encode_chunk_generic(chunk_type::VECTOR, ChunkEntry::FLAG_CRITICAL, &payload);
        let parsed = parse_chunk(&encoded).unwrap();
        let entry = parsed.entries[0];
        assert_eq!(entry.chunk_type, chunk_type::VECTOR);
        assert!(entry.is_critical());
        let slice = parsed.chunk_payload(&encoded, chunk_type::VECTOR).unwrap();
        assert_eq!(slice, payload.as_slice());
    }
}
