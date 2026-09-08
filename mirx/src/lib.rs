#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

mod alignment;
#[cfg(test)]
mod chunk;
pub mod coding;
mod crc32;
pub mod document;
mod error;
#[cfg(test)]
mod flat;
pub mod font;
mod format;
#[path = "payload/frames.rs"]
pub mod frames;
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

pub use alignment::{ByteAlignment, InvalidByteAlignment};
#[cfg(test)]
pub(crate) use chunk::{ImageChunkInput, encode_chunk_image, encode_chunks};
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
#[cfg(test)]
pub(crate) use flat::{FlatImageInput, encode_flat};
pub use font::{
    Font, FontAsset, FontError, FontRepresentation, FontRepresentationError,
    FontRepresentationFallback, FontRepresentationKind, FontRepresentationMatch,
    FontRepresentationPreference, FontRepresentationRequest, FontRepresentations,
    FontSelectionError, FontView,
};
pub(crate) use format::ColorFormat;
pub use header::Layout;
pub(crate) use header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, FILE_HEADER_LEN, FLAT_HEADER_LEN, VERSION_MAJOR,
    VERSION_MINOR,
};
pub use media::CodingId;
pub use model::{ChunkFlags, ChunkId, ChunkType, InvalidChunkType, PrimaryHints};
pub use path::{Path, PathCmd};
pub(crate) use payload::image::{ImageAsset, ImagePayloadError, ImageView};
pub use payload::meta::{
    Meta, MetaDecodeError, MetaEncodeError, MetaEntry, MetaEntryIter, MetaEntryRef,
    MetaMutationError, MetaValue, MetaValueRef, MetaView,
};
pub use payload::palette::{
    Palette, PaletteDecodeError, PaletteEncodeError, PaletteMutationError, PaletteView,
};
pub use payload::{ColorTableIter, ColorTableView};
pub use reader::{
    ChunkRef, ComplianceFinding, EntryIter, FindingIter, PayloadLimits, PayloadLocation,
    PayloadValidationError, PayloadValidationFailure, ReadOptions, Reader, TrailingBytesPolicy,
};
pub use scene::{
    CodecError, CompositeMode, FillRule, GradientStop, GradientUnits, LineCap, LineJoin,
    LinearGradient, Paint, RadialGradient, ResourceRef, Scene, SceneOp, SpreadMode,
    VectorChunkHeader, VectorEncodeError, VectorReadError,
};
pub use types::{Color, Fixed, Point, Rect, Transform};
