use crate::ColorFormat;
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

impl ChunkId {
    pub(crate) const fn from_session_counter(value: u32) -> Self {
        Self(value)
    }
}

/// Raw display hints stored beside the selected primary chunk.
///
/// The color-format byte is retained even when it is not known to this crate.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PrimaryHints {
    color_format: u8,
    width: u32,
    height: u32,
    stride: u32,
}

impl PrimaryHints {
    pub const ZERO: Self = Self::new(0, 0, 0, 0);

    pub const fn new(color_format: u8, width: u32, height: u32, stride: u32) -> Self {
        Self {
            color_format,
            width,
            height,
            stride,
        }
    }

    pub const fn color_format_raw(self) -> u8 {
        self.color_format
    }

    pub const fn known_color_format(self) -> Option<ColorFormat> {
        ColorFormat::from_u8(self.color_format)
    }

    pub const fn width(self) -> u32 {
        self.width
    }

    pub const fn height(self) -> u32 {
        self.height
    }

    pub const fn stride(self) -> u32 {
        self.stride
    }

    pub const fn is_zero(self) -> bool {
        self.color_format == 0 && self.width == 0 && self.height == 0 && self.stride == 0
    }
}

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

    #[test]
    fn primary_hints_retain_raw_color_format() {
        let known = PrimaryHints::new(ColorFormat::RGB565.to_u8(), 8, 4, 16);
        assert_eq!(known.known_color_format(), Some(ColorFormat::RGB565));
        assert_eq!(known.width(), 8);
        assert_eq!(known.height(), 4);
        assert_eq!(known.stride(), 16);

        let unknown = PrimaryHints::new(0xfe, 0, 0, 0);
        assert_eq!(unknown.color_format_raw(), 0xfe);
        assert_eq!(unknown.known_color_format(), None);
        assert!(!unknown.is_zero());
        assert!(PrimaryHints::ZERO.is_zero());
    }
}
