use core::ops::Range;

use super::{IntegrityError, IntegrityScan, advance_coverage, check_table};
use crate::media::{
    MEDIA_SECTION_LEN, MediaLayout, MediaPayloadError, MediaSectionDescriptor, MediaSectionKind,
    SectionScan,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Discover,
    Sections,
    Records,
    Done,
}

/// Validates table association and exact DATA partition coverage without reading DATA bytes.
///
/// Read nonempty prefixes of `read_range()` and pass them to `accept`. Ranges are payload-relative and revisit directory records. The source must provide one immutable version. Successful completion does not establish metadata or DATA checksum validity; those checks remain separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaIntegrityScan {
    sections: SectionScan,
    table: Option<MediaSectionDescriptor>,
    duplicate: bool,
    records: Option<IntegrityScan>,
    cursor: u32,
    end: u32,
    record: [u8; MEDIA_SECTION_LEN],
    filled: usize,
    phase: Phase,
}

impl MediaIntegrityScan {
    pub(crate) fn new(layout: MediaLayout) -> Result<Self, MediaPayloadError> {
        let mut scan = Self {
            sections: layout.sections(),
            table: None,
            duplicate: false,
            records: None,
            cursor: 0,
            end: 0,
            record: [0; MEDIA_SECTION_LEN],
            filled: 0,
            phase: Phase::Discover,
        };
        if scan.sections.record_range().is_none() {
            scan.discovered()?;
        }
        Ok(scan)
    }

    pub fn read_range(&self) -> Option<Range<usize>> {
        let mut range = match self.phase {
            Phase::Discover | Phase::Sections => self.sections.record_range()?,
            Phase::Records => {
                let range = self.records.as_ref()?.record_range()?;
                let base = self.table?.offset() as usize;
                base + range.start..base + range.end
            }
            Phase::Done => return None,
        };
        range.start += self.filled;
        Some(range)
    }

    /// Invalid windows and rejected records do not change scan state. An incomplete record is retained in fixed storage until its remaining bytes arrive.
    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), MediaPayloadError> {
        let range = self
            .read_range()
            .ok_or(MediaPayloadError::InvalidDataRange)?;
        if bytes.is_empty() || bytes.len() > range.len() {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        let mut next = *self;
        next.record[next.filled..next.filled + bytes.len()].copy_from_slice(bytes);
        next.filled += bytes.len();
        if next.filled == MEDIA_SECTION_LEN {
            next.complete_record()?;
            next.filled = 0;
        }
        *self = next;
        Ok(())
    }

    /// Return the validated INTEGRITY descriptor, or `None` for whole-DATA integrity. Reject incomplete scans.
    pub fn finish(self) -> Result<Option<MediaSectionDescriptor>, MediaPayloadError> {
        if self.phase != Phase::Done {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        Ok(self.table)
    }

    fn complete_record(&mut self) -> Result<(), MediaPayloadError> {
        match self.phase {
            Phase::Discover | Phase::Sections => {
                let section = self
                    .sections
                    .accept(&self.record)?
                    .expect("requested section record");
                if self.phase == Phase::Discover {
                    if section.kind() == MediaSectionKind::INTEGRITY {
                        self.duplicate |= self.table.is_some();
                        self.table.get_or_insert(section);
                    }
                    if self.sections.record_range().is_none() {
                        self.discovered()?;
                    }
                } else if section.kind() == MediaSectionKind::DATA && section.size() != 0 {
                    self.cursor = section.offset();
                    self.end = self.cursor + section.size();
                    self.next_partition()?;
                } else {
                    self.next_section();
                }
            }
            Phase::Records => {
                let range = self
                    .records
                    .as_mut()
                    .unwrap()
                    .accept(&self.record)
                    .map_err(MediaPayloadError::Integrity)?
                    .expect("requested integrity record");
                self.cursor = advance_coverage(self.cursor, self.end, range)
                    .map_err(MediaPayloadError::Integrity)?;
                if self.cursor == self.end {
                    self.next_section();
                } else {
                    self.next_partition()?;
                }
            }
            Phase::Done => unreachable!(),
        }
        Ok(())
    }

    fn discovered(&mut self) -> Result<(), MediaPayloadError> {
        let layout = self.sections.layout();
        check_table(
            layout.header().flags().has_indexed_integrity(),
            self.table,
            self.duplicate,
        )?;
        if let Some(table) = self.table {
            self.records = Some(
                IntegrityScan::new(table.size() as usize).map_err(MediaPayloadError::Integrity)?,
            );
            self.sections = layout.sections();
            self.next_section();
        } else {
            self.phase = Phase::Done;
        }
        Ok(())
    }

    fn next_partition(&mut self) -> Result<(), MediaPayloadError> {
        if self.records.as_ref().unwrap().record_range().is_none() {
            return Err(MediaPayloadError::Integrity(
                IntegrityError::IncompleteCoverage {
                    offset: self.cursor,
                },
            ));
        }
        self.phase = Phase::Records;
        Ok(())
    }

    fn next_section(&mut self) {
        if self.sections.record_range().is_some() {
            self.phase = Phase::Sections;
        } else if self.records.as_ref().unwrap().record_range().is_some() {
            // Any remaining nonempty range lies outside the declared DATA coverage.
            self.cursor = self.end;
            self.phase = Phase::Records;
        } else {
            self.phase = Phase::Done;
        }
    }
}
