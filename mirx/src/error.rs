/// Reasons MIRX parsing can fail. The variants are stable and add-only;
/// callers should treat unknown variants as generic failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// Buffer is shorter than required to even read the file header.
    Truncated,
    /// Magic bytes are not `"MIRX"`.
    BadMagic,
    /// Major version is not supported by this reader.
    UnsupportedVersion { major: u8, minor: u8 },
    /// `layout` byte is not a layout this reader implements.
    UnknownLayout(u8),
    /// Header CRC32 mismatch — the file is corrupted or truncated mid-header.
    HeaderCrcMismatch { expected: u32, actual: u32 },
    /// Pixel `color_format` byte does not map to a known [`PixelFormat`].
    UnknownColorFormat(u8),
    /// `width × height` (or stride math) overflows the addressable size.
    DimensionOverflow,
    /// CHUNK mode encountered an unknown `chunk_type` whose `critical` bit is
    /// set, which the spec mandates a hard reject for.
    UnknownCriticalChunk(u16),
    /// CHUNK mode IMAGE chunk uses a `compress` algorithm this reader can't
    /// decode.
    UnsupportedCompression(u8),
    /// Reserved-byte or padding-byte slot is non-zero, which the spec
    /// declares illegal.
    ReservedNonZero,
}
