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
#[path = "payload/meta.rs"]
pub mod meta;
mod model;
#[path = "payload/palette.rs"]
pub mod palette;
mod path;
mod payload;
pub mod reader;
pub mod scene;
pub mod types;
mod wire;

pub(crate) use alignment::ByteAlignment;
#[cfg(test)]
pub(crate) use chunk::{ImageChunkInput, encode_chunk_image, encode_chunks};
pub use crc32::compute as crc32;
pub use document::{
    ChunkIter, ChunksOfType, Document, DocumentChunkMut, DocumentChunkRef, PayloadOrigin,
};
#[cfg(test)]
pub(crate) use flat::{FlatImageInput, encode_flat};
pub(crate) use format::ColorFormat;
pub use header::Layout;
pub(crate) use header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, FILE_HEADER_LEN, FLAT_HEADER_LEN, VERSION_MAJOR,
    VERSION_MINOR,
};
pub use model::{ChunkFlags, ChunkId, ChunkType, InvalidChunkType, PrimaryHints};
pub(crate) use payload::image::{ImageAsset, ImagePayloadError, ImageView};
pub use reader::{ChunkRef, Reader};
pub(crate) use reader::{PayloadLimits, ReadOptions, TrailingBytesPolicy};
