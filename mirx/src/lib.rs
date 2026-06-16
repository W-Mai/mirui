#![no_std]

extern crate alloc;

mod chunk;
mod crc32;
mod error;
mod flat;
mod format;
mod header;

pub use chunk::{ChunkFile, ImageChunk, ImageChunkInput, encode_chunk_image, parse_chunk};
pub use crc32::compute as crc32;
pub use error::ParseError;
pub use flat::{FlatImage, FlatImageInput, encode_flat, parse_flat};
pub use format::{PRIMARY_FORMAT_NONE, PixelFormat};
pub use header::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkEntry, ChunkFileHeader, FILE_HEADER_LEN,
    FLAT_HEADER_LEN, FileHeader, FlatHeader, ImageChunkHeader, Layout, MAGIC, VERSION_MAJOR,
    VERSION_MINOR, chunk_type,
};

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
