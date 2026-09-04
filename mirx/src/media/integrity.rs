use core::{iter::FusedIterator, ops::Range};

use super::{MediaPayload, MediaSectionKind};
use crate::wire::{read_u32_le, write_u32_le};

mod check;
pub use check::DataCheckPlan;

pub const INTEGRITY_RECORD_LEN: usize = 12;

/// DATA checksum placement for canonical authoring.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DataIntegrity<'a> {
    /// One CRC32 trailer covering all DATA bytes.
    #[default]
    Whole,
    /// Cumulative DATA-relative partition ends, beginning implicitly at zero.
    /// Ends strictly increase and exactly cover DATA, including alignment gaps.
    /// Empty DATA requires an empty list. Checksums are computed by the writer.
    Indexed(&'a [u32]),
}

impl<'a> DataIntegrity<'a> {
    pub const fn partitions(self) -> Option<&'a [u32]> {
        match self {
            Self::Whole => None,
            Self::Indexed(ends) => Some(ends),
        }
    }

    pub(crate) const fn trailer_len(self) -> usize {
        match self {
            Self::Whole => super::MEDIA_CRC_LEN,
            Self::Indexed(_) => 0,
        }
    }

    pub(crate) fn section_len(self, data_len: usize) -> Result<usize, IntegrityError> {
        let data_len = u32::try_from(data_len).map_err(|_| IntegrityError::SizeOverflow)?;
        let Self::Indexed(ends) = self else {
            return Ok(0);
        };
        let size = ends
            .len()
            .checked_mul(INTEGRITY_RECORD_LEN)
            .ok_or(IntegrityError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| IntegrityError::SizeOverflow)?;
        let mut start = 0;
        for (index, &end) in ends.iter().enumerate() {
            if end <= start {
                return Err(IntegrityError::RangesOverlapOrReversed {
                    index: index as u32,
                });
            }
            if end > data_len {
                return Err(IntegrityError::InvalidCoverage { offset: end });
            }
            start = end;
        }
        if start != data_len {
            return Err(IntegrityError::IncompleteCoverage { offset: start });
        }
        Ok(size)
    }
}

/// One positive, payload-relative DATA range and its CRC32/IEEE checksum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrityRange {
    offset: u32,
    size: u32,
    checksum: u32,
}

impl IntegrityRange {
    pub fn new(range: Range<u32>, checksum: u32) -> Result<Self, IntegrityError> {
        if range.start >= range.end {
            return Err(IntegrityError::EmptyOrReversedRange);
        }
        Ok(Self {
            offset: range.start,
            size: range.end - range.start,
            checksum,
        })
    }

    pub const fn offset(self) -> u32 {
        self.offset
    }
    pub const fn size(self) -> u32 {
        self.size
    }
    pub const fn checksum(self) -> u32 {
        self.checksum
    }
    pub fn range(self) -> Range<u32> {
        self.offset..self.offset + self.size
    }

    fn read(bytes: &[u8]) -> Result<Self, IntegrityError> {
        let offset = read_u32_le(bytes, 0).ok_or(IntegrityError::Truncated)?;
        let size = read_u32_le(bytes, 4).ok_or(IntegrityError::Truncated)?;
        let end = offset
            .checked_add(size)
            .ok_or(IntegrityError::SizeOverflow)?;
        Self::new(
            offset..end,
            read_u32_le(bytes, 8).ok_or(IntegrityError::Truncated)?,
        )
    }

    pub(crate) fn encode_record(self) -> [u8; INTEGRITY_RECORD_LEN] {
        let mut bytes = [0; INTEGRITY_RECORD_LEN];
        write_u32_le(&mut bytes, 0, self.offset);
        write_u32_le(&mut bytes, 4, self.size);
        write_u32_le(&mut bytes, 8, self.checksum);
        bytes
    }
}

/// Borrowed ordered checksum ranges without overlap or hidden allocation.
///
/// `open` validates records only. A media payload additionally requires these
/// ranges to partition its DATA bodies exactly before exposing the table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrityTable<'a> {
    bytes: &'a [u8],
}

impl<'a> IntegrityTable<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, IntegrityError> {
        u32::try_from(bytes.len()).map_err(|_| IntegrityError::SizeOverflow)?;
        if bytes.len() % INTEGRITY_RECORD_LEN != 0 {
            return Err(IntegrityError::Truncated);
        }
        let mut previous_end = 0;
        for (index, record) in bytes.chunks_exact(INTEGRITY_RECORD_LEN).enumerate() {
            let range = IntegrityRange::read(record)?;
            if range.offset < previous_end {
                return Err(IntegrityError::RangesOverlapOrReversed {
                    index: index as u32,
                });
            }
            previous_end = range.range().end;
        }
        Ok(Self { bytes })
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / INTEGRITY_RECORD_LEN
    }
    pub const fn is_empty(self) -> bool {
        self.bytes.is_empty()
    }
    pub fn get(self, index: usize) -> Option<IntegrityRange> {
        if index >= self.len() {
            return None;
        }
        IntegrityRange::read(&self.bytes[index * INTEGRITY_RECORD_LEN..][..INTEGRITY_RECORD_LEN])
            .ok()
    }

    pub fn iter(self) -> IntegrityRanges<'a> {
        IntegrityRanges {
            table: self,
            front: 0,
            back: self.len(),
        }
    }

    pub(super) fn intersecting(self, requested: Range<u32>) -> IntegrityRanges<'a> {
        let mut low = 0;
        let mut high = self.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if self.get(middle).unwrap().range().end <= requested.start {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let front = low;
        high = self.len();
        while low < high {
            let middle = low + (high - low) / 2;
            if self.get(middle).unwrap().offset() < requested.end {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        IntegrityRanges {
            table: self,
            front,
            back: low,
        }
    }

    pub fn encoded_len(ranges: &[IntegrityRange]) -> Result<usize, IntegrityError> {
        let size = ranges
            .len()
            .checked_mul(INTEGRITY_RECORD_LEN)
            .ok_or(IntegrityError::SizeOverflow)?;
        u32::try_from(size).map_err(|_| IntegrityError::SizeOverflow)?;
        let mut previous_end = 0;
        for (index, range) in ranges.iter().enumerate() {
            if range.offset < previous_end {
                return Err(IntegrityError::RangesOverlapOrReversed {
                    index: index as u32,
                });
            }
            previous_end = range.range().end;
        }
        Ok(size)
    }

    /// Writes checked records without allocation; errors preserve all output.
    pub fn encode_into(ranges: &[IntegrityRange], out: &mut [u8]) -> Result<usize, IntegrityError> {
        let needed = Self::encoded_len(ranges)?;
        if out.len() < needed {
            return Err(IntegrityError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        for (index, range) in ranges.iter().enumerate() {
            let offset = index * INTEGRITY_RECORD_LEN;
            out[offset..offset + INTEGRITY_RECORD_LEN].copy_from_slice(&range.encode_record());
        }
        Ok(needed)
    }

    pub(super) fn validate_coverage(self, media: MediaPayload<'_>) -> Result<(), IntegrityError> {
        let mut ranges = self.iter();
        for section in media.sections_of_kind(MediaSectionKind::DATA) {
            let mut cursor = section.descriptor().offset();
            let end = cursor + section.descriptor().size();
            while cursor < end {
                let range = ranges
                    .next()
                    .ok_or(IntegrityError::IncompleteCoverage { offset: cursor })?;
                if range.offset != cursor || range.range().end > end {
                    return Err(IntegrityError::InvalidCoverage {
                        offset: range.offset,
                    });
                }
                cursor = range.range().end;
            }
        }
        if let Some(range) = ranges.next() {
            return Err(IntegrityError::InvalidCoverage {
                offset: range.offset,
            });
        }
        Ok(())
    }
}

/// Exact-size checksum iteration with constant-time ordinal skips.
#[derive(Clone, Debug)]
pub struct IntegrityRanges<'a> {
    table: IntegrityTable<'a>,
    front: usize,
    back: usize,
}

impl Iterator for IntegrityRanges<'_> {
    type Item = IntegrityRange;
    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.table.get(index)
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
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
impl DoubleEndedIterator for IntegrityRanges<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.table.get(self.back)
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.back -= n;
        self.next_back()
    }
}
impl ExactSizeIterator for IntegrityRanges<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}
impl FusedIterator for IntegrityRanges<'_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum IntegrityError {
    Truncated,
    EmptyOrReversedRange,
    RangesOverlapOrReversed { index: u32 },
    InvalidCoverage { offset: u32 },
    IncompleteCoverage { offset: u32 },
    BufferTooSmall { needed: usize, available: usize },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: [u8; 24] = [
        40, 0, 0, 0, 4, 0, 0, 0, 1, 2, 3, 4, 48, 0, 0, 0, 2, 0, 0, 0, 5, 6, 7, 8,
    ];

    #[test]
    fn independent_records_round_trip_and_retain_exact_ranges() {
        let table = IntegrityTable::open(&BYTES).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table.get(0).unwrap().range(), 40..44);
        assert_eq!(table.get(1).unwrap().checksum(), 0x0807_0605);
        assert_eq!(table.get(usize::MAX), None);
        let records = [table.get(0).unwrap(), table.get(1).unwrap()];
        let mut out = [0xa5; 27];
        assert_eq!(IntegrityTable::encode_into(&records, &mut out[1..]), Ok(24));
        assert_eq!(&out[1..25], BYTES);
        assert_eq!(out[0], 0xa5);
        assert_eq!(&out[25..], &[0xa5; 2]);
        assert!(table.iter().eq(records));
        assert!(table.iter().rev().eq(records.into_iter().rev()));
        assert_eq!(table.iter().nth(1), table.get(1));
        assert_eq!(table.iter().nth_back(1), table.get(0));
        assert_eq!(table.iter().nth(usize::MAX), None);
    }

    #[test]
    fn invalid_ranges_and_encoder_failures_are_bounded_and_atomic() {
        for end in 1..BYTES.len() {
            if end % 12 != 0 {
                assert!(IntegrityTable::open(&BYTES[..end]).is_err());
            }
        }
        assert!(IntegrityRange::new(4..4, 0).is_err());
        assert!(IntegrityRange::new(Range { start: 5, end: 4 }, 0).is_err());
        let mut corrupt = BYTES;
        write_u32_le(&mut corrupt, 12, 43);
        assert!(matches!(
            IntegrityTable::open(&corrupt),
            Err(IntegrityError::RangesOverlapOrReversed { .. })
        ));
        write_u32_le(&mut corrupt, 12, u32::MAX);
        assert_eq!(
            IntegrityTable::open(&corrupt),
            Err(IntegrityError::SizeOverflow)
        );
        let records = [
            IntegrityRange::new(10..20, 0).unwrap(),
            IntegrityRange::new(15..25, 0).unwrap(),
        ];
        let mut out = [0xa5; 24];
        assert!(IntegrityTable::encode_into(&records, &mut out).is_err());
        assert_eq!(out, [0xa5; 24]);
        assert!(IntegrityTable::encode_into(&records[..1], &mut out[..11]).is_err());
        assert_eq!(out, [0xa5; 24]);
    }
}
