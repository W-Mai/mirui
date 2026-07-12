use crate::header::{ChunkEntry, chunk_type};

/// Open MIRX chunk type.
///
/// Every nonzero `u16` is representable so applications can preserve and edit
/// custom chunks without waiting for a crate release. Zero is reserved by the
/// CHUNK header to mean that no primary chunk is selected.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkType(u16);

impl ChunkType {
    pub const IMAGE: Self = Self(chunk_type::IMAGE);
    pub const FRAMES: Self = Self(chunk_type::FRAMES);
    pub const VECTOR: Self = Self(chunk_type::VECTOR);
    pub const FONT: Self = Self(chunk_type::FONT);
    pub const META: Self = Self(chunk_type::META);
    pub const PALETTE: Self = Self(chunk_type::PALETTE);

    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for ChunkType {
    type Error = InvalidChunkType;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidChunkType)
    }
}

impl From<ChunkType> for u16 {
    fn from(value: ChunkType) -> Self {
        value.raw()
    }
}

/// Error returned when chunk type zero is used as a table-entry type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidChunkType;

/// Open MIRX chunk flags.
///
/// Unknown bits are deliberately retained for future-compatible raw editing.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ChunkFlags(u16);

impl ChunkFlags {
    pub const NONE: Self = Self(0);
    pub const CRITICAL: Self = Self(ChunkEntry::FLAG_CRITICAL);

    pub const fn from_bits_retain(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn is_critical(self) -> bool {
        self.0 & Self::CRITICAL.0 != 0
    }
}

/// Stable identity for one chunk inside a single editable document session.
///
/// The value is intentionally opaque and is never serialized into MIRX bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkId(u32);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_type_covers_standard_and_custom_nonzero_values() {
        assert_eq!(ChunkType::IMAGE.raw(), chunk_type::IMAGE);
        assert_eq!(ChunkType::new(0), None);
        assert_eq!(ChunkType::new(0xbeef).map(ChunkType::raw), Some(0xbeef));
        assert_eq!(ChunkType::try_from(0), Err(InvalidChunkType));
    }

    #[test]
    fn chunk_flags_retain_unknown_bits() {
        let flags = ChunkFlags::from_bits_retain(0xa501);
        assert_eq!(flags.bits(), 0xa501);
        assert!(flags.is_critical());
        assert!(!ChunkFlags::NONE.is_critical());
    }
}
