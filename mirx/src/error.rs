#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Truncated,
    BadMagic,
    UnsupportedVersion {
        major: u8,
        minor: u8,
    },
    UnknownLayout(u8),
    HeaderCrcMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownColorFormat(u8),
    DimensionOverflow,
    /// Critical chunks (chunk_flags bit 0) the reader doesn't recognise must
    /// be rejected per spec rather than skipped.
    UnknownCriticalChunk(u16),
    UnsupportedCompression(u8),
    /// Reserved-byte slots are required to be zero by spec; non-zero is a
    /// hard reject.
    ReservedNonZero,
}

use crate::InvalidChunkType;

/// Failures while opening or structurally inspecting MIRX bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReadError {
    Parse(ParseError),
    Truncated { needed: usize, available: usize },
    BadMagic,
    UnsupportedVersion { major: u8, minor: u8 },
    UnknownLayout(u8),
    ReservedNonZero { offset: usize },
    HeaderCrcMismatch { expected: u32, actual: u32 },
    UnknownColorFormat(u8),
    StrideTooSmall { minimum: u32, actual: u32 },
    SizeOverflow,
    TooManyChunks { count: u16, limit: u16 },
    ChunkTableBeforeHeader { offset: u32 },
    ChunkPayloadOutOfBounds { index: u16, offset: u32, size: u32 },
    InvalidChunkType,
}

impl From<ParseError> for ReadError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

impl From<InvalidChunkType> for ReadError {
    fn from(_: InvalidChunkType) -> Self {
        Self::InvalidChunkType
    }
}

/// Failures while constructing an editable document.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DocumentError {
    Read(ReadError),
    AllocationFailed,
}

impl From<ReadError> for DocumentError {
    fn from(value: ReadError) -> Self {
        Self::Read(value)
    }
}

/// Failures from an in-memory document mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditError {
    InvalidChunkId,
    InvalidChunkType,
    ChunkIdExhausted,
    AllocationFailed,
}

/// Failures while planning or emitting MIRX bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodeError {
    SizeOverflow,
    BufferTooSmall { needed: usize, available: usize },
    AllocationFailed,
}
