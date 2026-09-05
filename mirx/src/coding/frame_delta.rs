use super::buffer::{BufferError, Cursor, Emitter};
use crate::media::{CodingId, CodingRecord};

mod kernel;
#[cfg(target_arch = "aarch64")]
pub use kernel::NeonFrameDelta;
pub use kernel::{FrameDeltaKernel, ScalarFrameDelta};

const REPEAT: u8 = 0x40;
const LITERAL: u8 = 0x80;
const PATTERN: u8 = 0xc0;
const MAX_REPEAT: usize = 64;
const MAX_LITERAL: usize = 64;
const MAX_PATTERN: usize = 16;
const MAX_PATTERN_REPETITIONS: usize = u16::MAX as usize + 2;

/// Lossless byte residuals predicted from the same unit in the previous frame.
///
/// Subtraction and reconstruction use modulo-256 arithmetic. Runs and short
/// repeating residual patterns compact unchanged bytes, constant channel
/// changes, and interleaved pixel channels. Geometry, reference identity,
/// integrity, and frame bounds stay in the surrounding media records.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FrameDelta;

impl FrameDelta {
    pub const fn new() -> Self {
        Self
    }

    pub const fn record(self) -> CodingRecord<'static> {
        CodingRecord::new(CodingId::FRAME_DELTA, 1, &[])
    }

    pub fn from_record(record: CodingRecord<'_>) -> Result<Self, FrameDeltaError> {
        if record.id() != CodingId::FRAME_DELTA {
            return Err(FrameDeltaError::UnexpectedCoding(record.id()));
        }
        if record.revision() != 1 {
            return Err(FrameDeltaError::UnsupportedRevision(record.revision()));
        }
        if !record.params().is_empty() {
            return Err(FrameDeltaError::UnexpectedParameters);
        }
        Ok(Self)
    }

    /// Worst case: one literal control byte per 64 residual bytes.
    pub fn encoded_bound(self, byte_len: usize) -> Result<usize, FrameDeltaError> {
        let controls = byte_len
            .checked_add(MAX_LITERAL - 1)
            .ok_or(FrameDeltaError::SizeOverflow)?
            / MAX_LITERAL;
        byte_len
            .checked_add(controls)
            .ok_or(FrameDeltaError::SizeOverflow)
    }

    pub fn encoded_len(self, reference: &[u8], current: &[u8]) -> Result<usize, FrameDeltaError> {
        self.check_lengths(reference, current)?;
        Ok(self.encoding(reference, current)?.bytes)
    }

    /// Encodes one deterministic residual stream into caller storage.
    ///
    /// Length and output-capacity failures leave the complete output unchanged.
    pub fn encode_into(
        self,
        reference: &[u8],
        current: &[u8],
        output: &mut [u8],
    ) -> Result<usize, FrameDeltaError> {
        self.check_lengths(reference, current)?;
        let encoding = self.encoding(reference, current)?;
        let needed = encoding.bytes;
        if output.len() < needed {
            return Err(FrameDeltaError::OutputTooSmall {
                needed,
                available: output.len(),
            });
        }
        self.emit(
            reference,
            current,
            encoding.threshold,
            &mut Emitter {
                output: Some(&mut output[..needed]),
                position: 0,
            },
        )?;
        Ok(needed)
    }

    /// Validates exact input consumption and reconstructed byte count.
    pub fn plan(
        self,
        input: &[u8],
        decoded_len: usize,
    ) -> Result<FrameDeltaDecodePlan<'_>, FrameDeltaError> {
        let mut cursor = Cursor::new(input);
        let mut remaining = decoded_len;
        while remaining > 0 {
            let (block, count) = Self::read(&mut cursor)?;
            remaining = remaining
                .checked_sub(count)
                .ok_or(FrameDeltaError::OutputOverflow { remaining, count })?;
            if matches!(block, ResidualBlock::Literal(bytes) if bytes.len() != count) {
                return Err(FrameDeltaError::SizeOverflow);
            }
        }
        if !cursor.is_empty() {
            return Err(FrameDeltaError::TrailingData {
                offset: cursor.position,
            });
        }
        Ok(FrameDeltaDecodePlan { input, decoded_len })
    }

    fn check_lengths(self, reference: &[u8], current: &[u8]) -> Result<(), FrameDeltaError> {
        if reference.len() != current.len() {
            return Err(FrameDeltaError::LengthMismatch {
                reference: reference.len(),
                current: current.len(),
            });
        }
        Ok(())
    }

    fn encoding(self, reference: &[u8], current: &[u8]) -> Result<Encoding, FrameDeltaError> {
        let mut two = Emitter::count();
        self.emit(reference, current, 2, &mut two)?;
        let mut three = Emitter::count();
        self.emit(reference, current, 3, &mut three)?;
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

    fn residual(reference: &[u8], current: &[u8], index: usize) -> u8 {
        current[index].wrapping_sub(reference[index])
    }

    fn run_until(self, reference: &[u8], current: &[u8], start: usize, end: usize) -> (u8, usize) {
        let residual = Self::residual(reference, current, start);
        let mut len = 1;
        while len < MAX_REPEAT
            && start + len < end
            && Self::residual(reference, current, start + len) == residual
        {
            len += 1;
        }
        (residual, len)
    }

    fn emit(
        self,
        reference: &[u8],
        current: &[u8],
        threshold: usize,
        emitter: &mut Emitter<'_>,
    ) -> Result<(), BufferError> {
        let mut index = 0;
        while index < current.len() {
            if let Some(pattern) = self.best_pattern(reference, current, index, threshold)? {
                emitter.put(&[PATTERN | (pattern.period - 1) as u8])?;
                emitter.put(&((pattern.repetitions - 2) as u16).to_le_bytes())?;
                for offset in 0..pattern.period {
                    emitter.put(&[Self::residual(reference, current, index + offset)])?;
                }
                index += pattern.period * pattern.repetitions;
                continue;
            }
            index =
                self.emit_base_block(reference, current, index, current.len(), threshold, emitter)?;
        }
        Ok(())
    }

    fn emit_base_block(
        self,
        reference: &[u8],
        current: &[u8],
        mut index: usize,
        end: usize,
        threshold: usize,
        emitter: &mut Emitter<'_>,
    ) -> Result<usize, BufferError> {
        let (residual, run) = self.run_until(reference, current, index, end);
        if residual == 0 && run >= 2 {
            emitter.put(&[(run - 1) as u8])?;
            return Ok(index + run);
        }
        if run >= threshold {
            emitter.put(&[REPEAT | (run - 1) as u8, residual])?;
            return Ok(index + run);
        }
        let start = index;
        index += 1;
        while index < end && index - start < MAX_LITERAL {
            let (residual, run) = self.run_until(reference, current, index, end);
            if (residual == 0 && run >= 2) || run >= threshold {
                break;
            }
            index += 1;
        }
        emitter.put(&[LITERAL | (index - start - 1) as u8])?;
        for (&reference, &current) in reference[start..index].iter().zip(&current[start..index]) {
            emitter.put(&[current.wrapping_sub(reference)])?;
        }
        Ok(index)
    }

    fn base_len(
        self,
        reference: &[u8],
        current: &[u8],
        start: usize,
        end: usize,
        threshold: usize,
    ) -> Result<usize, BufferError> {
        let mut emitter = Emitter::count();
        let mut index = start;
        while index < end {
            index =
                self.emit_base_block(reference, current, index, end, threshold, &mut emitter)?;
        }
        Ok(emitter.position)
    }

    fn best_pattern(
        self,
        reference: &[u8],
        current: &[u8],
        start: usize,
        threshold: usize,
    ) -> Result<Option<Pattern>, BufferError> {
        let remaining = current.len() - start;
        let mut best: Option<(usize, Pattern)> = None;
        for period in 1..=MAX_PATTERN.min(remaining / 2) {
            let mut repetitions = 1;
            while repetitions < MAX_PATTERN_REPETITIONS && (repetitions + 1) * period <= remaining {
                let candidate = start + repetitions * period;
                let matches = (0..period).all(|offset| {
                    Self::residual(reference, current, candidate + offset)
                        == Self::residual(reference, current, start + offset)
                });
                if !matches {
                    break;
                }
                repetitions += 1;
            }
            if repetitions < 2 {
                continue;
            }
            let count = period * repetitions;
            let pattern_bytes = 3 + period;
            let base_bytes = self.base_len(reference, current, start, start + count, threshold)?;
            let Some(saving) = base_bytes.checked_sub(pattern_bytes) else {
                continue;
            };
            if saving == 0 {
                continue;
            }
            let pattern = Pattern {
                period,
                repetitions,
            };
            if best.is_none_or(|(best_saving, best_pattern)| {
                saving > best_saving
                    || (saving == best_saving
                        && (count > best_pattern.period * best_pattern.repetitions
                            || (count == best_pattern.period * best_pattern.repetitions
                                && period < best_pattern.period)))
            }) {
                best = Some((saving, pattern));
            }
        }
        Ok(best.map(|(_, pattern)| pattern))
    }

    fn read<'a>(cursor: &mut Cursor<'a>) -> Result<(ResidualBlock<'a>, usize), FrameDeltaError> {
        let control = cursor.byte()?;
        if control & PATTERN == PATTERN {
            if control & 0x30 != 0 {
                return Err(FrameDeltaError::ReservedControl(control));
            }
            let period = usize::from(control & 0x0f) + 1;
            let count = cursor.take(2)?;
            let repetitions = usize::from(u16::from_le_bytes([count[0], count[1]])) + 2;
            let residuals = cursor.take(period)?;
            let count = period
                .checked_mul(repetitions)
                .ok_or(FrameDeltaError::SizeOverflow)?;
            Ok((ResidualBlock::Pattern(residuals, repetitions), count))
        } else if control & LITERAL != 0 {
            let count = usize::from(control & 0x3f) + 1;
            Ok((ResidualBlock::Literal(cursor.take(count)?), count))
        } else if control & REPEAT != 0 {
            let count = usize::from(control & 0x3f) + 1;
            Ok((ResidualBlock::Repeat(cursor.byte()?), count))
        } else {
            Ok((ResidualBlock::Zero, usize::from(control) + 1))
        }
    }
}

/// Immutable validated residual stream and its borrowed predictor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameDeltaDecodePlan<'a> {
    input: &'a [u8],
    decoded_len: usize,
}

impl<'a> FrameDeltaDecodePlan<'a> {
    pub const fn input(self) -> &'a [u8] {
        self.input
    }

    pub const fn decoded_len(self) -> usize {
        self.decoded_len
    }

    /// Reconstructs validated bytes without allocation or partial failure writes.
    pub fn decode_into(
        self,
        reference: &[u8],
        output: &mut [u8],
    ) -> Result<usize, FrameDeltaError> {
        if reference.len() != self.decoded_len {
            return Err(FrameDeltaError::ReferenceLengthMismatch {
                expected: self.decoded_len,
                actual: reference.len(),
            });
        }
        if output.len() < self.decoded_len {
            return Err(FrameDeltaError::OutputTooSmall {
                needed: self.decoded_len,
                available: output.len(),
            });
        }
        output[..self.decoded_len].copy_from_slice(reference);
        self.apply_into(output)
    }

    /// Applies residuals to a buffer that already contains the predictor.
    pub fn apply_into(self, output: &mut [u8]) -> Result<usize, FrameDeltaError> {
        #[cfg(target_arch = "aarch64")]
        let mut kernel = NeonFrameDelta;
        #[cfg(not(target_arch = "aarch64"))]
        let mut kernel = ScalarFrameDelta;
        self.apply_with(output, &mut kernel)
    }

    /// Applies residuals through a caller-selected execution kernel.
    pub fn apply_with(
        self,
        output: &mut [u8],
        kernel: &mut impl FrameDeltaKernel,
    ) -> Result<usize, FrameDeltaError> {
        if output.len() < self.decoded_len {
            return Err(FrameDeltaError::OutputTooSmall {
                needed: self.decoded_len,
                available: output.len(),
            });
        }
        let mut position = 0;
        self.for_each_block(|block, count| {
            let end = position + count;
            match block {
                ResidualBlock::Zero => {}
                ResidualBlock::Repeat(residual) => {
                    kernel.add_repeat(&mut output[position..end], residual);
                }
                ResidualBlock::Literal(residuals) => {
                    kernel.add_literals(&mut output[position..end], residuals);
                }
                ResidualBlock::Pattern(residuals, repetitions) => {
                    debug_assert_eq!(residuals.len() * repetitions, count);
                    kernel.add_pattern(&mut output[position..end], residuals);
                }
            }
            position = end;
        });
        Ok(self.decoded_len)
    }

    pub(crate) fn for_each_block(self, mut consume: impl FnMut(ResidualBlock<'_>, usize)) {
        let mut cursor = Cursor::new(self.input);
        while !cursor.is_empty() {
            let (residuals, count) =
                FrameDelta::read(&mut cursor).expect("validated frame-delta residual stream");
            consume(residuals, count);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResidualBlock<'a> {
    Zero,
    Repeat(u8),
    Literal(&'a [u8]),
    Pattern(&'a [u8], usize),
}

struct Encoding {
    threshold: usize,
    bytes: usize,
}

#[derive(Clone, Copy)]
struct Pattern {
    period: usize,
    repetitions: usize,
}

/// Invalid profile, residual stream, predictor, or caller output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameDeltaError {
    UnexpectedCoding(CodingId),
    UnsupportedRevision(u16),
    UnexpectedParameters,
    LengthMismatch { reference: usize, current: usize },
    ReferenceLengthMismatch { expected: usize, actual: usize },
    SizeOverflow,
    OutputTooSmall { needed: usize, available: usize },
    Truncated { offset: usize },
    ReservedControl(u8),
    OutputOverflow { remaining: usize, count: usize },
    TrailingData { offset: usize },
}

impl From<BufferError> for FrameDeltaError {
    fn from(error: BufferError) -> Self {
        match error {
            BufferError::SizeOverflow => Self::SizeOverflow,
            BufferError::Truncated { offset } => Self::Truncated { offset },
        }
    }
}

#[cfg(test)]
mod tests;
