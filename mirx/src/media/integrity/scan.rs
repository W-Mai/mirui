use core::ops::Range;

use super::{INTEGRITY_RECORD_LEN, IntegrityError, IntegrityRange};
use crate::{crc32::Crc32, media::MediaPayloadError};

/// Ordered integrity-record validation using one record window at a time.
///
/// Record offsets are relative to the INTEGRITY section. Accepted ranges use payload-relative offsets. The scan checks ordering and arithmetic, not DATA coverage or checksums. Input bytes are never retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegrityScan {
    len: u32,
    next: u32,
    previous_end: u32,
}

impl IntegrityScan {
    pub fn new(byte_len: usize) -> Result<Self, IntegrityError> {
        let len = u32::try_from(byte_len).map_err(|_| IntegrityError::SizeOverflow)?;
        if byte_len % INTEGRITY_RECORD_LEN != 0 {
            return Err(IntegrityError::Truncated);
        }
        Ok(Self {
            len,
            next: 0,
            previous_end: 0,
        })
    }

    pub fn record_range(&self) -> Option<Range<usize>> {
        (self.next < self.len)
            .then(|| self.next as usize..self.next as usize + INTEGRITY_RECORD_LEN)
    }

    /// Accept a complete record. Invalid input leaves the current position unchanged; `None` means every declared record was accepted.
    pub fn accept(&mut self, record: &[u8]) -> Result<Option<IntegrityRange>, IntegrityError> {
        if self.record_range().is_none() {
            return Ok(None);
        }
        let range = IntegrityRange::read(record)?;
        if range.offset < self.previous_end {
            return Err(IntegrityError::RangesOverlapOrReversed {
                index: self.next / INTEGRITY_RECORD_LEN as u32,
            });
        }
        self.previous_end = range.range().end;
        self.next += INTEGRITY_RECORD_LEN as u32;
        Ok(Some(range))
    }
}

/// Incremental verification of one DATA partition with constant storage.
///
/// Supply nonempty prefixes of `read_range()` in order and call `finish()` after completion. No source access or allocation occurs inside this type. The source must remain the same immutable version for the entire check.
pub struct RangeCheck {
    range: IntegrityRange,
    next: u32,
    crc: Crc32,
}

impl RangeCheck {
    pub(super) fn new(range: IntegrityRange) -> Self {
        Self {
            next: range.offset(),
            range,
            crc: Crc32::new(),
        }
    }

    pub fn read_range(&self) -> Option<Range<u32>> {
        let end = self.range.range().end;
        (self.next < end).then_some(self.next..end)
    }

    /// Reject empty, oversized and post-completion input without changing checksum state.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), MediaPayloadError> {
        let remaining = self.range.range().end - self.next;
        let len = u32::try_from(bytes.len()).map_err(|_| MediaPayloadError::InvalidDataRange)?;
        if len == 0 || len > remaining {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        self.crc.update(bytes);
        self.next += len;
        Ok(())
    }

    pub fn finish(self) -> Result<(), MediaPayloadError> {
        if self.read_range().is_some() {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        let expected = self.range.checksum();
        let actual = self.crc.finish();
        if expected != actual {
            return Err(MediaPayloadError::RangeCrcMismatch {
                offset: self.range.offset(),
                expected,
                actual,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::integrity::IntegrityTable;

    fn scan_records(bytes: &[u8]) -> Result<(), IntegrityError> {
        let mut scan = IntegrityScan::new(bytes.len())?;
        while let Some(range) = scan.record_range() {
            scan.accept(&bytes[range])?;
        }
        Ok(())
    }

    #[test]
    fn record_windows_match_borrowed_validation_and_retry_without_advancing() {
        let ranges = [
            IntegrityRange::new(80..84, 1).unwrap(),
            IntegrityRange::new(84..90, 2).unwrap(),
            IntegrityRange::new(100..104, 3).unwrap(),
        ];
        let mut bytes = [0; 36];
        IntegrityTable::encode_into(&ranges, &mut bytes).unwrap();
        for offset in 0..bytes.len() {
            for value in [0, 1, 0x7f, 0xff] {
                let mut changed = bytes;
                changed[offset] = value;
                assert_eq!(
                    scan_records(&changed),
                    IntegrityTable::open(&changed).map(|_| ())
                );
            }
        }
        let mut scan = IntegrityScan::new(bytes.len()).unwrap();
        for expected in ranges {
            let position = scan.record_range().unwrap();
            let before = scan;
            assert_eq!(scan.accept(&[]), Err(IntegrityError::Truncated));
            assert_eq!(scan, before);
            let invalid = IntegrityRange::new(1..2, 0).unwrap().encode_record();
            if position.start != 0 {
                assert!(scan.accept(&invalid).is_err());
                assert_eq!(scan, before);
            }
            assert_eq!(scan.accept(&bytes[position]), Ok(Some(expected)));
        }
        assert_eq!(scan.record_range(), None);
        assert_eq!(scan.accept(&[]), Ok(None));
        assert_eq!(IntegrityScan::new(0).unwrap().record_range(), None);
        for size in 1..36 {
            if size % 12 != 0 {
                assert_eq!(IntegrityScan::new(size), Err(IntegrityError::Truncated));
            }
        }
        if usize::BITS > 32 {
            assert_eq!(
                IntegrityScan::new(usize::MAX),
                Err(IntegrityError::SizeOverflow)
            );
        }
    }

    #[test]
    fn partition_checks_preserve_crc_across_short_windows_and_rejected_input() {
        let bytes: [u8; 257] = core::array::from_fn(|index| index as u8);
        let start = u32::MAX - bytes.len() as u32;
        let partition =
            IntegrityRange::new(start..u32::MAX, crate::crc32::compute(&bytes)).unwrap();
        for size in [1, 2, 3, 12, 128, 256, 257] {
            let mut check = partition.check();
            assert!(check.finish().is_err());
            check = partition.check();
            while let Some(range) = check.read_range() {
                let first = (range.start - start) as usize;
                let len = (bytes.len() - first).min(size);
                assert_eq!(check.accept(&[]), Err(MediaPayloadError::InvalidDataRange));
                assert_eq!(check.read_range(), Some(range));
                check.accept(&bytes[first..first + len]).unwrap();
            }
            assert_eq!(check.accept(&[0]), Err(MediaPayloadError::InvalidDataRange));
            check.finish().unwrap();
        }
        let mut check = partition.check();
        check.accept(&bytes[..256]).unwrap();
        assert_eq!(
            check.accept(&[0; 2]),
            Err(MediaPayloadError::InvalidDataRange)
        );
        assert_eq!(check.read_range(), Some(u32::MAX - 1..u32::MAX));
        check.accept(&bytes[256..]).unwrap();
        check.finish().unwrap();
        for index in 0..bytes.len() {
            let mut changed = bytes;
            changed[index] ^= 1;
            let mut check = partition.check();
            check.accept(&changed).unwrap();
            assert!(
                matches!(check.finish(), Err(MediaPayloadError::RangeCrcMismatch { offset, .. }) if offset == start)
            );
        }
    }
}
