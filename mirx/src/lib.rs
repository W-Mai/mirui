#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

mod chunk;
pub mod coding;
mod crc32;
pub mod document;
mod error;
mod flat;
pub mod font;
mod format;
mod header;
pub mod image;
pub mod media;
mod model;
pub mod path;
pub mod payload;
pub mod reader;
pub mod scene;
pub mod types;
mod wire;

pub use chunk::{
    ChunkFile, ImageChunk, ImageChunkInput, encode_chunk_generic, encode_chunk_image,
    encode_chunks, parse_chunk,
};
pub use crc32::compute as crc32;
pub use document::{
    ChunkIter, ChunksOfType, CompatibilityPolicy, CriticalAssumption, Document, DocumentChunkMut,
    DocumentChunkRef, EncodeOptions, FileMetadata, LayoutPolicy, OpenOptions, PayloadInput,
    PayloadOrigin, RawChunkInput, RawChunkPolicy, RawTypePolicy, RelocationAssumption,
    RemovedChunkMetadata, ReservedBitsPolicy,
};
pub use error::{
    DocumentError, EditError, EncodeError, FontAccessError, FramesAccessError, ImageDecodeError,
    MetaAccessError, PaletteAccessError, ParseError, ReadError, TryEditError, VectorAccessError,
};
pub use flat::{FlatImage, FlatImageInput, encode_flat, parse_flat};
pub use font::{
    Font, FontAsset, FontCodepointError, FontCodepointIter, FontCodepoints, FontError,
    FontRepresentation, FontRepresentationError, FontRepresentationFallback,
    FontRepresentationKind, FontRepresentationMatch, FontRepresentationPreference,
    FontRepresentationRequest, FontRepresentations, FontSelectionError, FontView,
};
pub use format::ColorFormat;
pub use header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkEntry, ChunkFileHeader, FILE_HEADER_LEN,
    FLAT_HEADER_LEN, FileHeader, FlatHeader, Layout, MAGIC, VERSION_MAJOR, VERSION_MINOR,
    chunk_type,
};
pub use media::CodingId;
pub use model::{ChunkFlags, ChunkId, ChunkType, InvalidChunkType, PrimaryHints};
pub use path::{Path, PathCmd};
pub use payload::frames::{
    AnimationFrames, AnimationSettings, AssetFrameIter, AtlasFrames, BlendMode, DisposalMode,
    FRAME_COMPOSITION_RECORD_LEN, FRAME_SEQUENCE_RECORD_LEN, FRAME_TIMING_HEADER_LEN, Frame,
    FrameComposition, FrameCompositionAsset, FrameCompositionError, FrameCompositionOverride,
    FrameCompositionTable, FrameCounts, FrameIter, FrameMap, FrameMapAsset, FrameMapError,
    FrameMapIter, FramePresentation, FrameRow, FrameSequence, FrameSequenceError, FrameTiming,
    FrameTimingAsset, FrameTimingEncoding, FrameTimingError, FramesAsset, FramesDecodeError,
    FramesEncodeError, FramesMode, FramesMutationError, FramesView, KeyframeIndex,
    KeyframeIndexAsset, KeyframeIndexError, KeyframeIter, SectionedFramesError,
    SectionedFramesView,
};
pub use payload::image::{ImageAsset, ImageEncodeError, ImagePayloadError, ImageView};
pub use payload::meta::{
    Meta, MetaDecodeError, MetaEncodeError, MetaEntry, MetaEntryIter, MetaEntryRef,
    MetaMutationError, MetaValue, MetaValueRef, MetaView,
};
pub use payload::palette::{
    Palette, PaletteDecodeError, PaletteEncodeError, PaletteMutationError, PaletteView,
};
pub use payload::{ColorTableIter, ColorTableView};
pub use reader::{
    ChunkRef, ComplianceFinding, ContainerHeader, EntryIter, FindingIter, PayloadLimits,
    PayloadLocation, PayloadValidationError, PayloadValidationFailure, ReadOptions, Reader,
    TrailingBytesPolicy,
};
pub use scene::{
    CodecError, CompositeMode, FillRule, GradientStop, GradientUnits, LineCap, LineJoin,
    LinearGradient, Paint, RadialGradient, ResourceRef, Scene, SceneOp, SpreadMode,
    VectorChunkHeader, VectorEncodeError, VectorReadError,
};
pub use types::{Color, Fixed, Point, Rect, Transform};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirxFile<'a> {
    Flat(FlatImage<'a>),
    Chunk(ChunkFile<'a>),
}

pub fn parse(buf: &[u8]) -> Result<MirxFile<'_>, ParseError> {
    let header = FileHeader::parse(buf)?;
    match header.layout {
        Layout::Flat => parse_flat(buf).map(MirxFile::Flat),
        Layout::Chunk => parse_chunk(buf).map(MirxFile::Chunk),
    }
}

/// Only validates the common prefix; layout-specific bytes are untouched.
pub fn peek_header(buf: &[u8]) -> Result<FileHeader, ParseError> {
    FileHeader::parse(buf)
}
