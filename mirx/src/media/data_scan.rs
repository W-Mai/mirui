use core::ops::Range;

use super::{MEDIA_SECTION_LEN, MediaLayout, MediaPayloadError, MediaSectionKind, SectionScan};
use crate::crc32::Crc32;

#[derive(Clone, Copy)]
enum Phase {
    Directory,
    Data { start: usize, end: usize },
    Trailer,
    Done,
}

/// Constant-storage verification of the whole-DATA checksum through source windows.
///
/// Ranges use payload-relative offsets and may move backwards between DATA bodies and directory records. Supply nonempty prefixes of each requested range from the same immutable source. Metadata, padding and typed payload semantics are not validated by this scan; metadata and integrity-table association must be validated separately before consuming DATA.
pub struct WholeDataScan {
    scan: SectionScan,
    record: [u8; MEDIA_SECTION_LEN],
    filled: usize,
    phase: Phase,
    crc: Crc32,
    expected: u32,
}

impl WholeDataScan {
    pub(super) fn new(layout: MediaLayout) -> Option<Self> {
        layout.checksum_trailer()?;
        let mut state = Self {
            scan: layout.sections(),
            record: [0; MEDIA_SECTION_LEN],
            filled: 0,
            phase: Phase::Directory,
            crc: Crc32::new(),
            expected: 0,
        };
        state.advance();
        Some(state)
    }

    pub fn read_range(&self) -> Option<Range<usize>> {
        match self.phase {
            Phase::Directory => {
                let range = self.scan.record_range()?;
                Some(range.start + self.filled..range.end)
            }
            Phase::Data { start, end } => Some(start..end),
            Phase::Trailer => {
                let range = self.scan.layout().checksum_trailer()?;
                Some(range.start + self.filled..range.end)
            }
            Phase::Done => None,
        }
    }

    /// Accept a nonempty prefix of the requested range. Invalid input leaves the current position and checksum unchanged.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), MediaPayloadError> {
        let range = self
            .read_range()
            .ok_or(MediaPayloadError::InvalidDataRange)?;
        if bytes.is_empty() || bytes.len() > range.len() {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        match self.phase {
            Phase::Directory => {
                let mut record = self.record;
                let filled = self.filled + bytes.len();
                record[self.filled..filled].copy_from_slice(bytes);
                if filled < MEDIA_SECTION_LEN {
                    self.record = record;
                    self.filled = filled;
                    return Ok(());
                }
                let descriptor = self
                    .scan
                    .accept(&record)?
                    .expect("pending directory record");
                self.filled = 0;
                if descriptor.kind() == MediaSectionKind::DATA && descriptor.size() != 0 {
                    let start = descriptor.offset() as usize;
                    self.phase = Phase::Data {
                        start,
                        end: start + descriptor.size() as usize,
                    };
                } else {
                    self.advance();
                }
            }
            Phase::Data { start, end } => {
                self.crc.update(bytes);
                let start = start + bytes.len();
                if start == end {
                    self.advance();
                } else {
                    self.phase = Phase::Data { start, end };
                }
            }
            Phase::Trailer => {
                let filled = self.filled + bytes.len();
                self.record[self.filled..filled].copy_from_slice(bytes);
                self.filled = filled;
                if bytes.len() == range.len() {
                    self.expected = u32::from_le_bytes(self.record[..4].try_into().unwrap());
                    self.phase = Phase::Done;
                }
            }
            Phase::Done => unreachable!(),
        }
        Ok(())
    }

    /// Reject an incomplete scan or a mismatched DATA checksum without reading the source.
    pub fn finish(self) -> Result<(), MediaPayloadError> {
        if !matches!(self.phase, Phase::Done) {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        verify_checksum(self.expected, self.crc.finish())
    }

    fn advance(&mut self) {
        self.phase = if self.scan.record_range().is_some() {
            Phase::Directory
        } else {
            Phase::Trailer
        };
    }
}

pub(super) fn verify_checksum(expected: u32, actual: u32) -> Result<(), MediaPayloadError> {
    if actual != expected {
        return Err(MediaPayloadError::DataCrcMismatch { expected, actual });
    }
    Ok(())
}
