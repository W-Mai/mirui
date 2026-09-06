use core::iter::FusedIterator;

use super::{
    AccessCapabilities, CoverageBudget, CoverageError, SURFACE_RECORD_LEN, SurfaceDescriptor,
    SurfaceRecordError, UNIT_GROUP_RECORD_LEN, UnitGroup, UnitGroupRecordError,
};
use crate::media::{
    CodingTable, CodingTableError, MediaPayload, MediaPayloadError, MediaSection, MediaSectionKind,
};
use crate::payload::ColorTableView;

mod decode;
mod encode;
mod groups;
#[cfg(test)]
use super::UnitGroupRecord;
pub(crate) use groups::{CodingRecords, GroupRecords, GroupSource};
mod preflight;
pub use decode::{DecodeError, ImageDecodePlan};
pub use encode::EncodedImageAsset;
pub(crate) use encode::StoragePlan;
pub(crate) use preflight::Preflight;

#[cfg(test)]
mod sections_tests;
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
    checksum_bytes: u32,
}

/// Resolved directory entries, without resource-specific selection policy.
pub(crate) struct EncodedSections<'a> {
    pub(crate) codings: Option<MediaSection<'a>>,
    pub(crate) data: Option<MediaSection<'a>>,
    pub(crate) records: Option<MediaSection<'a>>,
    pub(crate) indexes: Option<MediaSection<'a>>,
    pub(crate) color_table: Option<MediaSection<'a>>,
}

impl<'a> EncodedImageView<'a> {
    pub fn open(payload: &'a [u8]) -> Result<Self, EncodedImageError> {
        let media = MediaPayload::open(payload).map_err(EncodedImageError::Media)?;
        Self::from_media(media, None)
    }
    /// Retains the outer position for file-relative checks during group preparation.
    pub fn open_at(payload: &'a [u8], file_offset: u32) -> Result<Self, EncodedImageError> {
        let media = MediaPayload::open(payload).map_err(EncodedImageError::Media)?;
        Self::from_media(media, Some(file_offset))
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

    pub(crate) const fn with_file_offset(mut self, offset: u32) -> Self {
        self.file_offset = Some(offset);
        self
    }
    /// Number of caller workspace slots, including an implicit whole-surface group.
    pub fn group_count(self) -> usize {
        self.records
            .map_or(1, |records| records.bytes().len() / UNIT_GROUP_RECORD_LEN)
    }
    pub fn validate_data(self) -> Result<(), EncodedImageError> {
        self.media.validate_data().map_err(EncodedImageError::Media)
    }

    /// Maximum declared group input alignment, or one for an implicit group.
    ///
    /// This scans only group records, without expanding units or checking
    /// coverage, codec syntax or actual backing addresses.
    pub fn input_alignment(self) -> Result<u32, EncodedImageError> {
        self.group_source().input_alignment()
    }

    pub(super) fn from_media(
        media: MediaPayload<'a>,
        file_offset: Option<u32>,
    ) -> Result<Self, EncodedImageError> {
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
        Self::from_sections(
            media,
            surface,
            EncodedSections {
                codings,
                data,
                records,
                indexes,
                color_table,
            },
            file_offset,
        )
    }

    pub(crate) fn from_sections(
        media: MediaPayload<'a>,
        surface: SurfaceDescriptor,
        sections: EncodedSections<'a>,
        file_offset: Option<u32>,
    ) -> Result<Self, EncodedImageError> {
        let EncodedSections {
            codings,
            data,
            records,
            indexes,
            color_table,
        } = sections;
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
            // Validated disjoint DATA spans fit inside the u32 payload boundary.
            checksum_bytes: media
                .sections_of_kind(MediaSectionKind::DATA)
                .map(|section| section.descriptor().size())
                .sum(),
        })
    }

    fn group_source(self) -> GroupSource<'a> {
        GroupSource {
            surface: self.surface,
            codings: CodingRecords::Wire(self.codings),
            records: self.records.map_or(GroupRecords::Implicit, |section| {
                GroupRecords::Wire(section.bytes())
            }),
            data: self.data.bytes(),
            indexes: self.indexes.map_or(&[], |section| section.bytes()),
            file_offset: self.file_offset,
            data_offset: self.data.descriptor().offset(),
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
        self.group_source()
            .visit_groups(None, |index, group| workspace[index] = Some(group))?;
        self.surface
            .validate_coverage_by(
                count,
                |index, _| Ok(workspace[index].expect("prepared group")),
                budget,
            )
            .map_err(EncodedImageError::Coverage)?;
        Ok(ImageGroups {
            image: self,
            groups: workspace,
        })
    }

    /// Validates groups and exact static coverage without storing a group table.
    ///
    /// Immutable records are re-resolved during coverage checks. Each resolution
    /// charges one record visit, its DATA span and the complete UNIT_INDEX byte
    /// count before parsing, in addition to geometric coverage work. This is a
    /// conservative work bound, not bytes read or elapsed time. Prepared caller
    /// workspace avoids those repeated scans. Codec syntax and DATA checksums
    /// remain separate checks.
    pub fn validate_groups(self, budget: &mut CoverageBudget) -> Result<(), EncodedImageError> {
        self.group_source().validate_groups(budget)
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

    /// Reports access implemented by the built-in scalar decoder.
    ///
    /// Every group is checked before the capability is returned. Unsupported
    /// profile metadata is reported without reading encoded DATA or writing an
    /// output buffer.
    pub fn access_capabilities(self) -> Result<AccessCapabilities, EncodedImageError> {
        for (group, value) in self.iter().enumerate() {
            value
                .access_capabilities()
                .map_err(|error| EncodedImageError::Coding { group, error })?;
        }
        Ok(AccessCapabilities::encoded_image())
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
            .checked_add(self.image.group_source().record(group)?.data_range().start)
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
    TooManyGroups {
        limit: u32,
        actual: usize,
    },
    TooManyUnits {
        limit: u32,
        actual: u64,
    },
    Coding {
        group: usize,
        error: super::UnitDecodeError,
    },
    Unit {
        group: usize,
        ordinal: usize,
        error: super::UnitDecodeError,
    },
    DecodedUnitTooLarge {
        group: usize,
        ordinal: usize,
        limit: usize,
        actual: usize,
    },
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
