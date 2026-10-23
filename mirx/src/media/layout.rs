use core::ops::Range;

use super::{
    MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaFlags, MediaHeader,
    MediaPayloadError, MediaSectionDescriptor,
};

/// Header and checked directory geometry for a sectioned payload.
///
/// Parsing inspects only the fixed header and the supplied payload length. It does not validate metadata checksums, integrity tables or section contents. The payload length must come from the containing resource, not the current read window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaLayout {
    header: MediaHeader,
    directory_end: u32,
    covered_len: u32,
}

impl MediaLayout {
    /// Prepare a whole-DATA checksum scan, or return `None` when this payload uses indexed integrity. Metadata and typed contents require separate validation.
    pub fn whole_data(self) -> Option<super::WholeDataScan> {
        super::WholeDataScan::new(self)
    }

    /// Validate integrity-table association and exact DATA partition coverage through bounded metadata reads. This does not checksum metadata or DATA bytes.
    pub fn integrity(self) -> Result<super::integrity::MediaIntegrityScan, MediaPayloadError> {
        super::integrity::MediaIntegrityScan::new(self)
    }
    pub(super) fn payload_len(self) -> usize {
        self.covered_len as usize
            + if self.header.flags.has_indexed_integrity() {
                0
            } else {
                MEDIA_CRC_LEN
            }
    }

    /// Prepare a bounded metadata-checksum scan. The returned ranges may revisit the directory; the source must support those reads without buffering the complete payload.
    pub fn metadata(self) -> super::MetadataScan {
        super::MetadataScan::new(self)
    }
    pub fn parse(header: &[u8], payload_len: usize) -> Result<Self, MediaPayloadError> {
        if let Some(&version) = header.first() {
            if version != MEDIA_VERSION {
                return Err(MediaPayloadError::UnsupportedVersion(version));
            }
        }
        let trailer_len = if header
            .get(1)
            .is_some_and(|&flags| MediaFlags::from_bits_retain(flags).has_indexed_integrity())
        {
            0
        } else {
            MEDIA_CRC_LEN
        };
        let needed = MEDIA_HEADER_LEN + trailer_len;
        if payload_len < needed {
            return Err(MediaPayloadError::Truncated {
                needed,
                available: payload_len,
            });
        }
        if header.len() < MEDIA_HEADER_LEN {
            return Err(MediaPayloadError::Truncated {
                needed: MEDIA_HEADER_LEN,
                available: header.len(),
            });
        }
        u32::try_from(payload_len).map_err(|_| MediaPayloadError::SizeOverflow)?;
        let header = MediaHeader::read(header);
        let directory_end = usize::from(header.section_count)
            .checked_mul(MEDIA_SECTION_LEN)
            .and_then(|len| MEDIA_HEADER_LEN.checked_add(len))
            .ok_or(MediaPayloadError::SizeOverflow)?;
        let covered_len = payload_len - trailer_len;
        if directory_end > covered_len {
            return Err(MediaPayloadError::SectionTableOutOfBounds);
        }
        Ok(Self {
            header,
            directory_end: directory_end as u32,
            covered_len: covered_len as u32,
        })
    }

    pub const fn header(self) -> MediaHeader {
        self.header
    }
    pub fn directory_range(self) -> Range<usize> {
        MEDIA_HEADER_LEN..self.directory_end as usize
    }
    pub fn checksum_trailer(self) -> Option<Range<usize>> {
        (!self.header.flags.has_indexed_integrity())
            .then_some(self.covered_len as usize..self.covered_len as usize + MEDIA_CRC_LEN)
    }
    pub fn sections(self) -> SectionScan {
        SectionScan {
            layout: self,
            next: 0,
            previous: None,
        }
    }
}

/// Ordered validation of isolated section records, with constant storage.
///
/// Accepting a record advances only after all structural checks succeed. Records are read in directory order; skipping a record cannot establish a validated scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SectionScan {
    layout: MediaLayout,
    next: u16,
    previous: Option<MediaSectionDescriptor>,
}

impl SectionScan {
    pub(super) const fn layout(&self) -> MediaLayout {
        self.layout
    }
    pub fn record_range(&self) -> Option<Range<usize>> {
        if self.next >= self.layout.header.section_count {
            return None;
        }
        let start = MEDIA_HEADER_LEN + usize::from(self.next) * MEDIA_SECTION_LEN;
        Some(start..start + MEDIA_SECTION_LEN)
    }

    /// Consume the next complete record window. `Ok(None)` means every declared record has already been accepted; a short window is an error, not the end of the table.
    pub fn accept(
        &mut self,
        record: &[u8],
    ) -> Result<Option<MediaSectionDescriptor>, MediaPayloadError> {
        let Some(range) = self.record_range() else {
            return Ok(None);
        };
        if record.len() < MEDIA_SECTION_LEN {
            return Err(MediaPayloadError::Truncated {
                needed: range.end,
                available: range.start + record.len(),
            });
        }
        let index = self.next;
        let descriptor = MediaSectionDescriptor::read(record)
            .ok_or(MediaPayloadError::InvalidSectionKind { index })?;
        if descriptor.offset < self.layout.directory_end {
            return Err(MediaPayloadError::SectionBeforeBodies {
                index,
                offset: descriptor.offset,
                minimum: self.layout.directory_end,
            });
        }
        let end = descriptor
            .offset
            .checked_add(descriptor.size)
            .ok_or(MediaPayloadError::SizeOverflow)?;
        if end > self.layout.covered_len {
            return Err(MediaPayloadError::SectionOutOfBounds {
                index,
                offset: descriptor.offset,
                size: descriptor.size,
            });
        }
        if let Some(previous) = self.previous {
            if descriptor.offset < previous.offset {
                return Err(MediaPayloadError::SectionsOutOfOrder {
                    previous: index - 1,
                    next: index,
                });
            }
            if descriptor.offset < previous.offset + previous.size {
                return Err(MediaPayloadError::SectionsOverlap {
                    previous: index - 1,
                    next: index,
                });
            }
        }
        self.previous = Some(descriptor);
        self.next += 1;
        Ok(Some(descriptor))
    }
}
