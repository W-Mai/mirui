use super::buffer::{BufferError, Cursor, Emitter};
use crate::media::{CodingId, CodingRecord};

#[cfg(test)]
mod tests;

/// Lossless literal/run coding of fixed-width byte elements.
///
/// Element size defaults to one byte; widths of 2, 3, or 4 bytes may be selected.
/// no sample layout, row boundary, color conversion, or history is implied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rle {
    element: u8,
}
impl Default for Rle {
    fn default() -> Self {
        Self::new()
    }
}

impl Rle {
    pub const fn new() -> Self {
        Self { element: 1 }
    }
    pub fn with_element_size(mut self, bytes: u8) -> Result<Self, RleError> {
        if !(1..=4).contains(&bytes) {
            return Err(RleError::InvalidElementSize(bytes));
        }
        self.element = bytes;
        Ok(self)
    }
    pub const fn element_size(self) -> u8 {
        self.element
    }
    pub const fn record(self) -> CodingRecord<'static> {
        let params: &[u8] = match self.element {
            1 => &[],
            2 => &[2],
            3 => &[3],
            _ => &[4],
        };
        CodingRecord::new(CodingId::RLE, 1, params)
    }
    pub fn from_record(record: CodingRecord<'_>) -> Result<Self, RleError> {
        if record.id() != CodingId::RLE {
            return Err(RleError::UnexpectedCoding(record.id()));
        }
        if record.revision() != 1 {
            return Err(RleError::UnsupportedRevision(record.revision()));
        }
        match record.params() {
            [] => Ok(Self::new()),
            [size @ 2..=4] => Self::new().with_element_size(*size),
            _ => Err(RleError::UnexpectedParameters),
        }
    }

    /// Conservative bound: input bytes plus one control per input element.
    pub fn encoded_bound(self, bytes: usize) -> Result<usize, RleError> {
        bytes
            .checked_add(self.element_count(bytes)?)
            .ok_or(RleError::SizeOverflow)
    }

    /// Exact size of the smaller canonical threshold-two/threshold-three stream.
    pub fn encoded_len(self, input: &[u8]) -> Result<usize, RleError> {
        Ok(self.encoding(input)?.bytes)
    }

    /// Counts both literal/run candidates before writing the smaller one.
    ///
    /// No temporary encoding is allocated. Errors leave output unchanged;
    /// success writes only the encoded prefix. Equal sizes select threshold two.
    pub fn encode_into(self, input: &[u8], output: &mut [u8]) -> Result<usize, RleError> {
        let encoding = self.encoding(input)?;
        if output.len() < encoding.bytes {
            return Err(RleError::OutputTooSmall {
                needed: encoding.bytes,
                available: output.len(),
            });
        }
        self.emit(
            input,
            encoding.threshold,
            &mut Emitter {
                output: Some(&mut output[..encoding.bytes]),
                position: 0,
            },
        )
        .expect("validated RLE encoding");
        Ok(encoding.bytes)
    }

    /// Validates exact input consumption and decoded byte count before writes.
    pub fn plan(self, input: &[u8], decoded_len: usize) -> Result<RleDecodePlan<'_>, RleError> {
        self.element_count(decoded_len)?;
        let mut cursor = Cursor::new(input);
        let mut remaining = decoded_len;
        while remaining > 0 {
            let (bytes, repeat) = self.read(&mut cursor)?;
            let count = bytes.len() * repeat;
            remaining = remaining
                .checked_sub(count)
                .ok_or(RleError::OutputOverflow { remaining, count })?;
        }
        if cursor.position != input.len() {
            return Err(RleError::TrailingData {
                offset: cursor.position,
            });
        }
        Ok(RleDecodePlan {
            codec: self,
            input,
            decoded_len,
        })
    }

    fn element_count(self, bytes: usize) -> Result<usize, RleError> {
        let size = usize::from(self.element);
        if bytes % size != 0 {
            return Err(RleError::PartialElement {
                bytes,
                element_size: self.element,
            });
        }
        Ok(bytes / size)
    }
    fn encoding(self, input: &[u8]) -> Result<Encoding, RleError> {
        self.element_count(input.len())?;
        let mut two = Emitter::count();
        self.emit(input, 2, &mut two)?;
        let mut three = Emitter::count();
        self.emit(input, 3, &mut three)?;
        Ok(if two.position <= three.position {
            Encoding {
                threshold: 2,
                bytes: two.position,
            }
        } else {
            Encoding {
                threshold: 3,
                bytes: three.position,
            }
        })
    }
    fn run(self, input: &[u8], index: usize, count: usize) -> usize {
        let size = usize::from(self.element);
        let value = &input[index * size..(index + 1) * size];
        let mut len = 1;
        while len < 128
            && len < count - index
            && &input[(index + len) * size..(index + len + 1) * size] == value
        {
            len += 1;
        }
        len
    }
    fn emit(
        self,
        input: &[u8],
        threshold: usize,
        emitter: &mut Emitter<'_>,
    ) -> Result<(), BufferError> {
        let size = usize::from(self.element);
        let count = input.len() / size;
        let mut index = 0;
        while index < count {
            let run = self.run(input, index, count);
            if run >= threshold {
                emitter.put(&[0x80 | (run - 1) as u8])?;
                emitter.put(&input[index * size..(index + 1) * size])?;
                index += run;
            } else {
                let start = index;
                index += 1;
                while index < count
                    && index - start < 128
                    && self.run(input, index, count) < threshold
                {
                    index += 1;
                }
                emitter.put(&[(index - start - 1) as u8])?;
                emitter.put(&input[start * size..index * size])?;
            }
        }
        Ok(())
    }
    fn read<'a>(self, cursor: &mut Cursor<'a>) -> Result<(&'a [u8], usize), RleError> {
        let op = cursor.byte()?;
        let count = usize::from(op & 127) + 1;
        let size = usize::from(self.element);
        if op & 128 == 0 {
            Ok((cursor.take(count * size)?, 1))
        } else {
            Ok((cursor.take(size)?, count))
        }
    }
}

struct Encoding {
    threshold: usize,
    bytes: usize,
}

/// Immutable validated RLE stream with an exact caller-output requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RleDecodePlan<'a> {
    codec: Rle,
    input: &'a [u8],
    decoded_len: usize,
}
impl<'a> RleDecodePlan<'a> {
    pub const fn codec(self) -> Rle {
        self.codec
    }
    pub const fn input(self) -> &'a [u8] {
        self.input
    }
    pub const fn decoded_len(self) -> usize {
        self.decoded_len
    }
    /// Replays validated blocks without allocation or partial failure writes.
    pub fn decode_into(self, output: &mut [u8]) -> Result<usize, RleError> {
        if output.len() < self.decoded_len {
            return Err(RleError::OutputTooSmall {
                needed: self.decoded_len,
                available: output.len(),
            });
        }
        let mut position = 0;
        self.for_each_block(|bytes, repeat| {
            let end = position + bytes.len() * repeat;
            for target in output[position..end].chunks_exact_mut(bytes.len()) {
                target.copy_from_slice(bytes);
            }
            position = end;
        });
        Ok(self.decoded_len)
    }
    pub(crate) fn for_each_block(self, mut consume: impl FnMut(&[u8], usize)) {
        let mut cursor = Cursor::new(self.input);
        while cursor.position < self.input.len() {
            let (bytes, repeat) = self.codec.read(&mut cursor).expect("validated RLE stream");
            consume(bytes, repeat);
        }
    }
}

/// Invalid RLE profile, element geometry, bitstream, or caller buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RleError {
    InvalidElementSize(u8),
    UnexpectedCoding(CodingId),
    UnsupportedRevision(u16),
    UnexpectedParameters,
    PartialElement { bytes: usize, element_size: u8 },
    SizeOverflow,
    OutputTooSmall { needed: usize, available: usize },
    Truncated { offset: usize },
    OutputOverflow { remaining: usize, count: usize },
    TrailingData { offset: usize },
}
impl From<BufferError> for RleError {
    fn from(error: BufferError) -> Self {
        match error {
            BufferError::SizeOverflow => Self::SizeOverflow,
            BufferError::Truncated { offset } => Self::Truncated { offset },
        }
    }
}
