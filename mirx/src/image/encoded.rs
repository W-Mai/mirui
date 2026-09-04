use core::iter::FusedIterator;

use super::{
    CoverageBudget, CoverageError, ReferenceMode, SURFACE_RECORD_LEN, SurfaceDescriptor,
    SurfaceRecordError, UNIT_GROUP_RECORD_LEN, UnitGroup, UnitGroupRecord, UnitGroupRecordError,
};
use crate::media::{
    CodingTable, CodingTableError, MediaPayload, MediaPayloadError, MediaSection, MediaSectionKind,
};
use crate::payload::ColorTableView;

mod encode;
pub use encode::EncodedImageAsset;

#[cfg(test)]
mod tests;

/// Borrowed encoded IMAGE metadata; opening does not decode or scan DATA.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedImageView<'a> {
    media: MediaPayload<'a>,
    surface: SurfaceDescriptor,
    codings: CodingTable<'a>,
    data: MediaSection<'a>,
    records: Option<MediaSection<'a>>,
    indexes: Option<MediaSection<'a>>,
    color_table: Option<ColorTableView<'a>>,
    file_offset: Option<u32>,
}

impl<'a> EncodedImageView<'a> {
    pub fn open(payload: &'a [u8]) -> Result<Self, EncodedImageError> {
        Self::from_payload(payload, None)
    }
    /// Retains the outer position for file-relative checks during group preparation.
    pub fn open_at(payload: &'a [u8], file_offset: u32) -> Result<Self, EncodedImageError> {
        Self::from_payload(payload, Some(file_offset))
    }
    pub const fn media(self) -> MediaPayload<'a> {
        self.media
    }
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }
    pub const fn codings(self) -> CodingTable<'a> {
        self.codings
    }
    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        self.color_table
    }
    /// Number of caller workspace slots, including an implicit whole-surface group.
    pub fn group_count(self) -> usize {
        self.records
            .map_or(1, |records| records.bytes().len() / UNIT_GROUP_RECORD_LEN)
    }
    pub fn validate_data(self) -> Result<(), EncodedImageError> {
        self.media.validate_data().map_err(EncodedImageError::Media)
    }

    fn from_payload(
        payload: &'a [u8],
        file_offset: Option<u32>,
    ) -> Result<Self, EncodedImageError> {
        let media = MediaPayload::open(payload).map_err(EncodedImageError::Media)?;
        let mut surface = None;
        let mut codings = None;
        let mut data = None;
        let mut records = None;
        let mut indexes = None;
        let mut color_table = None;
        for section in media.sections() {
            let kind = section.descriptor().kind();
            let slot = match kind {
                MediaSectionKind::SURFACE => Some(&mut surface),
                MediaSectionKind::CODINGS => Some(&mut codings),
                MediaSectionKind::DATA => Some(&mut data),
                MediaSectionKind::UNIT_GROUPS => Some(&mut records),
                MediaSectionKind::UNIT_INDEX => Some(&mut indexes),
                MediaSectionKind::COLOR_TABLE => Some(&mut color_table),
                MediaSectionKind::INTEGRITY => None,
                MediaSectionKind::PLANES => return Err(EncodedImageError::UnexpectedSection(kind)),
                _ if section.descriptor().flags().is_required() => {
                    return Err(EncodedImageError::UnknownRequiredSection(kind));
                }
                _ => None,
            };
            if let Some(slot) = slot {
                if slot.replace(section).is_some() {
                    return Err(EncodedImageError::DuplicateSection(kind));
                }
                if !section.descriptor().flags().is_required() {
                    return Err(EncodedImageError::SectionMustBeRequired(kind));
                }
            }
        }
        let surface =
            surface.ok_or(EncodedImageError::MissingSection(MediaSectionKind::SURFACE))?;
        if surface.bytes().len() != SURFACE_RECORD_LEN {
            return Err(EncodedImageError::SectionSizeMismatch {
                kind: MediaSectionKind::SURFACE,
                expected: SURFACE_RECORD_LEN,
                actual: surface.bytes().len(),
            });
        }
        let surface =
            SurfaceDescriptor::from_record(surface.bytes()).map_err(EncodedImageError::Surface)?;
        let codings = CodingTable::open(
            codings
                .ok_or(EncodedImageError::MissingSection(MediaSectionKind::CODINGS))?
                .bytes(),
        )
        .map_err(EncodedImageError::Codings)?;
        let data = data.ok_or(EncodedImageError::MissingSection(MediaSectionKind::DATA))?;
        if let Some(records) = records {
            if records.bytes().is_empty() || records.bytes().len() % UNIT_GROUP_RECORD_LEN != 0 {
                return Err(EncodedImageError::InvalidGroupTableLength(
                    records.bytes().len(),
                ));
            }
        } else if codings.len() != 1 || indexes.is_some() {
            return Err(EncodedImageError::AmbiguousImplicitGroup);
        }
        if indexes.is_some_and(|section| section.bytes().is_empty()) {
            return Err(EncodedImageError::EmptyIndexSection);
        }
        let color_table = surface
            .read_color_table(color_table.map(|section| section.bytes()))
            .map_err(EncodedImageError::from)?;
        Ok(Self {
            media,
            surface,
            codings,
            data,
            records,
            indexes,
            color_table,
            file_offset,
        })
    }

    fn record(self, index: usize) -> Result<UnitGroupRecord, EncodedImageError> {
        if index >= self.group_count() {
            return Err(EncodedImageError::GroupOutOfBounds(index));
        }
        match self.records {
            Some(records) => {
                UnitGroupRecord::open(&records.bytes()[index * UNIT_GROUP_RECORD_LEN..])
                    .map_err(|error| EncodedImageError::Group { index, error })
            }
            None => UnitGroupRecord::new(0, 0..self.data.descriptor().size())
                .map_err(|error| EncodedImageError::Group { index, error }),
        }
    }

    /// Prepares exact static coverage in caller-owned scratch, without allocation.
    ///
    /// Capacity errors preserve scratch. Other failures may overwrite its used
    /// prefix; only a returned handle guarantees prepared groups. The budget
    /// bounds geometric coverage, not the linear parsing of stored metadata.
    pub fn groups_into<'g>(
        self,
        workspace: &'g mut [Option<UnitGroup<'a>>],
        budget: &mut CoverageBudget,
    ) -> Result<ImageGroups<'a, 'g>, EncodedImageError> {
        let count = self.group_count();
        if workspace.len() < count {
            return Err(EncodedImageError::WorkspaceTooSmall {
                needed: count,
                available: workspace.len(),
            });
        }
        let workspace = &mut workspace[..count];
        workspace.fill(None);
        let index_bytes = self.indexes.map_or(&[][..], |section| section.bytes());
        let mut data_end = 0u32;
        let mut index_end = 0u32;
        for (index, slot) in workspace.iter_mut().enumerate() {
            let record = self.record(index)?;
            if record.reference() != ReferenceMode::Independent {
                return Err(EncodedImageError::ReferenceInStaticImage(index));
            }
            let alignment = record.input_alignment();
            let expected_start = data_end
                .checked_add(alignment - 1)
                .map(|end| end & !(alignment - 1))
                .ok_or(EncodedImageError::SizeOverflow)?;
            if record.data_range().start != expected_start {
                return Err(EncodedImageError::NonCanonicalDataRange {
                    index,
                    expected_start,
                    actual_start: record.data_range().start,
                });
            }
            if let Some(file_offset) = self.file_offset {
                let absolute_offset = file_offset
                    .checked_add(self.data.descriptor().offset())
                    .and_then(|offset| offset.checked_add(record.data_range().start))
                    .ok_or(EncodedImageError::SizeOverflow)?;
                if absolute_offset % alignment != 0 {
                    return Err(EncodedImageError::FileAddressUnaligned {
                        index,
                        absolute_offset,
                        alignment,
                    });
                }
            }
            let (group, range) = record
                .resolve_with_index_range(
                    self.surface,
                    self.codings,
                    self.data.bytes(),
                    index_bytes,
                )
                .map_err(|error| EncodedImageError::Group { index, error })?;
            if group.is_empty() && self.records.is_some() {
                return Err(EncodedImageError::EmptyGroup(index));
            }
            if !range.is_empty() {
                if range.start != index_end {
                    return Err(EncodedImageError::NonCanonicalIndexRange {
                        index,
                        expected_start: index_end,
                        actual_start: range.start,
                    });
                }
                index_end = range.end;
            }
            data_end = record.data_range().end;
            *slot = Some(group);
        }
        if data_end != self.data.descriptor().size() {
            return Err(EncodedImageError::UnreferencedData);
        }
        if index_end as usize != index_bytes.len() {
            return Err(EncodedImageError::UnreferencedIndexes);
        }
        self.surface
            .validate_coverage_by(
                count,
                |index| workspace[index].expect("prepared group"),
                budget,
            )
            .map_err(EncodedImageError::Coverage)?;
        Ok(ImageGroups {
            image: self,
            groups: workspace,
        })
    }
}

/// Prepared static IMAGE groups borrowing caller workspace and encoded bytes.
#[derive(Clone, Copy, Debug)]
pub struct ImageGroups<'a, 'g> {
    image: EncodedImageView<'a>,
    groups: &'g [Option<UnitGroup<'a>>],
}

impl<'a, 'g> ImageGroups<'a, 'g> {
    pub const fn image(self) -> EncodedImageView<'a> {
        self.image
    }
    pub const fn len(self) -> usize {
        self.groups.len()
    }
    pub const fn is_empty(self) -> bool {
        self.groups.is_empty()
    }
    pub fn get(self, index: usize) -> Option<UnitGroup<'a>> {
        self.groups.get(index).copied().flatten()
    }
    pub fn iter(self) -> ImageGroupIter<'a, 'g> {
        ImageGroupIter {
            groups: self.groups.iter(),
        }
    }
    /// Verifies intersecting checksum coverage and returns actual bytes checked.
    pub fn validate_unit(self, group: usize, ordinal: usize) -> Result<u32, EncodedImageError> {
        let unit = self
            .get(group)
            .ok_or(EncodedImageError::GroupOutOfBounds(group))?
            .get(ordinal)
            .ok_or(EncodedImageError::UnitOutOfBounds { group, ordinal })?;
        let base = self
            .image
            .data
            .descriptor()
            .offset()
            .checked_add(self.image.record(group)?.data_range().start)
            .ok_or(EncodedImageError::SizeOverflow)?;
        let range = unit.data_range();
        let start = base
            .checked_add(range.start)
            .ok_or(EncodedImageError::SizeOverflow)?;
        let end = base
            .checked_add(range.end)
            .ok_or(EncodedImageError::SizeOverflow)?;
        self.image
            .media
            .validate_data_range(start..end)
            .map_err(EncodedImageError::Media)
    }
}

#[derive(Clone, Debug)]
pub struct ImageGroupIter<'a, 'g> {
    groups: core::slice::Iter<'g, Option<UnitGroup<'a>>>,
}
impl<'a> Iterator for ImageGroupIter<'a, '_> {
    type Item = UnitGroup<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        self.groups
            .next()
            .map(|group| group.expect("prepared group"))
    }
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.groups
            .nth(n)
            .map(|group| group.expect("prepared group"))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.groups.size_hint()
    }
    fn count(self) -> usize {
        self.groups.len()
    }
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}
impl DoubleEndedIterator for ImageGroupIter<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.groups
            .next_back()
            .map(|group| group.expect("prepared group"))
    }
    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.groups
            .nth_back(n)
            .map(|group| group.expect("prepared group"))
    }
}
impl ExactSizeIterator for ImageGroupIter<'_, '_> {
    fn len(&self) -> usize {
        self.groups.len()
    }
}
impl FusedIterator for ImageGroupIter<'_, '_> {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodedImageError {
    Media(MediaPayloadError),
    Surface(SurfaceRecordError),
    Codings(CodingTableError),
    MissingSection(MediaSectionKind),
    DuplicateSection(MediaSectionKind),
    SectionMustBeRequired(MediaSectionKind),
    UnknownRequiredSection(MediaSectionKind),
    UnexpectedSection(MediaSectionKind),
    SectionSizeMismatch {
        kind: MediaSectionKind,
        expected: usize,
        actual: usize,
    },
    MissingColorTable,
    UnexpectedColorTable,
    InvalidGroupTableLength(usize),
    EmptyIndexSection,
    AmbiguousImplicitGroup,
    WorkspaceTooSmall {
        needed: usize,
        available: usize,
    },
    Group {
        index: usize,
        error: UnitGroupRecordError,
    },
    ReferenceInStaticImage(usize),
    EmptyGroup(usize),
    GroupOutOfBounds(usize),
    UnitOutOfBounds {
        group: usize,
        ordinal: usize,
    },
    NonCanonicalDataRange {
        index: usize,
        expected_start: u32,
        actual_start: u32,
    },
    NonCanonicalIndexRange {
        index: usize,
        expected_start: u32,
        actual_start: u32,
    },
    UnreferencedData,
    UnreferencedIndexes,
    FileAddressUnaligned {
        index: usize,
        absolute_offset: u32,
        alignment: u32,
    },
    Coverage(CoverageError),
    SizeOverflow,
}

impl From<super::color_table::ColorTableError> for EncodedImageError {
    fn from(error: super::color_table::ColorTableError) -> Self {
        use super::color_table::ColorTableError;
        match error {
            ColorTableError::Missing => Self::MissingColorTable,
            ColorTableError::Unexpected => Self::UnexpectedColorTable,
            ColorTableError::SizeMismatch { expected, actual } => Self::SectionSizeMismatch {
                kind: MediaSectionKind::COLOR_TABLE,
                expected,
                actual,
            },
            ColorTableError::SizeOverflow => Self::SizeOverflow,
        }
    }
}
