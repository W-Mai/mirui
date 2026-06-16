//! MIRX binary image format. Two on-disk layouts share an 8-byte header:
//!
//! - **FLAT** (`layout = 0x00`): 28-byte header followed by a single raw pixel
//!   stream and an optional `extra` region (palette for indexed formats, alpha
//!   plane for `RGB565A8`). Designed for zero-copy reads from `include_bytes!`.
//! - **CHUNK** (`layout = 0x01`): 44-byte header with 5-field hint, followed by
//!   a chunk table addressing one or more typed chunks (image / metadata /
//!   frames / vector / palette). Used when FLAT's invariants don't hold
//!   (compression, multiple frames, embedded metadata, …).

#![no_std]

extern crate alloc;
