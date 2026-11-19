//! Layer placement for a sprite built from a MIRX primary IMAGE atlas.
//!
//! The optional chunk identifies the file's primary IMAGE by role. Moving the
//! chunk table leaves its payload valid. Layer zero is the bottom layer.
#![no_std]

extern crate alloc;

mod prepared;
pub use prepared::{PreparedSpriteStack, SparseLevel, StackSample};

use alloc::vec;
use mirx::extension::{Extension, Policy, Relocation};
use mirx::image::{AtlasMap, AtlasMapError, ImageReadError};
use mirx::reader::ReadError;
use mirx::{ChunkFlags, ChunkType, Reader};

/// Noncritical sprite-stack descriptor chunk. Its payload starts with `SSTK`.
pub const CHUNK_TYPE: ChunkType = ChunkType::new(0x5353).unwrap();
const HEADER_BYTES: usize = 24;
const REGION_BYTES: usize = 16;
const VERSION: u16 = 1;

/// Errors opening or encoding the sprite-stack placement and its atlas reference.
#[derive(Debug, PartialEq, Eq)]
pub enum StackError {
    Container(ReadError),
    Image(ImageReadError),
    MissingPrimaryImage,
    MissingDescriptor,
    DuplicateDescriptor,
    InvalidFlags,
    InvalidHeader,
    UnsupportedVersion(u16),
    InvalidCanvas,
    InvalidLayerCount,
    InvalidSpacing,
    InvalidPivot,
    InvalidRegion { layer: usize },
    Atlas(AtlasMapError),
    OutputTooSmall,
}

impl core::fmt::Display for StackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "invalid MIRX sprite stack: {self:?}")
    }
}

impl core::error::Error for StackError {}

/// Borrowed common canvas, pivot, layer spacing and bottom-to-top rectangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackDescriptor<'a> {
    canvas: (u16, u16),
    pivot: [i32; 2],
    spacing: i32,
    layers: AtlasMap<'a>,
}

impl<'a> StackDescriptor<'a> {
    /// Constructs a descriptor from a validated atlas map. Pivot components and
    /// layer spacing use signed units of 1/256 model pixel. Empty layers retain
    /// their height. Each nonempty layer spans the entire logical canvas.
    pub fn new(
        canvas: (u16, u16),
        pivot: [i32; 2],
        spacing: i32,
        layers: AtlasMap<'a>,
    ) -> Result<Self, StackError> {
        if canvas.0 == 0 || canvas.1 == 0 {
            return Err(StackError::InvalidCanvas);
        }
        let count = u16::try_from(layers.len()).map_err(|_| StackError::InvalidLayerCount)?;
        if count == 0 {
            return Err(StackError::InvalidLayerCount);
        }
        if spacing < 0 || spacing.checked_mul(i32::from(count - 1)).is_none() {
            return Err(StackError::InvalidSpacing);
        }
        for (axis, length) in pivot.into_iter().zip([canvas.0, canvas.1]) {
            if axis.checked_neg().is_none()
                || (256 * i32::from(length - 1)).checked_sub(axis).is_none()
            {
                return Err(StackError::InvalidPivot);
            }
        }
        for (layer, region) in layers.iter().enumerate() {
            if region.width() != 0
                && (region.width(), region.height()) != (u32::from(canvas.0), u32::from(canvas.1))
            {
                return Err(StackError::InvalidRegion { layer });
            }
        }
        Ok(Self {
            canvas,
            pivot,
            spacing,
            layers,
        })
    }

    /// Opens the optional descriptor for the primary IMAGE and borrows its map.
    pub fn from_mirx(bytes: &'a [u8]) -> Result<Self, StackError> {
        Self::from_reader(&Reader::open(bytes).map_err(StackError::Container)?)
    }

    /// Reuses an open container and obtains atlas dimensions from its primary image.
    pub fn from_reader(reader: &Reader<'a>) -> Result<Self, StackError> {
        let mut matching = reader
            .chunks()
            .filter(|chunk| chunk.chunk_type() == CHUNK_TYPE);
        let chunk = matching.next().ok_or(StackError::MissingDescriptor)?;
        if matching.next().is_some() {
            return Err(StackError::DuplicateDescriptor);
        }
        if chunk.flags() != ChunkFlags::NONE {
            return Err(StackError::InvalidFlags);
        }
        let image = reader
            .primary()
            .map_err(StackError::Container)?
            .ok_or(StackError::MissingPrimaryImage)?
            .image()
            .map_err(StackError::Image)?
            .ok_or(StackError::MissingPrimaryImage)?;
        Self::open(
            image.surface().width(),
            image.surface().height(),
            chunk.payload(),
        )
    }

    /// Opens one extension payload against the provided atlas extent.
    pub fn open(width: u32, height: u32, payload: &'a [u8]) -> Result<Self, StackError> {
        let header = payload
            .get(..HEADER_BYTES)
            .ok_or(StackError::InvalidHeader)?;
        if header[..4] != *b"SSTK" {
            return Err(StackError::InvalidHeader);
        }
        let u16_at = |i| u16::from_le_bytes([header[i], header[i + 1]]);
        let i32_at =
            |i| i32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
        let version = u16_at(4);
        if version != VERSION {
            return Err(StackError::UnsupportedVersion(version));
        }
        let records = &payload[HEADER_BYTES..];
        if records.len() != usize::from(u16_at(10)) * REGION_BYTES {
            return Err(StackError::InvalidLayerCount);
        }
        let layers = AtlasMap::open(width, height, records).map_err(StackError::Atlas)?;
        Self::new(
            (u16_at(6), u16_at(8)),
            [i32_at(12), i32_at(16)],
            i32_at(20),
            layers,
        )
    }

    pub const fn canvas(self) -> (u16, u16) {
        self.canvas
    }
    /// Common pivot in source pixels multiplied by 256, measured from the top left.
    pub const fn pivot_q8(self) -> [i32; 2] {
        self.pivot
    }
    /// Vertical distance between adjacent slices in model pixels multiplied by 256.
    pub const fn spacing_q8(self) -> i32 {
        self.spacing
    }
    pub const fn layers(self) -> AtlasMap<'a> {
        self.layers
    }

    /// Payload size, including empty layer records.
    pub fn encoded_len(self) -> usize {
        HEADER_BYTES + self.layers.len() * REGION_BYTES
    }

    /// Writes the descriptor into caller-owned storage and reports the used length.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, StackError> {
        let len = self.encoded_len();
        let bytes = out.get_mut(..len).ok_or(StackError::OutputTooSmall)?;
        bytes[..4].copy_from_slice(b"SSTK");
        let dimensions = [
            VERSION,
            self.canvas.0,
            self.canvas.1,
            self.layers.len() as u16,
        ];
        for (field, value) in bytes[4..12].chunks_exact_mut(2).zip(dimensions) {
            field.copy_from_slice(&value.to_le_bytes());
        }
        for (field, value) in bytes[12..HEADER_BYTES].chunks_exact_mut(4).zip([
            self.pivot[0],
            self.pivot[1],
            self.spacing,
        ]) {
            field.copy_from_slice(&value.to_le_bytes());
        }
        for (out, region) in bytes[HEADER_BYTES..]
            .chunks_exact_mut(REGION_BYTES)
            .zip(self.layers)
        {
            let rect = [region.x(), region.y(), region.width(), region.height()];
            for (field, value) in out.chunks_exact_mut(4).zip(rect) {
                field.copy_from_slice(&value.to_le_bytes());
            }
        }
        Ok(len)
    }

    /// Encodes an owned noncritical extension for a document writer.
    pub fn extension(self) -> Extension<'static> {
        let mut bytes = vec![0; self.encoded_len()];
        self.encode_into(&mut bytes)
            .expect("exact descriptor capacity");
        Extension::owned(CHUNK_TYPE, bytes).with_policy(relocation_policy())
    }
}

/// Allows structural relocation by an editor that maintains the primary IMAGE binding.
pub const fn relocation_policy() -> Policy {
    Policy::infer().with_relocation(Relocation::AssumeRelocatable)
}

#[cfg(test)]
mod tests;
