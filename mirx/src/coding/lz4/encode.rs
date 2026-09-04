use super::{Lz4, Lz4Error};
use crate::coding::buffer::{BufferError, Emitter};

/// Independent LZ4 encoder borrowing a fixed caller-owned hash table.
///
/// Table size controls compression choices, not the block format. Every pass
/// resets table contents; no allocation, resizing or input retention occurs.
#[derive(Debug)]
pub struct Lz4Encoder<'a> {
    table: &'a mut [u32],
    shift: u32,
}
impl<'a> Lz4Encoder<'a> {
    pub(super) fn new(table: &'a mut [u32]) -> Result<Self, Lz4Error> {
        if !(16..=65536).contains(&table.len()) || !table.len().is_power_of_two() {
            return Err(Lz4Error::InvalidWorkspace {
                entries: table.len(),
            });
        }
        let shift = 32 - table.len().trailing_zeros();
        Ok(Self { table, shift })
    }
    /// Number of borrowed u32 entries; the workspace never grows.
    pub fn table_len(&self) -> usize {
        self.table.len()
    }
    /// Counts exact bytes using the same parser and emitter as encoding.
    pub fn encoded_len(&mut self, input: &[u8]) -> Result<usize, Lz4Error> {
        Lz4.validate_len(input.len())?;
        let mut emitter = Emitter::count();
        self.emit(input, &mut emitter)?;
        Ok(emitter.position)
    }
    /// Counts before writing; errors preserve output and success preserves its suffix.
    ///
    /// Workspace contents are scratch and may change even on insufficient output.
    pub fn encode_into(&mut self, input: &[u8], output: &mut [u8]) -> Result<usize, Lz4Error> {
        let len = self.encoded_len(input)?;
        if output.len() < len {
            return Err(Lz4Error::OutputTooSmall {
                needed: len,
                available: output.len(),
            });
        }
        self.emit(
            input,
            &mut Emitter {
                output: Some(&mut output[..len]),
                position: 0,
            },
        )
        .expect("validated LZ4 encoding");
        Ok(len)
    }
    fn hash(&self, input: &[u8], position: usize) -> usize {
        let word = u32::from_le_bytes(
            input[position..position + 4]
                .try_into()
                .expect("LZ4 search bounds"),
        );
        (word.wrapping_mul(2654435761) >> self.shift) as usize
    }
    fn emit(&mut self, input: &[u8], emitter: &mut Emitter<'_>) -> Result<(), BufferError> {
        self.table.fill(0);
        let mut anchor = 0;
        let mut position = 0;
        if let Some(last_match_start) = input.len().checked_sub(12) {
            while position <= last_match_start {
                let hash = self.hash(input, position);
                let previous = self.table[hash];
                self.table[hash] = position as u32 + 1;
                let Some(source) = previous.checked_sub(1) else {
                    position += 1;
                    continue;
                };
                let mut source = source as usize;
                let distance = position - source;
                if distance > u16::MAX as usize
                    || input[source..source + 4] != input[position..position + 4]
                {
                    position += 1;
                    continue;
                }
                let mut len = 4;
                while position > anchor && source > 0 && input[position - 1] == input[source - 1] {
                    position -= 1;
                    source -= 1;
                    len += 1;
                }
                while position + len < input.len() - 5
                    && input[source + len] == input[position + len]
                {
                    len += 1;
                }
                Self::sequence(
                    emitter,
                    &input[anchor..position],
                    Some((distance as u16, len)),
                )?;
                position += len;
                anchor = position;
                for index in position - 2..position {
                    let hash = self.hash(input, index);
                    self.table[hash] = index as u32 + 1;
                }
            }
        }
        Self::sequence(emitter, &input[anchor..], None)
    }
    fn sequence(
        emitter: &mut Emitter<'_>,
        literals: &[u8],
        matched: Option<(u16, usize)>,
    ) -> Result<(), BufferError> {
        let len = matched.map_or(0, |(_, len)| len - 4);
        emitter.put(&[((literals.len().min(15) as u8) << 4) | len.min(15) as u8])?;
        Self::length(emitter, literals.len())?;
        emitter.put(literals)?;
        if let Some((distance, _)) = matched {
            emitter.put(&distance.to_le_bytes())?;
            Self::length(emitter, len)?;
        }
        Ok(())
    }
    fn length(emitter: &mut Emitter<'_>, len: usize) -> Result<(), BufferError> {
        if let Some(mut remaining) = len.checked_sub(15) {
            while remaining >= 255 {
                emitter.put(&[255])?;
                remaining -= 255;
            }
            emitter.put(&[remaining as u8])?;
        }
        Ok(())
    }
}
