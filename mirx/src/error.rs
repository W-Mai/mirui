#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidImage(crate::ImagePayloadError),
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
    /// Reserved-byte slots are required to be zero by spec; non-zero is a
    /// hard reject.
    ReservedNonZero,
}

use crate::font::FontError;
use crate::frames::FramesError;
use crate::meta::{MetaDecodeError, MetaEncodeError};
use crate::model::{ChunkType, InvalidChunkType};
use crate::payload::image::ImagePayloadError;
use crate::payload::palette::{PaletteDecodeError, PaletteEncodeError};
use crate::reader::PayloadValidationError;
use crate::scene::{VectorEncodeError, VectorReadError};

/// Failures while opening or structurally inspecting MIRX bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReadError {
    Parse(ParseError),
    Truncated {
        needed: usize,
        available: usize,
    },
    BadMagic,
    UnsupportedVersion {
        major: u8,
        minor: u8,
    },
    UnknownLayout(u8),
    ReservedNonZero {
        offset: usize,
    },
    HeaderCrcMismatch {
        expected: u32,
        actual: u32,
    },
    UnknownColorFormat(u8),
    StrideTooSmall {
        minimum: u32,
        actual: u32,
    },
    SizeOverflow,
    InvalidFileSize {
        declared: u32,
        minimum: u32,
    },
    TrailingBytes {
        logical_len: usize,
        actual_len: usize,
    },
    TooManyChunks {
        count: u16,
        limit: u16,
    },
    ChunkTableBeforeHeader {
        offset: u32,
    },
    ChunkTableOutOfBounds {
        offset: u32,
        count: u16,
        file_size: u32,
    },
    ChunkPayloadOutOfBounds {
        index: u16,
        offset: u32,
        size: u32,
    },
    ChunkPayloadOverlapsHeader {
        index: u16,
        offset: u32,
        size: u32,
    },
    ChunkPayloadOverlapsTable {
        index: u16,
        offset: u32,
        size: u32,
    },
    CriticalPayload(PayloadValidationError),
    UnknownCriticalChunk {
        index: u16,
        chunk_type: ChunkType,
        payload_offset: u32,
    },
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
    /// The source uses future container semantics that were preserved at open.
    FutureSemanticsReadOnly,
    /// The source retains bytes after its logical MIRX boundary.
    PreservedTrailingBytesReadOnly,
    InvalidChunkId,
    InvalidChunkType,
    ChunkLayoutRequired,
    FlatLayoutRequired,
    /// The current CHUNK state cannot be represented as one lossless FLAT image.
    NotRepresentableAsFlat,
    /// The edit would place another same-typed node before the primary node.
    WouldShadowPrimary,
    /// The selected payload cannot provide primary display hints mechanically.
    PrimaryHintsRequired {
        chunk_type: ChunkType,
    },
    /// Explicit primary hints do not match the selected payload contract.
    InvalidPrimaryHints {
        chunk_type: ChunkType,
    },
    ChunkIdExhausted,
    AllocationFailed,
    RelocationAssumptionRequired {
        chunk_type: ChunkType,
    },
    CriticalAssumptionRequired {
        chunk_type: ChunkType,
    },
    ReservedFlagBits {
        bits: u16,
    },
    InvalidPayload(ImagePayloadError),
    InvalidFont(FontError),
    InvalidVector(VectorEncodeError),
    InvalidMeta(MetaEncodeError),
    InvalidPalette(PaletteEncodeError),
    InvalidFrames(FramesError),
    NonContiguousPayload {
        chunk_type: ChunkType,
    },
}

/// Failures while resolving and decoding a FONT node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontAccessError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not a FONT node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected raw node has no contiguous FONT payload representation.
    NonContiguousPayload,
    /// The selected FONT payload violates its typed contract.
    InvalidPayload(FontError),
    /// Owned metric or atlas-data storage could not be reserved.
    AllocationFailed,
}

impl From<FontError> for FontAccessError {
    fn from(value: FontError) -> Self {
        match value {
            FontError::AllocationFailed => Self::AllocationFailed,
            error => Self::InvalidPayload(error),
        }
    }
}

/// Failures while resolving and decoding a VECTOR node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VectorAccessError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not a VECTOR node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected raw node has no contiguous VECTOR payload representation.
    NonContiguousPayload,
    /// The selected VECTOR payload violates its typed contract.
    InvalidPayload(VectorReadError),
    /// Owned scene storage could not be reserved.
    AllocationFailed,
}

/// Failures while resolving and validating a META node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetaAccessError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not a META node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected raw node has no contiguous META payload representation.
    NonContiguousPayload,
    /// The selected META payload violates its typed contract.
    InvalidPayload(MetaDecodeError),
}

impl From<MetaDecodeError> for MetaAccessError {
    fn from(value: MetaDecodeError) -> Self {
        Self::InvalidPayload(value)
    }
}

/// Failures while resolving and validating a PALETTE node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PaletteAccessError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not a PALETTE node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected raw node has no contiguous PALETTE payload representation.
    NonContiguousPayload,
    /// The selected PALETTE payload violates its typed contract.
    InvalidPayload(PaletteDecodeError),
}

impl From<PaletteDecodeError> for PaletteAccessError {
    fn from(value: PaletteDecodeError) -> Self {
        Self::InvalidPayload(value)
    }
}

/// Failures while resolving and validating a FRAMES node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesAccessError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not a FRAMES node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected raw node has no contiguous FRAMES payload representation.
    NonContiguousPayload,
    /// The selected FRAMES payload violates its typed contract.
    InvalidPayload(FramesError),
}

impl From<FramesError> for FramesAccessError {
    fn from(value: FramesError) -> Self {
        Self::InvalidPayload(value)
    }
}

impl From<VectorReadError> for VectorAccessError {
    fn from(value: VectorReadError) -> Self {
        match value {
            VectorReadError::AllocationFailed => Self::AllocationFailed,
            error => Self::InvalidPayload(error),
        }
    }
}

/// Failure from a transactional typed edit with a fallible callback.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TryEditError<E> {
    Edit(EditError),
    Callback(E),
}

impl<E> From<EditError> for TryEditError<E> {
    fn from(value: EditError) -> Self {
        Self::Edit(value)
    }
}

/// Failures while resolving and decoding an IMAGE node from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageDecodeError {
    /// The container uses preserved semantics newer than this typed accessor.
    FutureSemanticsUnsupported,
    /// Stable chunk identities are available only in CHUNK layout.
    ChunkLayoutRequired,
    /// The identity does not name a live chunk in this document session.
    InvalidChunkId,
    /// The selected chunk is not an IMAGE node.
    UnexpectedChunkType { actual: ChunkType },
    /// The selected IMAGE payload violates its typed contract.
    InvalidPayload(ImagePayloadError),
}

impl From<ImagePayloadError> for ImageDecodeError {
    fn from(value: ImagePayloadError) -> Self {
        Self::InvalidPayload(value)
    }
}

/// Failures while planning or emitting MIRX bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The source uses preserved container semantics newer than MIRX 1.0.
    FutureSemanticsReadOnly,
    /// The source retains bytes after its logical MIRX boundary.
    PreservedTrailingBytesReadOnly,
    /// The selected output layout cannot represent the document without loss.
    NotRepresentableAsFlat,
    /// The document contains more table records than the CHUNK wire field can hold.
    TooManyChunks {
        count: usize,
    },
    /// The selected primary payload has no serializable display hints.
    PrimaryHintsRequired {
        chunk_type: ChunkType,
    },
    /// A raw payload has not been declared safe to move during rewrite.
    RelocationAssumptionRequired {
        index: u16,
        chunk_type: ChunkType,
    },
    /// A critical raw payload has not been declared semantically understood.
    CriticalAssumptionRequired {
        index: u16,
        chunk_type: ChunkType,
    },
    /// Reserved MIRX 1.0 flag bits lack an explicit preservation decision.
    ReservedFlagBits {
        index: u16,
        chunk_type: ChunkType,
        bits: u16,
    },
    /// A structured payload representation violates its own encoded contract.
    InvalidPayload {
        chunk_type: ChunkType,
    },
    SizeOverflow,
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    AllocationFailed,
}
