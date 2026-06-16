#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
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
    UnsupportedCompression(u8),
    /// Reserved-byte slots are required to be zero by spec; non-zero is a
    /// hard reject.
    ReservedNonZero,
}
