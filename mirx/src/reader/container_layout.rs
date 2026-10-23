use core::ops::Range;

use super::entry::{ChunkTableMeta, EntryRecord};
use super::{ContainerHeader, FlatImageLayout, ReadError, ReadOptions};
use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, FLAT_HEADER_LEN, Layout};
use crate::{ChunkFlags, ChunkType};

/// Container geometry parsed from its header, independent of source storage.
///
/// This is a range-reading primitive, not a replacement for [`super::Reader::open`]. It validates the header and directory extent, not the existence of source bytes, unread records or payload semantics. Consumers must check those before publishing a resource. No input slice is retained and parsing does not allocate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerLayout {
    pub(super) header: ContainerHeader,
    logical_len: usize,
    pub(super) table: Option<ChunkTableMeta>,
}

impl ContainerLayout {
    /// Determine the complete header length from the common eight-byte prefix.
    pub fn header_len(prefix: &[u8]) -> Result<usize, ReadError> {
        Ok(match super::parse_file_header(prefix)?.layout {
            Layout::Flat => FLAT_HEADER_LEN,
            Layout::Chunk => CHUNK_FILE_HEADER_LEN,
        })
    }

    /// Parse a complete header window. Trailing-byte policy applies when the complete source is checked, not to this window.
    pub fn parse(header: &[u8], options: &ReadOptions) -> Result<Self, ReadError> {
        Self::inspect(header, None, options).map(|(layout, _)| layout)
    }

    pub(super) fn inspect(
        bytes: &[u8],
        available: Option<usize>,
        options: &ReadOptions,
    ) -> Result<(Self, Option<FlatImageLayout>), ReadError> {
        let file = super::parse_file_header(bytes)?;
        let (header, logical_len, flat) = match file.layout {
            Layout::Flat => {
                let header = super::parse_flat_header(bytes, file)?;
                let flat = FlatImageLayout::from_header(header)?;
                (
                    ContainerHeader::Flat(header),
                    flat.logical_len(),
                    Some(flat),
                )
            }
            Layout::Chunk => {
                let header = super::parse_chunk_header(bytes, file)?;
                let len = super::chunk_logical_len(header, available.unwrap_or(usize::MAX))?;
                (ContainerHeader::Chunk(header), len, None)
            }
        };
        if let Some(available) = available {
            if available < logical_len {
                return Err(ReadError::Truncated {
                    needed: logical_len,
                    available,
                });
            }
        }
        let table = match header {
            ContainerHeader::Flat(_) => None,
            ContainerHeader::Chunk(header) => {
                Some(ChunkTableMeta::from_header(header, options.max_chunks())?)
            }
        };
        Ok((
            Self {
                header,
                logical_len,
                table,
            },
            flat,
        ))
    }

    pub const fn layout(self) -> Layout {
        self.header.file().layout
    }
    /// The declared primary type. Resolve its first matching directory entry; this hint does not validate a payload or guarantee that an entry exists.
    pub fn primary_type(self) -> Option<ChunkType> {
        self.header.primary_type()
    }
    pub const fn logical_len(self) -> usize {
        self.logical_len
    }
    pub const fn chunk_count(self) -> usize {
        match self.header {
            ContainerHeader::Flat(_) => 0,
            ContainerHeader::Chunk(header) => header.chunk_count as usize,
        }
    }

    pub fn flat_image(self) -> Option<FlatImageLayout> {
        match self.header {
            ContainerHeader::Flat(header) => {
                Some(FlatImageLayout::from_header(header).expect("validated FLAT geometry"))
            }
            ContainerHeader::Chunk(_) => None,
        }
    }

    /// Return the file-relative record window, or `None` when the index is absent.
    pub fn chunk_record_range(self, index: usize) -> Option<Range<usize>> {
        let start = self.table?.record_offset(index).ok()?;
        Some(start..start + CHUNK_TABLE_ENTRY_LEN)
    }

    /// Validate one isolated record against the logical file, without reading its payload. The record slice starts at the range returned by [`Self::chunk_record_range`].
    pub fn chunk(self, index: usize, record: &[u8]) -> Result<Option<ChunkLocation>, ReadError> {
        let (Some(table), ContainerHeader::Chunk(header)) = (self.table, self.header) else {
            return Ok(None);
        };
        if index >= self.chunk_count() {
            return Ok(None);
        }
        let record = table.validate_record(record, index, header.file_size)?;
        Ok(Some(ChunkLocation { index, record }))
    }
}

/// A validated directory record, without a borrowed or decoded payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkLocation {
    index: usize,
    record: EntryRecord,
}

impl ChunkLocation {
    pub const fn index(self) -> usize {
        self.index
    }
    pub const fn chunk_type(self) -> ChunkType {
        self.record.chunk_type
    }
    pub const fn flags(self) -> ChunkFlags {
        self.record.flags
    }
    pub fn payload_range(self) -> Range<usize> {
        let start = self.record.payload_offset as usize;
        start..start + self.record.payload_size as usize
    }
}
