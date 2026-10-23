use core::ops::Range;

use super::{EncodedImageError, EncodedImageLayout};
use crate::{
    image::{SURFACE_RECORD_LEN, SurfaceDescriptor},
    media::{
        MEDIA_SECTION_LEN, MediaLayout, MediaPayloadError, MediaSectionDescriptor,
        MediaSectionKind, SectionScan,
    },
};

pub(super) struct ImageSections<T> {
    pub surface: Option<T>,
    pub codings: Option<T>,
    pub data: Option<T>,
    pub records: Option<T>,
    pub indexes: Option<T>,
    pub color_table: Option<T>,
}

impl<T> Default for ImageSections<T> {
    fn default() -> Self {
        Self {
            surface: None,
            codings: None,
            data: None,
            records: None,
            indexes: None,
            color_table: None,
        }
    }
}

impl<T> ImageSections<T> {
    pub fn accept(
        &mut self,
        value: T,
        descriptor: MediaSectionDescriptor,
    ) -> Result<(), EncodedImageError> {
        let kind = descriptor.kind();
        let slot = match kind {
            MediaSectionKind::SURFACE => Some(&mut self.surface),
            MediaSectionKind::CODINGS => Some(&mut self.codings),
            MediaSectionKind::DATA => Some(&mut self.data),
            MediaSectionKind::UNIT_GROUPS => Some(&mut self.records),
            MediaSectionKind::UNIT_INDEX => Some(&mut self.indexes),
            MediaSectionKind::COLOR_TABLE => Some(&mut self.color_table),
            MediaSectionKind::INTEGRITY => None,
            MediaSectionKind::PLANES => return Err(EncodedImageError::UnexpectedSection(kind)),
            _ if descriptor.flags().is_required() => {
                return Err(EncodedImageError::UnknownRequiredSection(kind));
            }
            _ => None,
        };
        if let Some(slot) = slot {
            if slot.is_some() {
                return Err(EncodedImageError::DuplicateSection(kind));
            }
            if !descriptor.flags().is_required() {
                return Err(EncodedImageError::SectionMustBeRequired(kind));
            }
            *slot = Some(value);
        }
        Ok(())
    }
}

pub(super) fn parse_surface(bytes: &[u8]) -> Result<SurfaceDescriptor, EncodedImageError> {
    check_size(MediaSectionKind::SURFACE, SURFACE_RECORD_LEN, bytes.len())?;
    SurfaceDescriptor::from_record(bytes).map_err(EncodedImageError::Surface)
}

fn check_size(
    kind: MediaSectionKind,
    expected: usize,
    actual: usize,
) -> Result<(), EncodedImageError> {
    if actual == expected {
        Ok(())
    } else {
        Err(EncodedImageError::SectionSizeMismatch {
            kind,
            expected,
            actual,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Directory,
    Surface,
    Done,
}

/// Fixed-storage encoded IMAGE directory scan, without retaining DATA or coding tables.
///
/// Feed nonempty prefixes of `read_range()` to `accept`. After the scan completes, `section` provides payload-relative metadata descriptors for caller-budgeted reads; `finish` binds those bodies without copying them. Metadata CRC, DATA integrity, group coverage and codec syntax remain separate checks.
pub struct EncodedImageScan {
    directory: SectionScan,
    sections: ImageSections<MediaSectionDescriptor>,
    record: [u8; MEDIA_SECTION_LEN],
    surface: [u8; SURFACE_RECORD_LEN],
    phase: Phase,
    filled: usize,
    payload_offset: u32,
}

impl EncodedImageScan {
    pub fn new(layout: MediaLayout, payload_offset: u32) -> Self {
        Self {
            directory: layout.sections(),
            sections: ImageSections::default(),
            record: [0; MEDIA_SECTION_LEN],
            surface: [0; SURFACE_RECORD_LEN],
            phase: Phase::Directory,
            filled: 0,
            payload_offset,
        }
    }

    pub fn read_range(&self) -> Option<Range<usize>> {
        let mut range = match self.phase {
            Phase::Directory => self.directory.record_range()?,
            Phase::Surface => {
                let section = self.sections.surface?;
                section.offset() as usize..section.offset() as usize + section.size() as usize
            }
            Phase::Done => return None,
        };
        range.start += self.filled;
        Some(range)
    }

    pub fn accept(&mut self, bytes: &[u8]) -> Result<(), EncodedImageError> {
        let requested = self.read_range().ok_or_else(incomplete)?;
        if bytes.is_empty() || bytes.len() > requested.len() {
            return Err(incomplete());
        }
        let next = self.filled + bytes.len();
        match self.phase {
            Phase::Directory => {
                self.record[self.filled..next].copy_from_slice(bytes);
                if next == self.record.len() {
                    let mut directory = self.directory;
                    let descriptor = directory
                        .accept(&self.record)
                        .map_err(EncodedImageError::Media)?
                        .expect("requested record");
                    self.sections.accept(descriptor, descriptor)?;
                    self.directory = directory;
                }
            }
            Phase::Surface => self.surface[self.filled..next].copy_from_slice(bytes),
            Phase::Done => unreachable!(),
        }
        self.filled = next;
        if bytes.len() == requested.len() {
            self.filled = 0;
            match self.phase {
                Phase::Directory if self.directory.record_range().is_none() => {
                    let surface = self
                        .sections
                        .surface
                        .ok_or(EncodedImageError::MissingSection(MediaSectionKind::SURFACE))?;
                    check_size(
                        MediaSectionKind::SURFACE,
                        SURFACE_RECORD_LEN,
                        surface.size() as usize,
                    )?;
                    self.sections
                        .codings
                        .ok_or(EncodedImageError::MissingSection(MediaSectionKind::CODINGS))?;
                    self.sections
                        .data
                        .ok_or(EncodedImageError::MissingSection(MediaSectionKind::DATA))?;
                    self.phase = Phase::Surface;
                }
                Phase::Surface => {
                    parse_surface(&self.surface)?.validate_color_table_size(
                        self.sections.color_table.map(|s| s.size() as usize),
                    )?;
                    self.phase = Phase::Done;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Returns a selected descriptor only after the directory and SURFACE scan completes. INTEGRITY and optional unknown sections belong to their separate scans.
    pub fn section(&self, kind: MediaSectionKind) -> Option<MediaSectionDescriptor> {
        if self.phase != Phase::Done {
            return None;
        }
        match kind {
            MediaSectionKind::SURFACE => self.sections.surface,
            MediaSectionKind::CODINGS => self.sections.codings,
            MediaSectionKind::DATA => self.sections.data,
            MediaSectionKind::UNIT_GROUPS => self.sections.records,
            MediaSectionKind::UNIT_INDEX => self.sections.indexes,
            MediaSectionKind::COLOR_TABLE => self.sections.color_table,
            _ => None,
        }
    }

    pub fn surface(&self) -> Result<SurfaceDescriptor, EncodedImageError> {
        if self.phase != Phase::Done {
            return Err(incomplete());
        }
        parse_surface(&self.surface)
    }

    /// Checks the exact metadata body lengths and binds them to the existing unit-addressing interface. The result borrows caller buffers, not this scanner. Palette bytes are not bound or decoded.
    pub fn finish<'a>(
        &self,
        codings: &'a [u8],
        groups: Option<&'a [u8]>,
        indexes: Option<&'a [u8]>,
    ) -> Result<EncodedImageLayout<'a>, EncodedImageError> {
        let surface = self.surface()?;
        for (kind, expected, actual) in [
            (
                MediaSectionKind::CODINGS,
                self.sections.codings,
                Some(codings),
            ),
            (MediaSectionKind::UNIT_GROUPS, self.sections.records, groups),
            (MediaSectionKind::UNIT_INDEX, self.sections.indexes, indexes),
        ] {
            match (expected, actual) {
                (Some(section), Some(bytes)) => {
                    check_size(kind, section.size() as usize, bytes.len())?
                }
                (Some(_), None) => return Err(EncodedImageError::MissingSection(kind)),
                (None, Some(_)) => return Err(EncodedImageError::UnexpectedSection(kind)),
                (None, None) => {}
            }
        }
        let data = self.sections.data.expect("checked DATA section");
        EncodedImageLayout::new(
            surface,
            codings,
            groups,
            indexes,
            data.size(),
            Some(
                self.payload_offset
                    .checked_add(data.offset())
                    .ok_or(EncodedImageError::SizeOverflow)?,
            ),
        )
    }
}

fn incomplete() -> EncodedImageError {
    EncodedImageError::Media(MediaPayloadError::InvalidDataRange)
}

#[cfg(test)]
mod tests;
