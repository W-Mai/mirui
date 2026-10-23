use crate::image::{ImageReadError, ImageRef};
use core::iter::FusedIterator;

use crate::frames::{FramesError, FramesView};
use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkFileHeader};
use crate::meta::{MetaDecodeError, MetaView};
use crate::palette::{PaletteDecodeError, PaletteView};
use crate::reader::ReadError;
use crate::scene::{Scene, VectorReadError};
use crate::wire::{read_u16_le, read_u32_le, slice};
use crate::{ChunkFlags, ChunkType, PayloadLimits};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EntryRecord {
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload_offset: u32,
    payload_size: u32,
}

fn decode_record(record: &[u8]) -> Option<EntryRecord> {
    Some(EntryRecord {
        chunk_type: ChunkType::new(read_u16_le(record, 0)?)?,
        flags: ChunkFlags::from_bits_retain(read_u16_le(record, 2)?),
        payload_offset: read_u32_le(record, 4)?,
        payload_size: read_u32_le(record, 8)?,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ChunkTableMeta {
    offset: usize,
    count: usize,
}

impl ChunkTableMeta {
    pub(crate) fn inspect(
        bytes: &[u8],
        header: ChunkFileHeader,
        max_chunks: u16,
    ) -> Result<Self, ReadError> {
        let table = Self::from_header(header, max_chunks)?;
        for index in 0..table.count {
            let record_offset = table.record_offset(index)?;
            let record =
                slice(bytes, record_offset, CHUNK_TABLE_ENTRY_LEN).ok_or(ReadError::Truncated {
                    needed: record_offset.saturating_add(CHUNK_TABLE_ENTRY_LEN),
                    available: bytes.len(),
                })?;
            table.validate_record(record, index, header.file_size)?;
        }
        Ok(table)
    }

    fn from_header(header: ChunkFileHeader, max_chunks: u16) -> Result<Self, ReadError> {
        if header.chunk_count > max_chunks {
            return Err(ReadError::TooManyChunks {
                count: header.chunk_count,
                limit: max_chunks,
            });
        }
        if header.chunk_table_offset < CHUNK_FILE_HEADER_LEN as u32 {
            return Err(ReadError::ChunkTableBeforeHeader {
                offset: header.chunk_table_offset,
            });
        }
        let offset =
            usize::try_from(header.chunk_table_offset).map_err(|_| ReadError::SizeOverflow)?;
        let count = usize::from(header.chunk_count);
        let table_len = count
            .checked_mul(CHUNK_TABLE_ENTRY_LEN)
            .ok_or(ReadError::SizeOverflow)?;
        let table_end = offset
            .checked_add(table_len)
            .ok_or(ReadError::SizeOverflow)?;
        if usize::try_from(header.file_size).map_err(|_| ReadError::SizeOverflow)? < table_end {
            return Err(ReadError::ChunkTableOutOfBounds {
                offset: header.chunk_table_offset,
                count: header.chunk_count,
                file_size: header.file_size,
            });
        }

        Ok(Self { offset, count })
    }

    fn record_offset(self, index: usize) -> Result<usize, ReadError> {
        if index >= self.count {
            return Err(ReadError::SizeOverflow);
        }
        self.offset
            .checked_add(
                index
                    .checked_mul(CHUNK_TABLE_ENTRY_LEN)
                    .ok_or(ReadError::SizeOverflow)?,
            )
            .ok_or(ReadError::SizeOverflow)
    }

    // `record` is one read window; payload bounds belong to the logical file.
    fn validate_record(
        self,
        record: &[u8],
        index: usize,
        file_size: u32,
    ) -> Result<EntryRecord, ReadError> {
        let record_offset = self.record_offset(index)?;
        if record.len() < CHUNK_TABLE_ENTRY_LEN {
            return Err(ReadError::Truncated {
                needed: record_offset.saturating_add(CHUNK_TABLE_ENTRY_LEN),
                available: record_offset.saturating_add(record.len()),
            });
        }
        if let Some(relative) = record[12..16].iter().position(|&byte| byte != 0) {
            return Err(ReadError::ReservedNonZero {
                offset: record_offset + 12 + relative,
            });
        }
        let decoded = decode_record(record).ok_or(ReadError::InvalidChunkType)?;
        let out_of_bounds = ReadError::ChunkPayloadOutOfBounds {
            index: index as u16,
            offset: decoded.payload_offset,
            size: decoded.payload_size,
        };
        let payload_end = decoded
            .payload_offset
            .checked_add(decoded.payload_size)
            .ok_or_else(|| out_of_bounds.clone())?;
        if payload_end > file_size {
            return Err(out_of_bounds);
        }
        let payload_start =
            usize::try_from(decoded.payload_offset).map_err(|_| ReadError::SizeOverflow)?;
        let payload_end = usize::try_from(payload_end).map_err(|_| ReadError::SizeOverflow)?;
        if overlaps_or_points_into(payload_start, payload_end, 0, CHUNK_FILE_HEADER_LEN) {
            return Err(ReadError::ChunkPayloadOverlapsHeader {
                index: index as u16,
                offset: decoded.payload_offset,
                size: decoded.payload_size,
            });
        }
        let table_end = self.offset + self.count * CHUNK_TABLE_ENTRY_LEN;
        if overlaps_or_points_into(payload_start, payload_end, self.offset, table_end) {
            return Err(ReadError::ChunkPayloadOverlapsTable {
                index: index as u16,
                offset: decoded.payload_offset,
                size: decoded.payload_size,
            });
        }
        Ok(decoded)
    }
}

fn overlaps_or_points_into(
    range_start: usize,
    range_end: usize,
    structure_start: usize,
    structure_end: usize,
) -> bool {
    if structure_start == structure_end {
        return false;
    }
    if range_start == range_end {
        return structure_start <= range_start && range_start < structure_end;
    }
    range_start < structure_end && structure_start < range_end
}

/// One source-bound MIRX chunk table record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkRef<'a> {
    index: usize,
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload_offset: u32,
    payload: &'a [u8],
}

impl<'a> ChunkRef<'a> {
    pub const fn index(&self) -> usize {
        self.index
    }

    pub const fn chunk_type(&self) -> ChunkType {
        self.chunk_type
    }

    pub const fn flags(&self) -> ChunkFlags {
        self.flags
    }

    pub const fn payload_offset(&self) -> u32 {
        self.payload_offset
    }

    pub const fn payload(&self) -> &'a [u8] {
        self.payload
    }

    /// Returns a borrowed IMAGE view when this record has the IMAGE type.
    ///
    /// RAW exposes verified samples at their file position. Encoded metadata
    /// retains that position for explicit group, integrity and decode checks.
    /// Other types return `Ok(None)` without interpreting their payload bytes.
    pub fn image(&self) -> Result<Option<ImageRef<'a>>, ImageReadError> {
        if self.chunk_type != ChunkType::IMAGE {
            return Ok(None);
        }
        ImageRef::open_at(self.payload, self.payload_offset).map(Some)
    }

    /// Borrows one FONT face and retains its file placement without scanning DATA.
    /// Other chunk types return `Ok(None)` without interpreting their payload.
    pub fn font(
        &self,
        limits: &PayloadLimits,
    ) -> Result<Option<crate::font::FontView<'a>>, crate::font::FontError> {
        if self.chunk_type != ChunkType::FONT {
            return Ok(None);
        }
        crate::font::FontView::open_at(self.payload, self.payload_offset, limits).map(Some)
    }

    /// Returns a borrowed META view when this record has the META type.
    ///
    /// Other chunk types return `Ok(None)` without interpreting their payload
    /// bytes.
    pub fn meta(&self, limits: &PayloadLimits) -> Result<Option<MetaView<'a>>, MetaDecodeError> {
        if self.chunk_type != ChunkType::META {
            return Ok(None);
        }
        MetaView::open_payload(self.payload, limits).map(Some)
    }

    /// Returns a borrowed PALETTE view when this record has the PALETTE type.
    ///
    /// Other chunk types return `Ok(None)` without interpreting their payload
    /// bytes.
    pub fn palette(
        &self,
        limits: &PayloadLimits,
    ) -> Result<Option<PaletteView<'a>>, PaletteDecodeError> {
        if self.chunk_type != ChunkType::PALETTE {
            return Ok(None);
        }
        PaletteView::open_payload(self.payload, limits).map(Some)
    }

    /// Returns a borrowed FRAMES view when this record has the FRAMES type.
    ///
    /// Other chunk types return `Ok(None)` without interpreting their payload
    /// bytes.
    pub fn frames(&self, limits: &PayloadLimits) -> Result<Option<FramesView<'a>>, FramesError> {
        if self.chunk_type != ChunkType::FRAMES {
            return Ok(None);
        }
        FramesView::open_at(self.payload, self.payload_offset, limits).map(Some)
    }

    /// Decodes an owned VECTOR scene when this record has the VECTOR type.
    pub fn decode_vector(&self, limits: &PayloadLimits) -> Result<Option<Scene>, VectorReadError> {
        if self.chunk_type != ChunkType::VECTOR {
            return Ok(None);
        }
        Scene::decode_with_limits(self.payload, limits).map(Some)
    }
}

/// Lazy, zero-allocation iterator over a validated CHUNK table.
#[derive(Clone, Debug)]
pub struct EntryIter<'a> {
    bytes: &'a [u8],
    table_offset: usize,
    front: usize,
    back: usize,
}

impl<'a> EntryIter<'a> {
    pub(crate) const fn new(bytes: &'a [u8], table: ChunkTableMeta) -> Self {
        Self {
            bytes,
            table_offset: table.offset,
            front: 0,
            back: table.count,
        }
    }

    pub(crate) const fn empty(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            table_offset: 0,
            front: 0,
            back: 0,
        }
    }

    fn decode(&self, index: usize) -> ChunkRef<'a> {
        let record_offset = self
            .table_offset
            .checked_add(
                index
                    .checked_mul(CHUNK_TABLE_ENTRY_LEN)
                    .expect("validated CHUNK table index"),
            )
            .expect("validated CHUNK table offset");
        let record = slice(self.bytes, record_offset, CHUNK_TABLE_ENTRY_LEN)
            .expect("Reader::open validated every CHUNK record");
        let decoded =
            decode_record(record).expect("Reader::open validated every CHUNK record field");
        let payload = slice(
            self.bytes,
            usize::try_from(decoded.payload_offset).expect("validated CHUNK payload offset"),
            usize::try_from(decoded.payload_size).expect("validated CHUNK payload size"),
        )
        .expect("Reader::open validated every CHUNK payload range");
        ChunkRef {
            index,
            chunk_type: decoded.chunk_type,
            flags: decoded.flags,
            payload_offset: decoded.payload_offset,
            payload,
        }
    }
}

impl<'a> Iterator for EntryIter<'a> {
    type Item = ChunkRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        Some(self.decode(index))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.back - self.front;
        (remaining, Some(remaining))
    }
}

impl DoubleEndedIterator for EntryIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.decode(self.back))
    }
}

impl ExactSizeIterator for EntryIter<'_> {}

#[cfg(test)]
mod range_tests {
    use super::*;
    use crate::{Reader, encode_chunks};

    #[test]
    fn isolated_records_and_complete_sources_enforce_identical_ranges() {
        let original = encode_chunks(&[(crate::header::chunk_type::META, 0, b"payload")]);
        let file = crate::reader::parse_file_header(&original).unwrap();
        let header = crate::reader::parse_chunk_header(&original, file).unwrap();
        let table = ChunkTableMeta::from_header(header, 1).unwrap();
        let start = table.record_offset(0).unwrap();
        for offset in [0, 43, 44, 59, 60, 61, header.file_size, u32::MAX] {
            for size in [0, 1, 7, u32::MAX] {
                let mut bytes = original.clone();
                bytes[start + 4..start + 8].copy_from_slice(&offset.to_le_bytes());
                bytes[start + 8..start + 12].copy_from_slice(&size.to_le_bytes());
                let bounded = table.validate_record(&bytes[start..start + 16], 0, header.file_size);
                assert_eq!(bounded.err(), Reader::open(&bytes).err());
            }
        }
        let mut record = [0; CHUNK_TABLE_ENTRY_LEN];
        record.copy_from_slice(&original[start..start + CHUNK_TABLE_ENTRY_LEN]);
        record[15] = 1;
        assert_eq!(
            table.validate_record(&record, 0, header.file_size),
            Err(ReadError::ReservedNonZero { offset: start + 15 })
        );
        assert_eq!(
            table.validate_record(&record[..15], 0, header.file_size),
            Err(ReadError::Truncated {
                needed: start + 16,
                available: start + 15,
            })
        );
    }

    #[test]
    fn isolated_table_validation_does_not_require_payload_storage() {
        let bytes = encode_chunks(&[(crate::header::chunk_type::META, 0, b"")]);
        let file = crate::reader::parse_file_header(&bytes).unwrap();
        let mut header = crate::reader::parse_chunk_header(&bytes, file).unwrap();
        header.file_size = u32::MAX;
        let table = ChunkTableMeta::from_header(header, 1).unwrap();
        let mut record = [0; CHUNK_TABLE_ENTRY_LEN];
        record[..2].copy_from_slice(&crate::header::chunk_type::META.to_le_bytes());
        record[4..8].copy_from_slice(&(u32::MAX - 8).to_le_bytes());
        record[8..12].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(
            table
                .validate_record(&record, 0, header.file_size)
                .unwrap()
                .payload_size,
            8
        );
        assert!(table.record_offset(1).is_err());
        assert!(table.record_offset(usize::MAX).is_err());
        assert_eq!(
            core::mem::size_of::<ChunkTableMeta>(),
            2 * core::mem::size_of::<usize>()
        );
    }
}
impl FusedIterator for EntryIter<'_> {}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, chunk_type};
    use crate::{ReadOptions, Reader, crc32, encode_chunks};

    #[test]
    fn iterates_source_bound_records_in_both_directions() {
        let bytes = encode_chunks(&[(0xbeef, 0xa500, b"abc"), (chunk_type::IMAGE, 0, b"xy")]);
        let reader = Reader::open(&bytes).unwrap();
        let mut entries = reader.chunks();
        assert_eq!(entries.len(), 2);

        let first = entries.next().unwrap();
        assert_eq!(first.index(), 0);
        assert_eq!(first.chunk_type().raw(), 0xbeef);
        assert_eq!(first.flags().bits(), 0xa500);
        assert_eq!(first.payload(), b"abc");
        assert_eq!(
            first.payload().as_ptr(),
            bytes[first.payload_offset() as usize..].as_ptr()
        );

        let mut clone = entries.clone();
        assert_eq!(clone.next().unwrap().index(), 1);
        assert_eq!(clone.next(), None);
        assert_eq!(entries.len(), 1);

        let last = entries.next_back().unwrap();
        assert_eq!(last.index(), 1);
        assert_eq!(last.chunk_type(), ChunkType::IMAGE);
        assert_eq!(last.payload(), b"xy");
        assert_eq!(entries.len(), 0);
        assert_eq!(entries.next(), None);
        assert_eq!(entries.next_back(), None);
    }

    #[test]
    fn rejects_table_bounds_type_zero_and_reserved_bytes_during_open() {
        let valid = encode_chunks(&[(chunk_type::META, 0, b"a"), (chunk_type::FONT, 0, b"b")]);
        let table_end = CHUNK_FILE_HEADER_LEN + 2 * CHUNK_TABLE_ENTRY_LEN;
        let mut truncated_table = valid[..table_end - 1].to_vec();
        let truncated_len = truncated_table.len() as u32;
        truncated_table[16..20].copy_from_slice(&truncated_len.to_le_bytes());
        let checksum = crc32(&truncated_table[..40]);
        truncated_table[40..44].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reader::open(&truncated_table),
            Err(ReadError::ChunkTableOutOfBounds {
                offset: CHUNK_FILE_HEADER_LEN as u32,
                count: 2,
                file_size: table_end as u32 - 1,
            })
        );

        let mut before_header = encode_chunks(&[]);
        before_header[12..16].copy_from_slice(&8u32.to_le_bytes());
        let checksum = crc32(&before_header[..40]);
        before_header[40..44].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reader::open(&before_header),
            Err(ReadError::ChunkTableBeforeHeader { offset: 8 })
        );

        let mut type_zero = valid.clone();
        type_zero[CHUNK_FILE_HEADER_LEN..CHUNK_FILE_HEADER_LEN + 2]
            .copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(Reader::open(&type_zero), Err(ReadError::InvalidChunkType));

        for relative in 12..16 {
            let mut reserved = valid.clone();
            let bad_offset = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN + relative;
            reserved[bad_offset] = 1;
            assert_eq!(
                Reader::open(&reserved),
                Err(ReadError::ReservedNonZero { offset: bad_offset })
            );
        }
    }

    #[test]
    fn rejects_every_bad_payload_before_iteration() {
        let mut bytes =
            encode_chunks(&[(chunk_type::META, 0, b"ok"), (chunk_type::FONT, 0, b"bad")]);
        let second = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN;
        let offset = bytes.len() as u32 - 1;
        let size = 2u32;
        bytes[second + 4..second + 8].copy_from_slice(&offset.to_le_bytes());
        bytes[second + 8..second + 12].copy_from_slice(&size.to_le_bytes());
        assert_eq!(
            Reader::open(&bytes),
            Err(ReadError::ChunkPayloadOutOfBounds {
                index: 1,
                offset,
                size,
            })
        );

        let mut overflow = encode_chunks(&[(chunk_type::META, 0, b"")]);
        overflow[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
            .copy_from_slice(&u32::MAX.to_le_bytes());
        overflow[CHUNK_FILE_HEADER_LEN + 8..CHUNK_FILE_HEADER_LEN + 12]
            .copy_from_slice(&1u32.to_le_bytes());
        assert!(matches!(
            Reader::open(&overflow),
            Err(ReadError::ChunkPayloadOutOfBounds { index: 0, .. })
        ));

        let mut bad_start = encode_chunks(&[(chunk_type::META, 0, b"")]);
        let offset = bad_start.len() as u32 + 1;
        bad_start[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
            .copy_from_slice(&offset.to_le_bytes());
        assert_eq!(
            Reader::open(&bad_start),
            Err(ReadError::ChunkPayloadOutOfBounds {
                index: 0,
                offset,
                size: 0,
            })
        );
    }

    #[test]
    fn honors_chunk_limits() {
        let bytes = encode_chunks(&[(chunk_type::META, 0, b"a"), (chunk_type::FONT, 0, b"b")]);
        let options = ReadOptions::new().with_max_chunks(1);
        assert_eq!(
            Reader::open_with(&bytes, &options),
            Err(ReadError::TooManyChunks { count: 2, limit: 1 })
        );

        let over_default = usize::from(ReadOptions::DEFAULT_MAX_CHUNKS) + 1;
        let chunks = vec![(chunk_type::META, 0, b"" as &[u8]); over_default];
        let bytes = encode_chunks(&chunks);
        assert_eq!(
            Reader::open(&bytes),
            Err(ReadError::TooManyChunks {
                count: over_default as u16,
                limit: ReadOptions::DEFAULT_MAX_CHUNKS,
            })
        );
        let options = ReadOptions::new().with_max_chunks(over_default as u16);
        assert_eq!(options.max_chunks(), over_default as u16);
        assert_eq!(
            Reader::open_with(&bytes, &options).unwrap().chunks().len(),
            over_default
        );
    }

    #[test]
    fn empty_payloads_and_duplicate_types_are_preserved() {
        let bytes = encode_chunks(&[(chunk_type::META, 0, b""), (chunk_type::META, 0x8000, b"x")]);
        let reader = Reader::open(&bytes).unwrap();
        let mut entries = reader.chunks();
        assert_eq!(entries.next().unwrap().payload(), b"");
        assert_eq!(entries.next().unwrap().flags().bits(), 0x8000);
    }

    #[test]
    fn accepts_primary_type_zero_and_payloads_ending_at_eof() {
        let mut bytes =
            encode_chunks(&[(chunk_type::META, 0, b"tail"), (chunk_type::FONT, 0, b"")]);
        bytes[20..22].copy_from_slice(&0u16.to_le_bytes());
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());

        let reader = Reader::open(&bytes).unwrap();
        let entries = reader.chunks().collect::<alloc::vec::Vec<_>>();
        assert_eq!(entries[0].payload(), b"tail");
        assert_eq!(entries[1].payload(), b"");
        assert_eq!(entries[1].payload_offset() as usize, bytes.len());
    }

    #[test]
    fn flat_sources_have_an_empty_chunk_iterator() {
        let mut bytes = vec![0; crate::FLAT_HEADER_LEN + 1];
        bytes[..4].copy_from_slice(b"MIRX");
        bytes[4] = crate::VERSION_MAJOR;
        bytes[5] = crate::VERSION_MINOR;
        bytes[6] = crate::Layout::Flat.to_u8();
        bytes[8] = crate::ColorFormat::A8.to_u8();
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.chunks().len(), 0);
    }
}
