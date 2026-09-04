use super::buffer::{BufferError, Cursor};
use crate::media::{CodingId, CodingRecord};

mod encode;
pub use encode::Lz4Encoder;

#[cfg(test)]
mod tests;

/// Independent LZ4 blocks with caller-supplied decoded length.
///
/// Blocks contain no frame, size prefix, external dictionary, or cross-unit
/// history. The final sequence contains only literals and uses a zero match
/// nibble. Blocks with matches retain at least 5 final literals and begin the
/// last match at least 12 decoded bytes before the end.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Lz4;

impl Lz4 {
    pub const fn new() -> Self {
        Self
    }
    /// Suggested hash-table length in u32 entries (4 KiB); callers may vary it.
    pub const TABLE_LEN: usize = 1024;

    /// Borrows a power-of-two hash table with 16..=65536 u32 entries.
    pub fn encoder(self, table: &mut [u32]) -> Result<Lz4Encoder<'_>, Lz4Error> {
        Lz4Encoder::new(table)
    }

    /// Conservative block bound with checked arithmetic and u32 input limits.
    pub fn encoded_bound(self, bytes: usize) -> Result<usize, Lz4Error> {
        self.validate_len(bytes)?;
        bytes
            .checked_add(bytes / 255)
            .and_then(|n| n.checked_add(16))
            .ok_or(Lz4Error::SizeOverflow)
    }

    fn validate_len(self, bytes: usize) -> Result<(), Lz4Error> {
        u32::try_from(bytes)
            .map(|_| ())
            .map_err(|_| Lz4Error::InputTooLarge { bytes })
    }
    pub const fn record(self) -> CodingRecord<'static> {
        CodingRecord::new(CodingId::LZ4, 1, &[])
    }
    pub fn from_record(record: CodingRecord<'_>) -> Result<Self, Lz4Error> {
        if record.id() != CodingId::LZ4 {
            return Err(Lz4Error::UnexpectedCoding(record.id()));
        }
        if record.revision() != 1 {
            return Err(Lz4Error::UnsupportedRevision(record.revision()));
        }
        if !record.params().is_empty() {
            return Err(Lz4Error::UnexpectedParameters);
        }
        Ok(Self)
    }

    /// Checks every sequence and backward reference without allocating history.
    ///
    /// Successful preflight consumes the complete input and produces exactly
    /// `decoded_len` bytes. Output capacity is checked separately at execution.
    pub fn plan(self, input: &[u8], decoded_len: usize) -> Result<Lz4DecodePlan<'_>, Lz4Error> {
        let mut cursor = Cursor::new(input);
        let mut remaining = decoded_len;
        let mut last_match_remaining = None;
        loop {
            let sequence = Sequence::read(&mut cursor)?;
            remaining =
                remaining
                    .checked_sub(sequence.literals.len())
                    .ok_or(Lz4Error::OutputOverflow {
                        remaining,
                        count: sequence.literals.len(),
                    })?;
            if let Some((distance, len)) = sequence.match_ {
                let produced = decoded_len - remaining;
                if distance == 0 || usize::from(distance) > produced {
                    return Err(Lz4Error::InvalidOffset { distance, produced });
                }
                last_match_remaining = Some(remaining);
                remaining = remaining.checked_sub(len).ok_or(Lz4Error::OutputOverflow {
                    remaining,
                    count: len,
                })?;
            } else {
                if remaining != 0 {
                    return Err(Lz4Error::DecodedSizeMismatch {
                        expected: decoded_len,
                        actual: decoded_len - remaining,
                    });
                }
                if last_match_remaining.is_some_and(|len| len < 12 || sequence.literals.len() < 5) {
                    return Err(Lz4Error::InvalidBlockEnd);
                }
                break;
            }
        }
        Ok(Lz4DecodePlan { input, decoded_len })
    }
}

struct Sequence<'a> {
    literals: &'a [u8],
    match_: Option<(u16, usize)>,
}
impl<'a> Sequence<'a> {
    fn read(cursor: &mut Cursor<'a>) -> Result<Self, Lz4Error> {
        let token = cursor.byte()?;
        let literal_len = Self::length(cursor, token >> 4)?;
        let literals = cursor.take(literal_len)?;
        let match_ = if cursor.is_empty() {
            if token & 15 != 0 {
                return Err(Lz4Error::InvalidBlockEnd);
            }
            None
        } else {
            let bytes = cursor.take(2)?;
            let distance = u16::from_le_bytes([bytes[0], bytes[1]]);
            let len = Self::length(cursor, token & 15)?
                .checked_add(4)
                .ok_or(Lz4Error::SizeOverflow)?;
            Some((distance, len))
        };
        Ok(Self { literals, match_ })
    }
    fn length(cursor: &mut Cursor<'_>, nibble: u8) -> Result<usize, Lz4Error> {
        let mut len = usize::from(nibble);
        if nibble == 15 {
            loop {
                let byte = cursor.byte()?;
                len = len
                    .checked_add(usize::from(byte))
                    .ok_or(Lz4Error::SizeOverflow)?;
                if byte != 255 {
                    break;
                }
            }
        }
        Ok(len)
    }
}

/// Immutable validated block with no external dictionary or temporary output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Lz4DecodePlan<'a> {
    input: &'a [u8],
    decoded_len: usize,
}
impl<'a> Lz4DecodePlan<'a> {
    pub const fn codec(self) -> Lz4 {
        Lz4
    }
    pub const fn input(self) -> &'a [u8] {
        self.input
    }
    pub const fn decoded_len(self) -> usize {
        self.decoded_len
    }
    /// Decodes into caller memory, preserving the suffix and all bytes on error.
    pub fn decode_into(self, output: &mut [u8]) -> Result<usize, Lz4Error> {
        if output.len() < self.decoded_len {
            return Err(Lz4Error::OutputTooSmall {
                needed: self.decoded_len,
                available: output.len(),
            });
        }
        let mut position = 0;
        self.for_each_sequence(|literals, match_| {
            let end = position + literals.len();
            output[position..end].copy_from_slice(literals);
            position = end;
            if let Some((distance, len)) = match_ {
                let source = position - usize::from(distance);
                let end = position + len;
                while position < end {
                    let count = (position - source).min(end - position);
                    output.copy_within(source..source + count, position);
                    position += count;
                }
            }
        });
        Ok(self.decoded_len)
    }
    pub(crate) fn for_each_sequence(self, mut consume: impl FnMut(&[u8], Option<(u16, usize)>)) {
        let mut cursor = Cursor::new(self.input);
        while !cursor.is_empty() {
            let sequence = Sequence::read(&mut cursor).expect("validated LZ4 block");
            consume(sequence.literals, sequence.match_);
        }
    }
}

/// Invalid LZ4 profile, sequence, decoded length, or caller output capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Lz4Error {
    InvalidWorkspace { entries: usize },
    InputTooLarge { bytes: usize },
    UnexpectedCoding(CodingId),
    UnsupportedRevision(u16),
    UnexpectedParameters,
    SizeOverflow,
    Truncated { offset: usize },
    InvalidOffset { distance: u16, produced: usize },
    InvalidBlockEnd,
    OutputOverflow { remaining: usize, count: usize },
    DecodedSizeMismatch { expected: usize, actual: usize },
    OutputTooSmall { needed: usize, available: usize },
}
impl From<BufferError> for Lz4Error {
    fn from(error: BufferError) -> Self {
        match error {
            BufferError::SizeOverflow => Self::SizeOverflow,
            BufferError::Truncated { offset } => Self::Truncated { offset },
        }
    }
}
