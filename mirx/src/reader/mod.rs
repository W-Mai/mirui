mod entry;
mod finding;
#[cfg(test)]
mod length;
mod limits;
mod options;
mod preflight;
mod primary;
#[cfg(test)]
mod ranges;

pub use entry::{ChunkRef, EntryIter};
pub use finding::{ComplianceFinding, FindingIter};
pub use limits::PayloadLimits;
pub use options::{ReadOptions, TrailingBytesPolicy};
pub use preflight::{PayloadLocation, PayloadValidationError, PayloadValidationFailure};

use crate::ImageView;
use crate::ReadError;
use crate::crc32;
use crate::header::{
    CHUNK_FILE_HEADER_LEN, ChunkFileHeader, FILE_HEADER_LEN, FLAT_HEADER_LEN, FileHeader,
    FlatHeader, Layout, MAGIC, VERSION_MAJOR, VERSION_MINOR,
};
use crate::wire::{read_u16_le, read_u32_le, slice};
use entry::ChunkTableMeta;

/// Parsed MIRX container header without payload allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerHeader {
    Flat(FlatHeader),
    Chunk(ChunkFileHeader),
}

impl ContainerHeader {
    pub const fn file(self) -> FileHeader {
        match self {
            Self::Flat(header) => header.file,
            Self::Chunk(header) => header.file,
        }
    }
}

/// Zero-allocation structural view over one MIRX source buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    logical_len: usize,
    header: ContainerHeader,
    flat_image: Option<ImageView<'a>>,
    chunk_table: Option<ChunkTableMeta>,
    has_future_semantics: bool,
}

impl<'a> Reader<'a> {
    /// Validates the container structure and critical payload semantics without
    /// allocating.
    ///
    /// Higher minor versions and nonzero file flags are retained and marked as
    /// future semantics. Their layout-specific reserved bytes are not interpreted.
    pub fn open(bytes: &'a [u8]) -> Result<Self, ReadError> {
        Self::open_with(bytes, &ReadOptions::default())
    }

    pub fn open_with(bytes: &'a [u8], options: &ReadOptions) -> Result<Self, ReadError> {
        let file = parse_file_header(bytes)?;
        let has_future_semantics = file.version_minor > VERSION_MINOR || file.flags != 0;
        let (header, flat_image, chunk_table, logical_len) = match file.layout {
            Layout::Flat => {
                let header = parse_flat_header(bytes, file, has_future_semantics)?;
                if has_future_semantics {
                    (ContainerHeader::Flat(header), None, None, bytes.len())
                } else {
                    let (image, logical_len) = ImageView::from_flat(bytes, header)?;
                    (
                        ContainerHeader::Flat(header),
                        Some(image),
                        None,
                        logical_len,
                    )
                }
            }
            Layout::Chunk => {
                let header = parse_chunk_header(bytes, file, has_future_semantics)?;
                let logical_len = chunk_logical_len(header, bytes.len())?;
                let logical_bytes = &bytes[..logical_len];
                let table = ChunkTableMeta::inspect(
                    logical_bytes,
                    header,
                    !has_future_semantics,
                    options.max_chunks(),
                )?;
                (
                    ContainerHeader::Chunk(header),
                    None,
                    Some(table),
                    logical_len,
                )
            }
        };

        if logical_len < bytes.len()
            && options.trailing_bytes_policy() == TrailingBytesPolicy::Reject
        {
            return Err(ReadError::TrailingBytes {
                logical_len,
                actual_len: bytes.len(),
            });
        }

        let reader = Self {
            bytes,
            logical_len,
            header,
            flat_image,
            chunk_table,
            has_future_semantics,
        };
        reader.validate_critical_payloads(&options.payload_limits())?;
        Ok(reader)
    }

    pub const fn header(&self) -> ContainerHeader {
        self.header
    }

    pub const fn file_header(&self) -> FileHeader {
        self.header.file()
    }

    pub const fn layout(&self) -> Layout {
        self.file_header().layout
    }

    pub const fn has_future_semantics(&self) -> bool {
        self.has_future_semantics
    }

    pub const fn source(&self) -> &'a [u8] {
        self.bytes
    }

    pub const fn logical_len(&self) -> usize {
        self.logical_len
    }

    pub fn logical_source(&self) -> &'a [u8] {
        &self.bytes[..self.logical_len]
    }

    pub fn trailing_bytes(&self) -> &'a [u8] {
        &self.bytes[self.logical_len..]
    }

    pub const fn has_trailing_bytes(&self) -> bool {
        self.logical_len < self.bytes.len()
    }

    /// Returns the validated FLAT image for current container semantics.
    ///
    /// Future headers remain source-preservable but are not interpreted as a
    /// current image payload.
    pub const fn flat_image(&self) -> Option<ImageView<'a>> {
        self.flat_image
    }

    pub fn chunks(&self) -> EntryIter<'a> {
        let bytes = self.logical_source();
        match self.chunk_table {
            Some(table) => EntryIter::new(bytes, table),
            None => EntryIter::empty(bytes),
        }
    }
}

fn chunk_logical_len(header: ChunkFileHeader, available: usize) -> Result<usize, ReadError> {
    if header.file_size < CHUNK_FILE_HEADER_LEN as u32 {
        return Err(ReadError::InvalidFileSize {
            declared: header.file_size,
            minimum: CHUNK_FILE_HEADER_LEN as u32,
        });
    }
    let logical_len = usize::try_from(header.file_size).map_err(|_| ReadError::SizeOverflow)?;
    if available < logical_len {
        return Err(ReadError::Truncated {
            needed: logical_len,
            available,
        });
    }
    Ok(logical_len)
}

fn parse_file_header(bytes: &[u8]) -> Result<FileHeader, ReadError> {
    require_len(bytes, FILE_HEADER_LEN)?;
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(ReadError::BadMagic);
    }

    let version_major = bytes[4];
    let version_minor = bytes[5];
    if version_major != VERSION_MAJOR {
        return Err(ReadError::UnsupportedVersion {
            major: version_major,
            minor: version_minor,
        });
    }

    let layout = Layout::from_u8(bytes[6]).ok_or(ReadError::UnknownLayout(bytes[6]))?;
    Ok(FileHeader {
        version_major,
        version_minor,
        layout,
        flags: bytes[7],
    })
}

fn parse_flat_header(
    bytes: &[u8],
    file: FileHeader,
    has_future_semantics: bool,
) -> Result<FlatHeader, ReadError> {
    require_len(bytes, FLAT_HEADER_LEN)?;
    if !has_future_semantics {
        require_zero(bytes, &[9, 10, 11])?;
    }
    validate_crc(bytes, 24, 24)?;

    Ok(FlatHeader {
        file,
        color_format: bytes[8],
        width: read_u32(bytes, 12)?,
        height: read_u32(bytes, 16)?,
        stride: read_u32(bytes, 20)?,
        header_crc32: read_u32(bytes, 24)?,
    })
}

fn parse_chunk_header(
    bytes: &[u8],
    file: FileHeader,
    has_future_semantics: bool,
) -> Result<ChunkFileHeader, ReadError> {
    require_len(bytes, CHUNK_FILE_HEADER_LEN)?;
    if !has_future_semantics {
        require_zero(bytes, &[10, 11, 23, 36, 37, 38, 39])?;
    }
    validate_crc(bytes, 40, 40)?;

    Ok(ChunkFileHeader {
        file,
        chunk_count: read_u16(bytes, 8)?,
        chunk_table_offset: read_u32(bytes, 12)?,
        file_size: read_u32(bytes, 16)?,
        primary_chunk_type: read_u16(bytes, 20)?,
        primary_color_format: bytes[22],
        primary_width: read_u32(bytes, 24)?,
        primary_height: read_u32(bytes, 28)?,
        primary_stride: read_u32(bytes, 32)?,
        header_crc32: read_u32(bytes, 40)?,
    })
}

fn require_len(bytes: &[u8], needed: usize) -> Result<(), ReadError> {
    if bytes.len() < needed {
        return Err(ReadError::Truncated {
            needed,
            available: bytes.len(),
        });
    }
    Ok(())
}

fn require_zero(bytes: &[u8], offsets: &[usize]) -> Result<(), ReadError> {
    for &offset in offsets {
        if bytes[offset] != 0 {
            return Err(ReadError::ReservedNonZero { offset });
        }
    }
    Ok(())
}

fn validate_crc(bytes: &[u8], covered_len: usize, stored_offset: usize) -> Result<(), ReadError> {
    let covered = slice(bytes, 0, covered_len).ok_or(ReadError::Truncated {
        needed: covered_len,
        available: bytes.len(),
    })?;
    let expected = read_u32(bytes, stored_offset)?;
    let actual = crc32(covered);
    if expected != actual {
        return Err(ReadError::HeaderCrcMismatch { expected, actual });
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ReadError> {
    read_u16_le(bytes, offset).ok_or(ReadError::Truncated {
        needed: offset.saturating_add(2),
        available: bytes.len(),
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ReadError> {
    read_u32_le(bytes, offset).ok_or(ReadError::Truncated {
        needed: offset.saturating_add(4),
        available: bytes.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_header(minor: u8, flags: u8) -> [u8; FLAT_HEADER_LEN] {
        let mut bytes = [0; FLAT_HEADER_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION_MAJOR;
        bytes[5] = minor;
        bytes[6] = Layout::Flat.to_u8();
        bytes[7] = flags;
        bytes[8] = crate::ColorFormat::RGB565.to_u8();
        bytes[12..16].copy_from_slice(&2u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&4u32.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn flat_file(minor: u8, flags: u8) -> alloc::vec::Vec<u8> {
        let mut bytes = alloc::vec::Vec::from(flat_header(minor, flags));
        bytes.extend_from_slice(&[0; 4]);
        bytes
    }

    fn chunk_header(minor: u8, flags: u8) -> [u8; CHUNK_FILE_HEADER_LEN] {
        let mut bytes = [0; CHUNK_FILE_HEADER_LEN];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION_MAJOR;
        bytes[5] = minor;
        bytes[6] = Layout::Chunk.to_u8();
        bytes[7] = flags;
        bytes[8..10].copy_from_slice(&0u16.to_le_bytes());
        bytes[12..16].copy_from_slice(&44u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&44u32.to_le_bytes());
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn opens_flat_and_chunk_headers_without_allocation() {
        let flat = flat_file(VERSION_MINOR, 0);
        let reader = Reader::open(&flat).unwrap();
        assert_eq!(reader.layout(), Layout::Flat);
        assert_eq!(reader.source().as_ptr(), flat.as_ptr());
        assert!(!reader.has_future_semantics());
        assert!(matches!(reader.header(), ContainerHeader::Flat(_)));

        let chunk = chunk_header(VERSION_MINOR, 0);
        let reader = Reader::open(&chunk).unwrap();
        assert_eq!(reader.layout(), Layout::Chunk);
        assert!(matches!(reader.header(), ContainerHeader::Chunk(_)));
    }

    #[test]
    fn rejects_common_header_failures() {
        assert_eq!(
            Reader::open(&MAGIC),
            Err(ReadError::Truncated {
                needed: FILE_HEADER_LEN,
                available: MAGIC.len(),
            })
        );

        let mut bad_magic = flat_header(VERSION_MINOR, 0);
        bad_magic[0] = b'X';
        assert_eq!(Reader::open(&bad_magic), Err(ReadError::BadMagic));

        let mut bad_version = flat_header(VERSION_MINOR, 0);
        bad_version[4] = VERSION_MAJOR + 1;
        assert!(matches!(
            Reader::open(&bad_version),
            Err(ReadError::UnsupportedVersion { .. })
        ));

        let mut bad_layout = flat_header(VERSION_MINOR, 0);
        bad_layout[6] = 0xff;
        assert_eq!(
            Reader::open(&bad_layout),
            Err(ReadError::UnknownLayout(0xff))
        );
    }

    #[test]
    fn rejects_layout_header_truncation_and_crc_mismatch() {
        let flat = flat_header(VERSION_MINOR, 0);
        assert_eq!(
            Reader::open(&flat[..FLAT_HEADER_LEN - 1]),
            Err(ReadError::Truncated {
                needed: FLAT_HEADER_LEN,
                available: FLAT_HEADER_LEN - 1,
            })
        );

        let chunk = chunk_header(VERSION_MINOR, 0);
        assert_eq!(
            Reader::open(&chunk[..CHUNK_FILE_HEADER_LEN - 1]),
            Err(ReadError::Truncated {
                needed: CHUNK_FILE_HEADER_LEN,
                available: CHUNK_FILE_HEADER_LEN - 1,
            })
        );

        let mut bad_crc = flat;
        bad_crc[12] ^= 1;
        assert!(matches!(
            Reader::open(&bad_crc),
            Err(ReadError::HeaderCrcMismatch { .. })
        ));
    }

    #[test]
    fn current_reserved_bytes_are_zero_but_future_headers_are_preserved() {
        for offset in [9, 10, 11] {
            let mut current = flat_header(VERSION_MINOR, 0);
            current[offset] = 1;
            let checksum = crc32(&current[..24]);
            current[24..28].copy_from_slice(&checksum.to_le_bytes());
            assert_eq!(
                Reader::open(&current),
                Err(ReadError::ReservedNonZero { offset })
            );
        }

        for offset in [10, 11, 23, 36, 37, 38, 39] {
            let mut current = chunk_header(VERSION_MINOR, 0);
            current[offset] = 1;
            let checksum = crc32(&current[..40]);
            current[40..44].copy_from_slice(&checksum.to_le_bytes());
            assert_eq!(
                Reader::open(&current),
                Err(ReadError::ReservedNonZero { offset })
            );
        }

        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut future = flat_file(minor, flags);
            future[9] = 0x7f;
            future[8] = 0xfe;
            let checksum = crc32(&future[..24]);
            future[24..28].copy_from_slice(&checksum.to_le_bytes());
            let reader = Reader::open(&future).unwrap();
            assert!(reader.has_future_semantics());
            assert_eq!(reader.file_header().version_minor, minor);
            assert_eq!(reader.file_header().flags, flags);
            assert_eq!(reader.flat_image(), None);
        }
    }
}
