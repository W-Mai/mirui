use core::ops::Range;

use super::{
    MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaLayout, MediaPayloadError,
    MediaSectionDescriptor, MediaSectionKind, SectionScan,
};
use crate::crc32::Crc32;

pub(super) struct MetadataCoverage {
    start: usize,
    len: usize,
}

impl MetadataCoverage {
    pub(super) fn new(len: usize) -> Self {
        Self {
            start: MEDIA_HEADER_LEN,
            len,
        }
    }
    pub(super) fn before_data(&mut self, descriptor: MediaSectionDescriptor) -> Range<usize> {
        let start = self.start;
        let end = descriptor.offset() as usize;
        self.start = end + descriptor.size() as usize;
        start..end
    }
    pub(super) fn tail(&self) -> Range<usize> {
        self.start..self.len
    }
}

#[derive(Clone, Copy)]
enum Phase {
    Directory,
    LocateData,
    Bytes {
        start: usize,
        end: usize,
        terminal: bool,
    },
    Done,
}

/// Constant-storage validation of metadata checksum coverage using source windows.
///
/// Read the range returned by [`Self::read_range`] and pass any nonempty prefix to [`Self::accept`]. No input borrow is retained. Directory records are visited twice, so reads can move backwards. DATA bytes are excluded; all other covered bytes, including padding and the whole-DATA checksum trailer, are included. Finishing this scan does not validate DATA or typed payload semantics.
pub struct MetadataScan {
    scan: SectionScan,
    coverage: MetadataCoverage,
    crc: Crc32,
    record: [u8; MEDIA_SECTION_LEN],
    filled: usize,
    phase: Phase,
}

impl MetadataScan {
    pub(super) fn new(layout: MediaLayout) -> Self {
        let header = layout.header();
        let count = header.section_count().to_le_bytes();
        let mut crc = Crc32::new();
        crc.update(&[MEDIA_VERSION, header.flags().bits(), count[0], count[1]]);
        let mut coverage = MetadataCoverage::new(layout.payload_len());
        coverage.start = layout.directory_range().end;
        let mut state = Self {
            scan: layout.sections(),
            coverage,
            crc,
            record: [0; MEDIA_SECTION_LEN],
            filled: 0,
            phase: Phase::Directory,
        };
        if state.scan.record_range().is_none() {
            state.locate_or_tail();
        }
        state
    }

    pub fn read_range(&self) -> Option<Range<usize>> {
        match self.phase {
            Phase::Directory | Phase::LocateData => {
                let range = self.scan.record_range()?;
                Some(range.start + self.filled..range.end)
            }
            Phase::Bytes { start, end, .. } => Some(start..end),
            Phase::Done => None,
        }
    }

    /// Accept a prefix of the requested range. Empty, oversized or post-completion input is rejected without changing state. Each call performs work bounded by the supplied byte count.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), MediaPayloadError> {
        let range = self
            .read_range()
            .ok_or(MediaPayloadError::InvalidDataRange)?;
        if bytes.is_empty() || bytes.len() > range.len() {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        match self.phase {
            Phase::Directory | Phase::LocateData => {
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
                match self.phase {
                    Phase::Directory => {
                        self.crc.update(&record);
                        if self.scan.record_range().is_none() {
                            self.scan = self.scan.layout().sections();
                            self.locate_or_tail();
                        }
                    }
                    Phase::LocateData => {
                        if descriptor.kind() == MediaSectionKind::DATA {
                            let range = self.coverage.before_data(descriptor);
                            self.body(range, false);
                        } else {
                            self.locate_or_tail();
                        }
                    }
                    _ => unreachable!(),
                }
            }
            Phase::Bytes {
                start,
                end,
                terminal,
            } => {
                self.crc.update(bytes);
                let start = start + bytes.len();
                if start == end {
                    if terminal {
                        self.phase = Phase::Done;
                    } else {
                        self.locate_or_tail();
                    }
                } else {
                    self.phase = Phase::Bytes {
                        start,
                        end,
                        terminal,
                    };
                }
            }
            Phase::Done => unreachable!(),
        }
        Ok(())
    }

    /// Reject incomplete coverage or a mismatched checksum. No source is read by this call.
    pub fn finish(self) -> Result<(), MediaPayloadError> {
        if !matches!(self.phase, Phase::Done) {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        let expected = self.scan.layout().header().metadata_crc32;
        let actual = self.crc.finish();
        if actual != expected {
            return Err(MediaPayloadError::MetadataCrcMismatch { expected, actual });
        }
        Ok(())
    }

    fn body(&mut self, range: Range<usize>, terminal: bool) {
        if range.is_empty() {
            if terminal {
                self.phase = Phase::Done;
            } else {
                self.locate_or_tail();
            }
        } else {
            self.phase = Phase::Bytes {
                start: range.start,
                end: range.end,
                terminal,
            };
        }
    }

    fn locate_or_tail(&mut self) {
        if self.scan.record_range().is_some() {
            self.phase = Phase::LocateData;
        } else {
            self.body(self.coverage.tail(), true);
        }
    }
}
