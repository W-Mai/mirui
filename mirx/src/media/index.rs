use core::{iter::FusedIterator, ops::Range};

use crate::ByteAlignment;
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

#[cfg(test)]
mod aligned_tests;

#[cfg(test)]
fn alignment(bytes: u32) -> ByteAlignment {
    ByteAlignment::new(bytes).unwrap()
}

/// Number of units between stored length-table checkpoints.
pub const UNIT_CHECKPOINT_INTERVAL: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Storage<'a> {
    Fixed {
        size: u32,
        step: u32,
    },
    Ranges(&'a [Range<u32>]),
    Offsets(&'a [u8]),
    Lengths {
        checkpoints: &'a [u8],
        lengths: &'a [u8],
        width: LengthWidth,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LengthWidth {
    U16,
    U32,
}
impl LengthWidth {
    fn bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }
    fn read(self, bytes: &[u8], index: usize) -> u32 {
        match self {
            Self::U16 => u32::from(read_u16_le(bytes, index * 2).expect("complete unit length")),
            Self::U32 => read_u32_le(bytes, index * 4).expect("complete unit length"),
        }
    }
}

/// Validated DATA-relative byte ranges without per-unit geometry or allocation.
///
/// This index describes storage, not decoder independence. Groups and profiles
/// validate count, total coverage, geometry, references, and empty-unit policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitIndex<'a> {
    storage: Storage<'a>,
    count: usize,
    byte_len: u32,
    alignment: ByteAlignment,
}

impl<'a> UnitIndex<'a> {
    /// Derives equal-size ranges without storing any index bytes.
    /// Alignment pads starts between units, never the final unit's end.
    pub fn fixed(
        count: u32,
        unit_bytes: u32,
        alignment: ByteAlignment,
    ) -> Result<Self, UnitIndexError> {
        let step = if count > 1 {
            Self::aligned(unit_bytes, alignment)?
        } else {
            unit_bytes
        };
        let byte_len = if count == 0 {
            0
        } else {
            (count - 1)
                .checked_mul(step)
                .and_then(|n| n.checked_add(unit_bytes))
                .ok_or(UnitIndexError::SizeOverflow)?
        };
        let count = usize::try_from(count).map_err(|_| UnitIndexError::SizeOverflow)?;
        Ok(Self {
            storage: Storage::Fixed {
                size: unit_bytes,
                step,
            },
            count,
            byte_len,
            alignment,
        })
    }

    /// Borrows canonical DATA-relative ranges.
    pub fn ranges(
        ranges: &'a [Range<u32>],
        alignment: ByteAlignment,
    ) -> Result<Self, UnitIndexError> {
        u32::try_from(ranges.len()).map_err(|_| UnitIndexError::SizeOverflow)?;
        let mut previous_end = 0;
        for (index, range) in ranges.iter().enumerate() {
            if range.end < range.start {
                return Err(UnitIndexError::InvalidRange {
                    index: index as u32,
                    start: range.start,
                    end: range.end,
                });
            }
            let expected = Self::aligned(previous_end, alignment)?;
            if range.start != expected {
                return Err(UnitIndexError::RangeStartMismatch {
                    index: index as u32,
                    expected,
                    actual: range.start,
                });
            }
            previous_end = range.end;
        }
        Ok(Self {
            storage: Storage::Ranges(ranges),
            count: ranges.len(),
            byte_len: previous_end,
            alignment,
        })
    }

    /// Borrows little-endian u32 offsets, including the final range end.
    /// The first offset must be zero; subsequent offsets must be monotone.
    pub fn offsets(bytes: &'a [u8]) -> Result<Self, UnitIndexError> {
        u32::try_from(bytes.len()).map_err(|_| UnitIndexError::SizeOverflow)?;
        if bytes.len() < 4 || bytes.len() % 4 != 0 {
            return Err(UnitIndexError::InvalidOffsetTableLength(bytes.len()));
        }
        let first = read_u32_le(bytes, 0).expect("complete first offset");
        if first != 0 {
            return Err(UnitIndexError::FirstOffsetNonZero(first));
        }
        let count = bytes.len() / 4 - 1;
        let mut previous = 0;
        for index in 0..count {
            let next = read_u32_le(bytes, (index + 1) * 4).expect("complete offset");
            if next < previous {
                return Err(UnitIndexError::OffsetsOutOfOrder {
                    index: index as u32,
                    previous,
                    next,
                });
            }
            previous = next;
        }
        Ok(Self {
            storage: Storage::Offsets(bytes),
            count,
            byte_len: previous,
            alignment: ByteAlignment::ONE,
        })
    }

    /// Borrows u32 checkpoints followed by u16 lengths; count comes from the group.
    ///
    /// One checkpoint precedes each block of 64 units. Opening validates every
    /// checkpoint and the total byte length; random access sums at most 63
    /// preceding lengths. Sequential iteration reads each length only once.
    /// Checkpoints are aligned physical starts; lengths exclude all padding.
    pub fn lengths16(
        count: u32,
        bytes: &'a [u8],
        alignment: ByteAlignment,
    ) -> Result<Self, UnitIndexError> {
        Self::lengths(count, bytes, alignment, LengthWidth::U16)
    }

    /// Borrows u32 checkpoints and u32 lengths with the same 64-unit access bound.
    pub fn lengths32(
        count: u32,
        bytes: &'a [u8],
        alignment: ByteAlignment,
    ) -> Result<Self, UnitIndexError> {
        Self::lengths(count, bytes, alignment, LengthWidth::U32)
    }

    fn lengths(
        count: u32,
        bytes: &'a [u8],
        alignment: ByteAlignment,
        width: LengthWidth,
    ) -> Result<Self, UnitIndexError> {
        let count = usize::try_from(count).map_err(|_| UnitIndexError::SizeOverflow)?;
        let encoding = match width {
            LengthWidth::U16 => UnitIndexEncoding::Lengths16,
            LengthWidth::U32 => UnitIndexEncoding::Lengths32,
        };
        let needed = encoding.table_len(count)?;
        if bytes.len() != needed {
            return Err(UnitIndexError::LengthMismatch {
                expected: needed,
                actual: bytes.len(),
            });
        }
        let checkpoint_count = count.div_ceil(UNIT_CHECKPOINT_INTERVAL);
        let (checkpoints, lengths) = bytes.split_at(checkpoint_count * 4);
        let mut byte_len = 0u32;
        for index in 0..count {
            let start = Self::aligned(byte_len, alignment)?;
            if index % UNIT_CHECKPOINT_INTERVAL == 0 {
                let actual = read_u32_le(checkpoints, index / UNIT_CHECKPOINT_INTERVAL * 4)
                    .expect("complete checkpoint");
                if actual != start {
                    return Err(UnitIndexError::CheckpointMismatch {
                        index: index as u32,
                        expected: start,
                        actual,
                    });
                }
            }
            byte_len = start
                .checked_add(width.read(lengths, index))
                .ok_or(UnitIndexError::SizeOverflow)?;
        }
        Ok(Self {
            storage: Storage::Lengths {
                checkpoints,
                lengths,
                width,
            },
            count,
            byte_len,
            alignment,
        })
    }

    pub const fn len(self) -> usize {
        self.count
    }
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }
    pub const fn byte_len(self) -> u32 {
        self.byte_len
    }

    /// Resolves one ordinal into a DATA-relative range with bounded work.
    pub fn get(self, index: usize) -> Option<Range<u32>> {
        if index >= self.count {
            return None;
        }
        let start = match self.storage {
            Storage::Fixed { step, .. } => index as u32 * step,
            Storage::Ranges(ranges) => ranges.get(index)?.start,
            Storage::Offsets(bytes) => read_u32_le(bytes, index * 4)?,
            Storage::Lengths {
                checkpoints,
                lengths,
                width,
            } => {
                let block = index / UNIT_CHECKPOINT_INTERVAL;
                let mut offset = read_u32_le(checkpoints, block * 4)?;
                for preceding in block * UNIT_CHECKPOINT_INTERVAL..index {
                    offset = Self::aligned(offset + width.read(lengths, preceding), self.alignment)
                        .expect("validated unit start");
                }
                offset
            }
        };
        Some(start..start + self.unit_len(index))
    }

    pub fn iter(self) -> UnitRanges<'a> {
        UnitRanges {
            index: self,
            front: 0,
            back: self.count,
            front_offset: 0,
            back_offset: self.byte_len,
        }
    }

    fn unit_len(self, index: usize) -> u32 {
        match self.storage {
            Storage::Fixed { size, .. } => size,
            Storage::Ranges(ranges) => {
                let range = &ranges[index];
                range.end - range.start
            }
            Storage::Offsets(bytes) => {
                read_u32_le(bytes, (index + 1) * 4).unwrap()
                    - read_u32_le(bytes, index * 4).unwrap()
            }
            Storage::Lengths { lengths, width, .. } => width.read(lengths, index),
        }
    }

    pub(crate) fn aligned(offset: u32, alignment: ByteAlignment) -> Result<u32, UnitIndexError> {
        let alignment = alignment.get();
        offset
            .checked_add(offset.wrapping_neg() & (alignment - 1))
            .ok_or(UnitIndexError::SizeOverflow)
    }
}

/// Exact-size, double-ended iteration with O(1) work per adjacent range.
#[derive(Clone, Debug)]
pub struct UnitRanges<'a> {
    index: UnitIndex<'a>,
    front: usize,
    back: usize,
    front_offset: u32,
    back_offset: u32,
}

impl Iterator for UnitRanges<'_> {
    type Item = Range<u32>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let start = self.front_offset;
        let end = start + self.index.unit_len(self.front);
        self.front += 1;
        self.front_offset = if self.front < self.index.count {
            UnitIndex::aligned(end, self.index.alignment).expect("validated unit start")
        } else {
            end
        };
        Some(start..end)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.front_offset = self.index.get(self.front)?.start;
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.len(), Some(self.len()))
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for UnitRanges<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        let len = self.index.unit_len(self.back);
        let step = if self.back + 1 == self.index.count {
            len
        } else {
            UnitIndex::aligned(len, self.index.alignment).expect("validated unit step")
        };
        self.back_offset -= step;
        Some(self.back_offset..self.back_offset + len)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n + 1;
        let range = self.index.get(self.back)?;
        self.back_offset = range.start;
        Some(range)
    }
}

impl ExactSizeIterator for UnitRanges<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for UnitRanges<'_> {}

/// Explicit wire representation for a variable-size unit index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnitIndexEncoding {
    /// Adjacent u32 offsets, including the final end; no inter-unit gaps.
    Offsets,
    /// A physical-start checkpoint per 64 units, followed by u16 coded lengths.
    Lengths16,
    /// A physical-start checkpoint per 64 units, followed by u32 coded lengths.
    Lengths32,
}

impl UnitIndexEncoding {
    pub(crate) fn table_len(self, count: usize) -> Result<usize, UnitIndexError> {
        let size = match self {
            Self::Offsets => count.checked_add(1).and_then(|count| count.checked_mul(4)),
            Self::Lengths16 | Self::Lengths32 => count
                .div_ceil(UNIT_CHECKPOINT_INTERVAL)
                .checked_mul(4)
                .and_then(|checkpoints| {
                    count
                        .checked_mul(if self == Self::Lengths16 { 2 } else { 4 })
                        .and_then(|lengths| lengths.checked_add(checkpoints))
                }),
        }
        .ok_or(UnitIndexError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| UnitIndexError::SizeOverflow)?;
        Ok(size)
    }

    pub fn encoded_len(
        self,
        lengths: &[u32],
        alignment: ByteAlignment,
    ) -> Result<usize, UnitIndexError> {
        let needed = self.table_len(lengths.len())?;
        let mut total = 0u32;
        for (index, &length) in lengths.iter().enumerate() {
            if self == Self::Lengths16 && length > u32::from(u16::MAX) {
                return Err(UnitIndexError::LengthTooLarge {
                    index: index as u32,
                    bytes: length,
                });
            }
            if self == Self::Offsets {
                if total % alignment.get() != 0 {
                    return Err(UnitIndexError::UnalignedOffset {
                        index: index as u32,
                        offset: total,
                        alignment,
                    });
                }
            } else {
                total = UnitIndex::aligned(total, alignment)?;
            }
            total = total
                .checked_add(length)
                .ok_or(UnitIndexError::SizeOverflow)?;
        }
        Ok(needed)
    }

    /// Encodes caller-supplied lengths without allocation or silent format changes.
    /// Errors preserve the entire output; success preserves its unused suffix.
    pub fn encode_into(
        self,
        lengths: &[u32],
        alignment: ByteAlignment,
        out: &mut [u8],
    ) -> Result<usize, UnitIndexError> {
        let needed = self.encoded_len(lengths, alignment)?;
        if out.len() < needed {
            return Err(UnitIndexError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        let mut offset = 0u32;
        match self {
            Self::Offsets => {
                write_u32_le(out, 0, 0);
                for (index, &length) in lengths.iter().enumerate() {
                    offset += length;
                    write_u32_le(out, (index + 1) * 4, offset);
                }
            }
            Self::Lengths16 | Self::Lengths32 => {
                let width = if self == Self::Lengths16 {
                    LengthWidth::U16
                } else {
                    LengthWidth::U32
                };
                let lengths_start = lengths.len().div_ceil(UNIT_CHECKPOINT_INTERVAL) * 4;
                for (index, &length) in lengths.iter().enumerate() {
                    offset = UnitIndex::aligned(offset, alignment).expect("validated unit start");
                    if index % UNIT_CHECKPOINT_INTERVAL == 0 {
                        write_u32_le(out, index / UNIT_CHECKPOINT_INTERVAL * 4, offset);
                    }
                    let position = lengths_start + index * width.bytes();
                    match width {
                        LengthWidth::U16 => {
                            write_u16_le(out, position, length as u16);
                        }
                        LengthWidth::U32 => {
                            write_u32_le(out, position, length);
                        }
                    }
                    offset += length;
                }
            }
        }
        Ok(needed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UnitIndexError {
    InvalidRange {
        index: u32,
        start: u32,
        end: u32,
    },
    RangeStartMismatch {
        index: u32,
        expected: u32,
        actual: u32,
    },
    UnalignedOffset {
        index: u32,
        offset: u32,
        alignment: ByteAlignment,
    },
    InvalidOffsetTableLength(usize),
    FirstOffsetNonZero(u32),
    OffsetsOutOfOrder {
        index: u32,
        previous: u32,
        next: u32,
    },
    CheckpointMismatch {
        index: u32,
        expected: u32,
        actual: u32,
    },
    LengthMismatch {
        expected: usize,
        actual: usize,
    },
    LengthTooLarge {
        index: u32,
        bytes: u32,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn independent_offsets_and_checkpoints_resolve_identical_ranges() {
        let offsets = [0, 0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 5, 0, 0, 0];
        let checkpointed = [0, 0, 0, 0, 2, 0, 0, 0, 3, 0];
        let expected = [0..2, 2..2, 2..5];
        for index in [
            UnitIndex::ranges(&expected, alignment(1)).unwrap(),
            UnitIndex::offsets(&offsets).unwrap(),
            UnitIndex::lengths16(3, &checkpointed, alignment(1)).unwrap(),
        ] {
            assert_eq!(index.len(), 3);
            assert_eq!(index.byte_len(), 5);
            assert!(index.iter().eq(expected.clone()));
            assert!(index.iter().rev().eq(expected.clone().into_iter().rev()));
            for (ordinal, range) in expected.iter().enumerate() {
                assert_eq!(index.get(ordinal), Some(range.clone()));
            }
            assert_eq!(index.get(usize::MAX), None);
            assert_eq!(index.get(3), None);
        }
    }

    #[test]
    fn checkpoint_boundaries_and_mixed_iterator_skips_preserve_coverage() {
        for count in [0, 1, 63, 64, 65, 127, 128, 129, 4097] {
            let lengths: Vec<_> = (0..count).map(|index| index * 101 % 65536).collect();
            let mut expected_offset = 0;
            let expected: Vec<_> = lengths
                .iter()
                .map(|length| {
                    let range = expected_offset..expected_offset + length;
                    expected_offset += length;
                    range
                })
                .collect();
            for encoding in [
                UnitIndexEncoding::Offsets,
                UnitIndexEncoding::Lengths16,
                UnitIndexEncoding::Lengths32,
            ] {
                let mut bytes =
                    vec![0xa5; encoding.encoded_len(&lengths, alignment(1)).unwrap() + 3];
                let len = encoding
                    .encode_into(&lengths, alignment(1), &mut bytes[1..])
                    .unwrap();
                assert_eq!(bytes[0], 0xa5);
                assert_eq!(&bytes[1 + len..], &[0xa5; 2]);
                let index = match encoding {
                    UnitIndexEncoding::Offsets => UnitIndex::offsets(&bytes[1..1 + len]),
                    UnitIndexEncoding::Lengths16 => {
                        UnitIndex::lengths16(count, &bytes[1..1 + len], alignment(1))
                    }
                    UnitIndexEncoding::Lengths32 => {
                        UnitIndex::lengths32(count, &bytes[1..1 + len], alignment(1))
                    }
                }
                .unwrap();
                assert_eq!(index.byte_len(), expected_offset);
                assert!(index.iter().eq(expected.iter().cloned()));
                for (ordinal, range) in expected.iter().enumerate() {
                    assert_eq!(index.get(ordinal), Some(range.clone()));
                }
                let mut ranges = index.iter();
                let mut reference = expected.iter().cloned();
                assert_eq!(ranges.next(), reference.next());
                assert_eq!(ranges.next_back(), reference.next_back());
                assert_eq!(ranges.nth(62), reference.nth(62));
                assert_eq!(ranges.nth_back(62), reference.nth_back(62));
                assert_eq!(ranges.len(), reference.len());
                assert!(ranges.eq(reference));
                let mut exhausted = index.iter();
                assert_eq!(exhausted.nth(usize::MAX), None);
                assert_eq!(exhausted.next_back(), None);
                assert_eq!(exhausted.len(), 0);
            }
        }
    }

    #[test]
    fn fixed_ranges_need_no_table_and_check_arithmetic() {
        let index = UnitIndex::fixed(3, 4, alignment(1)).unwrap();
        assert!(index.iter().eq([0..4, 4..8, 8..12]));
        assert_eq!(index.byte_len(), 12);
        assert_eq!(
            UnitIndex::fixed(u32::MAX, 2, alignment(1)),
            Err(UnitIndexError::SizeOverflow)
        );
        let large = UnitIndex::fixed(u32::MAX, 1, alignment(1)).unwrap();
        assert_eq!(large.iter().count(), u32::MAX as usize);
        assert_eq!(large.iter().last(), Some(u32::MAX - 1..u32::MAX));
        assert_eq!(
            large.iter().nth(u32::MAX as usize - 1),
            Some(u32::MAX - 1..u32::MAX)
        );
        assert_eq!(large.iter().nth_back(u32::MAX as usize - 1), Some(0..1));
        assert!(
            UnitIndex::fixed(0, u32::MAX, alignment(1))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn malformed_offsets_and_checkpoint_sums_are_rejected() {
        assert!(matches!(
            UnitIndex::ranges(&[0..2, 1..3], alignment(1)),
            Err(UnitIndexError::RangeStartMismatch { .. })
        ));
        assert!(matches!(
            UnitIndex::ranges(&[core::ops::Range { start: 2, end: 1 }], alignment(1)),
            Err(UnitIndexError::InvalidRange { .. })
        ));
        assert!(matches!(
            UnitIndex::ranges(&[0..3, 64..65], alignment(1)),
            Err(UnitIndexError::RangeStartMismatch { .. })
        ));
        assert_eq!(
            UnitIndex::ranges(&[0..3, 64..65], alignment(64))
                .unwrap()
                .byte_len(),
            65
        );
        for len in [0, 1, 2, 3, 5, 6, 7] {
            assert!(UnitIndex::offsets(&[0; 7][..len]).is_err());
        }
        assert_eq!(
            UnitIndex::offsets(&[1, 0, 0, 0]),
            Err(UnitIndexError::FirstOffsetNonZero(1))
        );
        assert!(matches!(
            UnitIndex::offsets(&[0, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0]),
            Err(UnitIndexError::OffsetsOutOfOrder { .. })
        ));
        let lengths = [1; 65];
        let mut bytes = [0; 138];
        UnitIndexEncoding::Lengths16
            .encode_into(&lengths, alignment(1), &mut bytes)
            .unwrap();
        for end in 0..bytes.len() {
            assert!(UnitIndex::lengths16(65, &bytes[..end], alignment(1)).is_err());
        }
        for checkpoint in [0, 4] {
            bytes[checkpoint] ^= 1;
            assert!(matches!(
                UnitIndex::lengths16(65, &bytes, alignment(1)),
                Err(UnitIndexError::CheckpointMismatch { .. })
            ));
            bytes[checkpoint] ^= 1;
        }
        assert!(UnitIndex::lengths16(64, &bytes, alignment(1)).is_err());
        assert!(UnitIndex::lengths16(u32::MAX, &[], alignment(1)).is_err());
    }

    #[test]
    fn encoder_rejects_expansion_overflow_and_short_buffers_before_writing() {
        let mut out = [0xa5; 32];
        assert!(matches!(
            UnitIndexEncoding::Lengths16.encode_into(&[65536], alignment(1), &mut out),
            Err(UnitIndexError::LengthTooLarge { .. })
        ));
        assert_eq!(out, [0xa5; 32]);
        for encoding in [UnitIndexEncoding::Offsets, UnitIndexEncoding::Lengths16] {
            assert!(
                encoding
                    .encode_into(&[u32::MAX, 1], alignment(1), &mut out)
                    .is_err()
            );
            assert!(matches!(
                encoding.encode_into(&[1; 65], alignment(1), &mut out),
                Err(UnitIndexError::BufferTooSmall { .. })
            ));
            assert_eq!(out, [0xa5; 32]);
        }
    }
}
