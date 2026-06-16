use crate::error::ParseError;

/// Magic bytes at offset 0..4 of every MIRX file: ASCII `"MIRX"`.
pub const MAGIC: [u8; 4] = *b"MIRX";

/// Major version of the format this crate implements. Reader rejects files
/// whose major differs (cross-major changes are breaking by definition).
pub const VERSION_MAJOR: u8 = 1;

/// Minor version this crate emits. Readers tolerate higher minors as long as
/// the higher minor only adds new chunks / fields the reader skips.
pub const VERSION_MINOR: u8 = 0;

pub const FILE_HEADER_LEN: usize = 8;
pub const FLAT_HEADER_LEN: usize = 28;
pub const CHUNK_FILE_HEADER_LEN: usize = 44;
pub const CHUNK_TABLE_ENTRY_LEN: usize = 16;

/// `layout` byte at offset 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Layout {
    Flat = 0x00,
    Chunk = 0x01,
}

impl Layout {
    pub const fn from_u8(b: u8) -> Option<Self> {
        Some(match b {
            0x00 => Self::Flat,
            0x01 => Self::Chunk,
            _ => return None,
        })
    }

    pub const fn to_u8(self) -> u8 {
        self as u8
    }
}

/// Common 8-byte prefix shared by FLAT and CHUNK files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileHeader {
    pub version_major: u8,
    pub version_minor: u8,
    pub layout: Layout,
    pub flags: u8,
}

impl FileHeader {
    /// Read the 8-byte common prefix, validating magic, version major, and
    /// the layout byte.
    pub fn parse(buf: &[u8]) -> Result<Self, ParseError> {
        if buf.len() < FILE_HEADER_LEN {
            return Err(ParseError::Truncated);
        }
        if buf[0..4] != MAGIC {
            return Err(ParseError::BadMagic);
        }
        let version_major = buf[4];
        let version_minor = buf[5];
        if version_major != VERSION_MAJOR {
            return Err(ParseError::UnsupportedVersion {
                major: version_major,
                minor: version_minor,
            });
        }
        let layout = Layout::from_u8(buf[6]).ok_or(ParseError::UnknownLayout(buf[6]))?;
        let flags = buf[7];
        Ok(Self {
            version_major,
            version_minor,
            layout,
            flags,
        })
    }

    /// Serialize the 8-byte prefix.
    pub fn write_into(&self, out: &mut [u8; FILE_HEADER_LEN]) {
        out[0..4].copy_from_slice(&MAGIC);
        out[4] = self.version_major;
        out[5] = self.version_minor;
        out[6] = self.layout.to_u8();
        out[7] = self.flags;
    }
}

/// FLAT-mode 28-byte header (8 common + 20 layout-specific). Pixel data
/// follows immediately at offset 28.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlatHeader {
    pub file: FileHeader,
    pub color_format: u8,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub header_crc32: u32,
}

/// CHUNK-mode 44-byte header (8 common + 36 layout-specific including hint
/// fields and CRC32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkFileHeader {
    pub file: FileHeader,
    pub chunk_count: u16,
    pub chunk_table_offset: u32,
    pub file_size: u32,
    pub primary_chunk_type: u16,
    pub primary_color_format: u8,
    pub primary_width: u32,
    pub primary_height: u32,
    pub primary_stride: u32,
    pub header_crc32: u32,
}

/// One entry in the CHUNK-mode chunk table. 16 bytes on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkEntry {
    pub chunk_type: u16,
    pub chunk_flags: u16,
    pub chunk_offset: u32,
    pub chunk_size: u32,
}

impl ChunkEntry {
    /// `chunk_flags` bit 0: when set, readers that don't recognise
    /// [`ChunkEntry::chunk_type`] must reject the file rather than skip the
    /// chunk. Mirrors PNG's ancillary/critical chunk distinction.
    pub const FLAG_CRITICAL: u16 = 1 << 0;

    pub const fn is_critical(&self) -> bool {
        self.chunk_flags & Self::FLAG_CRITICAL != 0
    }
}

/// Standard `chunk_type` IDs. v1 only fully implements [`Self::IMAGE`]; the
/// rest are reserved on disk so readers can recognise (and skip / reject)
/// chunks emitted by future writers.
pub mod chunk_type {
    pub const IMAGE: u16 = 0x0001;
    pub const FRAMES: u16 = 0x0002;
    pub const VECTOR: u16 = 0x0003;
    pub const META: u16 = 0x0010;
    pub const PALETTE: u16 = 0x0080;
}

/// IMAGE-chunk inner-header (the 32 bytes that start at the chunk's
/// `chunk_offset`, before any padding-to-`data_offset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageChunkHeader {
    pub width: u32,
    pub height: u32,
    pub color_format: u8,
    pub compress: u8,
    pub stride: u32,
    pub data_offset: u32,
    pub data_size: u32,
    pub extra_data_size: u32,
}

impl ImageChunkHeader {
    pub const SIZE: usize = 32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_flat_prefix() {
        let mut buf = [0u8; FILE_HEADER_LEN];
        buf[..4].copy_from_slice(&MAGIC);
        buf[4] = 1;
        buf[5] = 0;
        buf[6] = 0x00;
        buf[7] = 0;
        let header = FileHeader::parse(&buf).unwrap();
        assert_eq!(header.version_major, 1);
        assert_eq!(header.version_minor, 0);
        assert_eq!(header.layout, Layout::Flat);
        assert_eq!(header.flags, 0);
    }

    #[test]
    fn truncated_buffer_is_rejected() {
        assert_eq!(FileHeader::parse(&[]), Err(ParseError::Truncated));
        assert_eq!(FileHeader::parse(&[b'M', b'I']), Err(ParseError::Truncated));
    }

    #[test]
    fn bad_magic_is_rejected() {
        let bad = [0u8; FILE_HEADER_LEN];
        assert_eq!(FileHeader::parse(&bad), Err(ParseError::BadMagic));
    }

    #[test]
    fn unsupported_major_version_is_rejected() {
        let mut buf = [0u8; FILE_HEADER_LEN];
        buf[..4].copy_from_slice(&MAGIC);
        buf[4] = 99;
        buf[5] = 0;
        buf[6] = 0x00;
        assert_eq!(
            FileHeader::parse(&buf),
            Err(ParseError::UnsupportedVersion {
                major: 99,
                minor: 0
            })
        );
    }

    #[test]
    fn unknown_layout_is_rejected() {
        let mut buf = [0u8; FILE_HEADER_LEN];
        buf[..4].copy_from_slice(&MAGIC);
        buf[4] = 1;
        buf[5] = 0;
        buf[6] = 0x42;
        assert_eq!(
            FileHeader::parse(&buf),
            Err(ParseError::UnknownLayout(0x42))
        );
    }

    #[test]
    fn write_then_parse_round_trips() {
        let h = FileHeader {
            version_major: 1,
            version_minor: 0,
            layout: Layout::Chunk,
            flags: 0,
        };
        let mut buf = [0u8; FILE_HEADER_LEN];
        h.write_into(&mut buf);
        assert_eq!(FileHeader::parse(&buf), Ok(h));
    }

    #[test]
    fn chunk_entry_critical_bit() {
        let plain = ChunkEntry {
            chunk_type: chunk_type::IMAGE,
            chunk_flags: 0,
            chunk_offset: 0,
            chunk_size: 0,
        };
        assert!(!plain.is_critical());
        let critical = ChunkEntry {
            chunk_flags: ChunkEntry::FLAG_CRITICAL,
            ..plain
        };
        assert!(critical.is_critical());
    }
}
