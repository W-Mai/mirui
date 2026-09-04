//! Common wire vocabulary for sectioned media payloads.
//!
//! Sectioned payloads share a fixed header, directory, coding identifiers,
//! and independent metadata/DATA checksum coverage. Typed payload modules
//! define section contents and physical storage requirements.

use core::iter::FusedIterator;

mod coding;
mod index;
mod integrity;
mod selection;
pub use coding::{
    CODING_RECORD_LEN, CODING_TABLE_HEADER_LEN, CodingRecord, CodingTable, CodingTableError,
};
pub use index::{
    UNIT_CHECKPOINT_INTERVAL, UnitIndex, UnitIndexEncoding, UnitIndexError, UnitRanges,
};
pub use integrity::{
    DataCheckPlan, DataIntegrity, INTEGRITY_RECORD_LEN, IntegrityError, IntegrityRange,
    IntegrityRanges, IntegrityTable,
};
pub use selection::{
    SELECTION_CHECKPOINT_INTERVAL, SelectedUnits, UnitSelection, UnitSelectionEncoding,
    UnitSelectionError,
};

use crate::crc32::Crc32;
use crate::wire::{read_u16_le, read_u32_le};

pub const MEDIA_HEADER_LEN: usize = 8;
pub const MEDIA_SECTION_LEN: usize = 12;
pub const MEDIA_CRC_LEN: usize = 4;
pub const MEDIA_VERSION: u8 = 1;

pub(crate) mod output;

/// Open identifier for one stored coding profile.
///
/// Unknown values remain representable so raw editing never depends on the
/// decoder profiles compiled into the current application.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodingId(u16);

impl CodingId {
    /// Uncoded bytes described directly by the payload's memory records.
    pub const RAW: Self = Self(0);
    /// Independent lossless RGB/RGBA pixel state stream.
    pub const PIXEL: Self = Self(1);
    /// Independent lossless byte or fixed-width element run-length stream.
    pub const RLE: Self = Self(2);
    /// Independent LZ4 blocks without frames, dictionaries, or size prefixes.
    pub const LZ4: Self = Self(3);
    /// Reversible color decorrelation and integer 8x8 frequency blocks.
    pub const FREQUENCY_REVERSIBLE: Self = Self(4);
    /// Quantized integer 8x8 frequency blocks with explicit quality.
    pub const FREQUENCY_QUANTIZED: Self = Self(5);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl From<u16> for CodingId {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

impl From<CodingId> for u16 {
    fn from(value: CodingId) -> Self {
        value.raw()
    }
}

/// Open media-payload flags.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MediaFlags(u8);

impl MediaFlags {
    pub const NONE: Self = Self(0);
    /// INTEGRITY records replace the whole-DATA checksum trailer.
    pub const INDEXED_INTEGRITY: Self = Self(1);

    pub const fn has_indexed_integrity(self) -> bool {
        self.0 & Self::INDEXED_INTEGRITY.0 != 0
    }
    pub const fn unknown_bits(self) -> u8 {
        self.0 & !Self::INDEXED_INTEGRITY.0
    }

    pub const fn from_bits_retain(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// Open section identifier shared by IMAGE and FONT payloads.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MediaSectionKind(u16);

impl MediaSectionKind {
    pub const SURFACE: Self = Self(0x0001);
    pub const PLANES: Self = Self(0x0002);
    pub const CODINGS: Self = Self(0x0003);
    pub const UNIT_GROUPS: Self = Self(0x0004);
    pub const DATA: Self = Self(0x0005);
    /// RGBA entries referenced by an indexed IMAGE sample plane.
    pub const COLOR_TABLE: Self = Self(0x0006);

    pub const UNIT_INDEX: Self = Self(0x0007);
    pub const INTEGRITY: Self = Self(0x0008);

    pub const CODEPOINTS: Self = Self(0x0011);
    pub const REPRESENTATIONS: Self = Self(0x0012);
    pub const METRICS: Self = Self(0x0013);
    pub const GLYPH_MAPS: Self = Self(0x0014);
    pub const SURFACE_GROUPS: Self = Self(0x0015);

    pub const fn new(value: u16) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn raw(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for MediaSectionKind {
    type Error = InvalidMediaSectionKind;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidMediaSectionKind)
    }
}

impl From<MediaSectionKind> for u16 {
    fn from(value: MediaSectionKind) -> Self {
        value.raw()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidMediaSectionKind;

/// Open flags attached to one section directory entry.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MediaSectionFlags(u16);

impl MediaSectionFlags {
    pub const NONE: Self = Self(0);
    /// Readers must understand the section to expose the typed payload.
    pub const REQUIRED: Self = Self(1);

    pub const fn from_bits_retain(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn is_required(self) -> bool {
        self.0 & Self::REQUIRED.0 != 0
    }
}

/// Common media identity and metadata checksum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaHeader {
    flags: MediaFlags,
    section_count: u16,
    metadata_crc32: u32,
}

impl MediaHeader {
    pub const fn flags(self) -> MediaFlags {
        self.flags
    }
    pub const fn section_count(self) -> u16 {
        self.section_count
    }
    pub const fn metadata_crc32(self) -> u32 {
        self.metadata_crc32
    }

    fn read(bytes: &[u8]) -> Self {
        Self {
            flags: MediaFlags::from_bits_retain(bytes[1]),
            section_count: read_u16_le(bytes, 2).expect("validated media header"),
            metadata_crc32: read_u32_le(bytes, 4).expect("validated media header"),
        }
    }
}

/// Decoded directory record for one section body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaSectionDescriptor {
    kind: MediaSectionKind,
    flags: MediaSectionFlags,
    offset: u32,
    size: u32,
}

impl MediaSectionDescriptor {
    pub const fn kind(self) -> MediaSectionKind {
        self.kind
    }

    pub const fn flags(self) -> MediaSectionFlags {
        self.flags
    }

    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Byte length of the stored section body.
    pub const fn size(self) -> u32 {
        self.size
    }

    fn read(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            kind: MediaSectionKind::new(read_u16_le(bytes, 0)?)?,
            flags: MediaSectionFlags::from_bits_retain(read_u16_le(bytes, 2)?),
            offset: read_u32_le(bytes, 4)?,
            size: read_u32_le(bytes, 8)?,
        })
    }
}

/// Failure while opening or validating a common media payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MediaPayloadError {
    Truncated {
        needed: usize,
        available: usize,
    },
    UnsupportedVersion(u8),
    InvalidAlignment(u32),
    InvalidDataRange,
    MissingIntegrityTable,
    UnexpectedIntegrityTable,
    DuplicateIntegrityTable,
    IntegrityTableMustBeRequired,
    Integrity(IntegrityError),
    RangeCrcMismatch {
        offset: u32,
        expected: u32,
        actual: u32,
    },
    MetadataCrcMismatch {
        expected: u32,
        actual: u32,
    },

    SectionTableOutOfBounds,
    InvalidSectionKind {
        index: u16,
    },
    SectionBeforeBodies {
        index: u16,
        offset: u32,
        minimum: u32,
    },
    SectionOutOfBounds {
        index: u16,
        offset: u32,
        size: u32,
    },
    SectionsOutOfOrder {
        previous: u16,
        next: u16,
    },
    SectionsOverlap {
        previous: u16,
        next: u16,
    },
    DataOffsetUnaligned {
        index: u16,
        absolute_offset: u32,
        alignment: u32,
    },
    DataCrcMismatch {
        expected: u32,
        actual: u32,
    },
    SizeOverflow,
}

/// Borrowed, validated common media payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaPayload<'a> {
    payload: &'a [u8],
    header: MediaHeader,
    directory: &'a [u8],
    integrity: Option<IntegrityTable<'a>>,
}

impl<'a> MediaPayload<'a> {
    pub(crate) const fn payload_len(self) -> usize {
        self.payload.len()
    }
    /// Opens metadata without reading or checksumming DATA bodies.
    ///
    /// Validates section bounds and the metadata CRC, including the stored
    /// DATA checksum. Call `validate_data` before consuming the whole DATA
    /// coverage. This slice API does not perform streamed file reads.
    pub fn open(payload: &'a [u8]) -> Result<Self, MediaPayloadError> {
        let media = Self::parse(payload)?;
        let actual = media.metadata_crc();
        let expected = media.header.metadata_crc32;
        if actual != expected {
            return Err(MediaPayloadError::MetadataCrcMismatch { expected, actual });
        }
        Ok(media)
    }

    fn parse(payload: &'a [u8]) -> Result<Self, MediaPayloadError> {
        if let Some(&version) = payload.first() {
            if version != MEDIA_VERSION {
                return Err(MediaPayloadError::UnsupportedVersion(version));
            }
        }
        let trailer_len = if payload
            .get(1)
            .is_some_and(|&flags| MediaFlags::from_bits_retain(flags).has_indexed_integrity())
        {
            0
        } else {
            MEDIA_CRC_LEN
        };
        let needed = MEDIA_HEADER_LEN + trailer_len;
        if payload.len() < needed {
            return Err(MediaPayloadError::Truncated {
                needed,
                available: payload.len(),
            });
        }
        u32::try_from(payload.len()).map_err(|_| MediaPayloadError::SizeOverflow)?;
        let header = MediaHeader::read(payload);
        let covered = &payload[..payload.len() - trailer_len];
        let directory_end = usize::from(header.section_count)
            .checked_mul(MEDIA_SECTION_LEN)
            .and_then(|len| MEDIA_HEADER_LEN.checked_add(len))
            .ok_or(MediaPayloadError::SizeOverflow)?;
        if directory_end > covered.len() {
            return Err(MediaPayloadError::SectionTableOutOfBounds);
        }
        let directory = &covered[MEDIA_HEADER_LEN..directory_end];
        validate_sections(covered, directory, header.section_count, directory_end)?;
        let mut media = Self {
            payload,
            header,
            directory,
            integrity: None,
        };
        let mut tables = media.sections_of_kind(MediaSectionKind::INTEGRITY);
        let section = tables.next();
        if tables.next().is_some() {
            return Err(MediaPayloadError::DuplicateIntegrityTable);
        }
        match (header.flags.has_indexed_integrity(), section) {
            (false, None) => {}
            (false, Some(_)) => return Err(MediaPayloadError::UnexpectedIntegrityTable),
            (true, None) => return Err(MediaPayloadError::MissingIntegrityTable),
            (true, Some(section)) => {
                if !section.descriptor.flags.is_required() {
                    return Err(MediaPayloadError::IntegrityTableMustBeRequired);
                }
                let table =
                    IntegrityTable::open(section.bytes()).map_err(MediaPayloadError::Integrity)?;
                table
                    .validate_coverage(media)
                    .map_err(MediaPayloadError::Integrity)?;
                media.integrity = Some(table);
            }
        }
        Ok(media)
    }

    /// Validates all declared coverage, scanning DATA bodies exactly once.
    ///
    /// Metadata and inter-section padding are excluded. A successful metadata
    /// open alone does not establish DATA integrity.
    pub fn validate_data(self) -> Result<(), MediaPayloadError> {
        if let Some(table) = self.integrity {
            for range in table.iter() {
                self.validate_integrity_range(range)?;
            }
            return Ok(());
        }
        let expected = read_u32_le(self.payload, self.payload.len() - MEDIA_CRC_LEN)
            .expect("validated DATA checksum trailer");
        let actual = self.data_crc();
        if actual != expected {
            return Err(MediaPayloadError::DataCrcMismatch { expected, actual });
        }
        Ok(())
    }

    /// Returns indexed coverage, or `None` for the whole-DATA checksum form.
    pub const fn integrity(self) -> Option<IntegrityTable<'a>> {
        self.integrity
    }

    /// Verifies a payload-relative request contained in one DATA section.
    ///
    /// Returns the actual number of DATA bytes checksummed. Indexed coverage
    /// verifies only intersecting records; whole-DATA coverage scans all DATA.
    /// Empty requests check no DATA bytes. This operation does not perform I/O.
    pub fn validate_data_range(
        self,
        requested: core::ops::Range<u32>,
    ) -> Result<u32, MediaPayloadError> {
        let plan = self.data_check_plan(requested)?;
        let byte_len = plan.byte_len();
        plan.verify()?;
        Ok(byte_len)
    }

    fn validate_integrity_range(self, range: IntegrityRange) -> Result<(), MediaPayloadError> {
        let bytes = &self.payload[range.offset() as usize..range.range().end as usize];
        let actual = crate::crc32::compute(bytes);
        let expected = range.checksum();
        if actual != expected {
            return Err(MediaPayloadError::RangeCrcMismatch {
                offset: range.offset(),
                expected,
                actual,
            });
        }
        Ok(())
    }

    fn data_crc(self) -> u32 {
        let mut crc = Crc32::new();
        for section in self.sections_of_kind(MediaSectionKind::DATA) {
            crc.update(section.bytes());
        }
        crc.finish()
    }

    fn metadata_crc(self) -> u32 {
        let mut crc = Crc32::new();
        crc.update(&self.payload[..4]);
        let mut start = MEDIA_HEADER_LEN;
        for section in self.sections_of_kind(MediaSectionKind::DATA) {
            let offset = section.descriptor.offset as usize;
            crc.update(&self.payload[start..offset]);
            start = offset + section.bytes.len();
        }
        crc.update(&self.payload[start..]);
        crc.finish()
    }

    pub const fn header(self) -> MediaHeader {
        self.header
    }

    /// Borrows one directory entry by ordinal in constant time.
    ///
    /// The index is not the nth occurrence of a kind. DATA integrity remains a
    /// separate check, and no decoded directory array or allocation is created.
    ///
    /// ```
    /// use mirx::{image::{ColorDescription, RawImageAsset, SampleLayout, SurfaceDescriptor},
    ///     media::{MediaPayload, MediaSectionKind}};
    /// let surface = SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let bytes = RawImageAsset::new(surface, &[&[7]]).encode().unwrap();
    /// let media = MediaPayload::open(&bytes).unwrap();
    /// assert_eq!(media.get(0).unwrap().descriptor().kind(), MediaSectionKind::SURFACE);
    /// assert_eq!(media.get(1).unwrap().bytes(), &[7]);
    /// assert!(media.get(usize::MAX).is_none());
    /// media.validate_data().unwrap();
    /// ```
    pub fn get(self, index: usize) -> Option<MediaSection<'a>> {
        self.sections().nth(index)
    }

    pub fn sections(self) -> MediaSections<'a> {
        MediaSections {
            payload: self.payload,
            directory: self.directory,
            front: 0,
            back: usize::from(self.header.section_count),
            #[cfg(test)]
            reads: 0,
        }
    }

    pub fn sections_of_kind(self, kind: MediaSectionKind) -> MediaSectionsOfKind<'a> {
        MediaSectionsOfKind {
            sections: self.sections(),
            kind,
        }
    }

    pub fn section(self, kind: MediaSectionKind) -> Option<MediaSection<'a>> {
        self.sections_of_kind(kind).next()
    }

    /// Checks the real addresses of every DATA section for a runtime backend.
    pub fn data_addresses_are_aligned(self, alignment: usize) -> bool {
        self.sections_of_kind(MediaSectionKind::DATA)
            .all(|section| section.address_is_aligned(alignment))
    }

    /// Checks DATA file offsets against an explicit placement requirement.
    /// Per-plane and actual runtime addresses are separate typed checks.
    pub fn validate_file_alignment(
        self,
        payload_file_offset: u32,
        alignment: u32,
    ) -> Result<(), MediaPayloadError> {
        if !alignment.is_power_of_two() {
            return Err(MediaPayloadError::InvalidAlignment(alignment));
        }
        for section in self.sections_of_kind(MediaSectionKind::DATA) {
            let absolute_offset = payload_file_offset
                .checked_add(section.descriptor.offset)
                .ok_or(MediaPayloadError::SizeOverflow)?;
            if absolute_offset % alignment != 0 {
                return Err(MediaPayloadError::DataOffsetUnaligned {
                    index: section.index,
                    absolute_offset,
                    alignment,
                });
            }
        }
        Ok(())
    }
}

/// One borrowed section body and its validated descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaSection<'a> {
    index: u16,
    descriptor: MediaSectionDescriptor,
    bytes: &'a [u8],
}

impl<'a> MediaSection<'a> {
    pub const fn index(self) -> u16 {
        self.index
    }

    pub const fn descriptor(self) -> MediaSectionDescriptor {
        self.descriptor
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Checks the actual in-memory start address against a backend requirement.
    pub fn address_is_aligned(self, alignment: usize) -> bool {
        alignment.is_power_of_two() && (self.bytes.as_ptr() as usize) % alignment == 0
    }
}

/// Exact-size iterator over every section in physical order.
/// Direct forward/backward skips read only the selected descriptor.
#[derive(Clone, Debug)]
pub struct MediaSections<'a> {
    payload: &'a [u8],
    directory: &'a [u8],
    front: usize,
    back: usize,
    #[cfg(test)]
    reads: usize,
}

impl<'a> MediaSections<'a> {
    fn read(&mut self, index: usize) -> MediaSection<'a> {
        #[cfg(test)]
        {
            self.reads += 1;
        }
        let entry_start = index * MEDIA_SECTION_LEN;
        let descriptor = MediaSectionDescriptor::read(
            &self.directory[entry_start..entry_start + MEDIA_SECTION_LEN],
        )
        .expect("validated media section descriptor");
        let start = usize::try_from(descriptor.offset).expect("validated section offset");
        let size = usize::try_from(descriptor.size).expect("validated section size");
        MediaSection {
            index: u16::try_from(index).expect("section index fits u16"),
            descriptor,
            bytes: &self.payload[start..start + size],
        }
    }
}

impl<'a> Iterator for MediaSections<'a> {
    type Item = MediaSection<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let section = self.read(self.front);
        self.front += 1;
        Some(section)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.front = self.back;
            return None;
        }
        self.front += n;
        self.next()
    }

    fn count(self) -> usize {
        self.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for MediaSections<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.read(self.back))
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

impl ExactSizeIterator for MediaSections<'_> {
    fn len(&self) -> usize {
        self.back - self.front
    }
}

impl FusedIterator for MediaSections<'_> {}

/// Iterator over sections matching one open section kind.
#[derive(Clone, Debug)]
pub struct MediaSectionsOfKind<'a> {
    sections: MediaSections<'a>,
    kind: MediaSectionKind,
}

impl<'a> Iterator for MediaSectionsOfKind<'a> {
    type Item = MediaSection<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.sections
            .find(|section| section.descriptor.kind == self.kind)
    }
}

impl DoubleEndedIterator for MediaSectionsOfKind<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.sections
            .rfind(|section| section.descriptor.kind == self.kind)
    }
}

impl FusedIterator for MediaSectionsOfKind<'_> {}

fn validate_sections(
    covered: &[u8],
    directory: &[u8],
    section_count: u16,
    bodies_start: usize,
) -> Result<(), MediaPayloadError> {
    let minimum = u32::try_from(bodies_start).map_err(|_| MediaPayloadError::SizeOverflow)?;
    let covered_len = u32::try_from(covered.len()).map_err(|_| MediaPayloadError::SizeOverflow)?;
    let mut previous: Option<(u16, u32, u32)> = None;

    for index in 0..section_count {
        let entry_start = usize::from(index) * MEDIA_SECTION_LEN;
        let entry = &directory[entry_start..entry_start + MEDIA_SECTION_LEN];
        let Some(descriptor) = MediaSectionDescriptor::read(entry) else {
            return Err(MediaPayloadError::InvalidSectionKind { index });
        };
        if descriptor.offset < minimum {
            return Err(MediaPayloadError::SectionBeforeBodies {
                index,
                offset: descriptor.offset,
                minimum,
            });
        }
        let end = descriptor
            .offset
            .checked_add(descriptor.size)
            .ok_or(MediaPayloadError::SizeOverflow)?;
        if end > covered_len {
            return Err(MediaPayloadError::SectionOutOfBounds {
                index,
                offset: descriptor.offset,
                size: descriptor.size,
            });
        }

        if let Some((previous_index, previous_offset, previous_end)) = previous {
            if descriptor.offset < previous_offset {
                return Err(MediaPayloadError::SectionsOutOfOrder {
                    previous: previous_index,
                    next: index,
                });
            }
            if descriptor.offset < previous_end {
                return Err(MediaPayloadError::SectionsOverlap {
                    previous: previous_index,
                    next: index,
                });
            }
        }
        previous = Some((index, descriptor.offset, end));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn refresh_checksums(payload: &mut [u8]) {
    let Ok(media) = MediaPayload::parse(payload) else {
        return;
    };
    if let Some(table) = media.integrity() {
        let section_offset = media
            .section(MediaSectionKind::INTEGRITY)
            .unwrap()
            .descriptor()
            .offset() as usize;
        let checksums: alloc::vec::Vec<_> = table
            .iter()
            .map(|range| {
                crate::crc32(&payload[range.offset() as usize..range.range().end as usize])
            })
            .collect();
        for (index, checksum) in checksums.into_iter().enumerate() {
            let field = section_offset + index * INTEGRITY_RECORD_LEN + 8;
            payload[field..field + 4].copy_from_slice(&checksum.to_le_bytes());
        }
    } else {
        let data_crc = media.data_crc();
        let end = payload.len() - MEDIA_CRC_LEN;
        payload[end..].copy_from_slice(&data_crc.to_le_bytes());
    }
    let metadata_crc = MediaPayload::parse(payload).unwrap().metadata_crc();
    payload[4..8].copy_from_slice(&metadata_crc.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::wire::{write_u16_le, write_u32_le};

    #[test]
    fn direct_ordinals_and_skips_read_only_the_selected_descriptor() {
        let count = usize::from(u16::MAX);
        let start = (MEDIA_HEADER_LEN + MEDIA_SECTION_LEN * count) as u32;
        let sections: Vec<_> = (0..count)
            .map(|i| TestSection {
                kind: 0x8000 + (i % 32) as u16,
                flags: 0,
                offset: start,
                bytes: &[],
            })
            .collect();
        let bytes = payload(&sections, 0);
        let media = MediaPayload::open(&bytes).unwrap();
        for index in [0, 31, 1000, count - 1] {
            let section = media.get(index).unwrap();
            assert_eq!(usize::from(section.index()), index);
            assert_eq!(
                section.descriptor().kind().raw(),
                0x8000 + (index % 32) as u16
            );
        }
        assert_eq!(media.get(count), None);
        assert_eq!(media.get(usize::MAX), None);
        let mut iter = media.sections();
        assert_eq!(iter.nth(62_000).unwrap().index(), 62_000);
        assert_eq!(iter.reads, 1);
        assert_eq!(iter.nth_back(1000).unwrap().index(), 64_534);
        assert_eq!(iter.reads, 2);
        assert_eq!(iter.next().unwrap().index(), 62_001);
        assert_eq!(iter.next_back().unwrap().index(), 64_533);
        assert_eq!(iter.reads, 4);
        assert_eq!(iter.size_hint(), (2531, Some(2531)));
        assert_eq!(iter.nth(usize::MAX), None);
        assert_eq!(iter.reads, 4);
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.count(), 0);
        assert_eq!(media.sections().last().unwrap().index(), u16::MAX - 1);
        assert_eq!(media.sections().count(), count);
        let mut reversed = media.sections();
        assert_eq!(reversed.nth_back(usize::MAX), None);
        assert_eq!(reversed.reads, 0);
        assert_eq!(reversed.len(), 0);
    }

    #[test]
    fn directory_ordinals_do_not_mean_kind_occurrences_or_data_verification() {
        let sections = [
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 1,
                offset: 44,
                bytes: &[1],
            },
            TestSection {
                kind: MediaSectionKind::SURFACE.raw(),
                flags: 1,
                offset: 45,
                bytes: &[2],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 1,
                offset: 46,
                bytes: &[3],
            },
        ];
        let mut bytes = payload(&sections, 0);
        bytes[46] ^= 1;
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(
            media.get(1).unwrap().descriptor().kind(),
            MediaSectionKind::SURFACE
        );
        assert_eq!(media.get(2).unwrap().bytes().as_ptr(), bytes[46..].as_ptr());
        assert_eq!(media.section(MediaSectionKind::DATA).unwrap().index(), 0);
        assert_eq!(
            media
                .sections_of_kind(MediaSectionKind::DATA)
                .nth(1)
                .unwrap()
                .index(),
            2
        );
        assert!(media.validate_data().is_err());
        let empty = payload(&[], 0);
        let media = MediaPayload::open(&empty).unwrap();
        assert_eq!(media.get(0), None);
        assert_eq!(media.sections().next_back(), None);
        assert_eq!(media.sections().last(), None);
    }

    fn indexed_image() -> Vec<u8> {
        use crate::image::{ColorDescription, SampleLayout, SurfaceDescriptor};
        let mut bytes = vec![0; 108];
        bytes[0] = MEDIA_VERSION;
        bytes[1] = MediaFlags::INDEXED_INTEGRITY.bits();
        write_u16_le(&mut bytes, 2, 3);
        for (index, (kind, offset, size)) in [
            (MediaSectionKind::SURFACE, 44, 32),
            (MediaSectionKind::INTEGRITY, 76, 24),
            (MediaSectionKind::DATA, 100, 8),
        ]
        .into_iter()
        .enumerate()
        {
            let entry = 8 + index * 12;
            write_u16_le(&mut bytes, entry, kind.raw());
            write_u16_le(&mut bytes, entry + 2, 1);
            write_u32_le(&mut bytes, entry + 4, offset);
            write_u32_le(&mut bytes, entry + 8, size);
        }
        SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE)
            .unwrap()
            .encode_record_into(&mut bytes[44..76])
            .unwrap();
        bytes[100..].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        let ranges = [
            IntegrityRange::new(100..104, crate::crc32(&bytes[100..104])).unwrap(),
            IntegrityRange::new(104..108, crate::crc32(&bytes[104..108])).unwrap(),
        ];
        IntegrityTable::encode_into(&ranges, &mut bytes[76..100]).unwrap();
        let mut metadata = Crc32::new();
        metadata.update(&bytes[..4]);
        metadata.update(&bytes[8..100]);
        write_u32_le(&mut bytes, 4, metadata.finish());
        bytes
    }

    #[test]
    fn indexed_integrity_checks_only_intersecting_ranges_and_reports_cost() {
        let mut bytes = indexed_image();
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(media.integrity().unwrap().len(), 2);
        assert_eq!(media.validate_data_range(100..101), Ok(4));
        assert_eq!(media.validate_data_range(103..105), Ok(8));
        assert_eq!(media.validate_data_range(108..108), Ok(0));
        assert_eq!(
            media.validate_data_range(99..101),
            Err(MediaPayloadError::InvalidDataRange)
        );
        media.validate_data().unwrap();
        let image = crate::image::RawImageView::open(&bytes).unwrap();
        assert_eq!(image.plane(0).unwrap().bytes(), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(image.packed().is_some());
        bytes[105] ^= 1;
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(media.validate_data_range(100..101), Ok(4));
        assert!(matches!(
            media.validate_data_range(103..105),
            Err(MediaPayloadError::RangeCrcMismatch { offset: 104, .. })
        ));
        assert!(media.validate_data().is_err());
        assert!(crate::image::RawImageView::open(&bytes).is_err());
    }

    #[test]
    fn indexed_integrity_cannot_leave_gaps_cross_data_or_duplicate_coverage() {
        for (field, value) in [(76, 99), (76, 101), (80, 3), (80, 5), (88, 103), (92, 5)] {
            let mut bytes = indexed_image();
            write_u32_le(&mut bytes, field, value);
            assert!(
                matches!(
                    MediaPayload::open(&bytes),
                    Err(MediaPayloadError::Integrity(_))
                ),
                "field {field}, value {value}"
            );
        }
        let mut checksum = indexed_image();
        checksum[84] ^= 1;
        assert!(matches!(
            MediaPayload::open(&checksum),
            Err(MediaPayloadError::MetadataCrcMismatch { .. })
        ));

        let mut missing = indexed_image();
        write_u16_le(&mut missing, 20, 0x8000);
        assert_eq!(
            MediaPayload::open(&missing),
            Err(MediaPayloadError::MissingIntegrityTable)
        );
        let mut optional = indexed_image();
        write_u16_le(&mut optional, 22, 0);
        assert_eq!(
            MediaPayload::open(&optional),
            Err(MediaPayloadError::IntegrityTableMustBeRequired)
        );
        let mut duplicated = indexed_image();
        write_u16_le(&mut duplicated, 8, MediaSectionKind::INTEGRITY.raw());
        assert_eq!(
            MediaPayload::open(&duplicated),
            Err(MediaPayloadError::DuplicateIntegrityTable)
        );
        let mut unflagged = indexed_image();
        unflagged[1] = 0;
        unflagged.extend_from_slice(&[0; 4]);
        assert_eq!(
            MediaPayload::open(&unflagged),
            Err(MediaPayloadError::UnexpectedIntegrityTable)
        );
    }

    #[test]
    fn whole_data_integrity_reports_the_full_cost_of_a_partial_request() {
        let bytes = payload(
            &[
                TestSection {
                    kind: MediaSectionKind::DATA.raw(),
                    flags: 1,
                    offset: 40,
                    bytes: &[1; 8],
                },
                TestSection {
                    kind: MediaSectionKind::DATA.raw(),
                    flags: 1,
                    offset: 52,
                    bytes: &[2; 4],
                },
            ],
            0,
        );
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(media.integrity(), None);
        assert_eq!(media.validate_data_range(41..42), Ok(12));
        assert_eq!(
            media.validate_data_range(48..52),
            Err(MediaPayloadError::InvalidDataRange)
        );
    }

    #[test]
    fn indexed_raw_images_survive_document_placement_and_lossless_demotion() {
        let bytes = indexed_image();
        let mut document = crate::Document::new();
        let id = document
            .push_raw(crate::RawChunkInput::new(
                crate::ChunkType::IMAGE,
                bytes.as_slice(),
            ))
            .unwrap();
        document.set_primary(id).unwrap();
        let encoded = document.encode(&crate::EncodeOptions::new()).unwrap();
        let reader = crate::Reader::open(&encoded).unwrap();
        let chunk = reader.chunks().next().unwrap();
        assert_eq!(chunk.payload(), bytes);
        let image =
            crate::image::RawImageView::open_at(chunk.payload(), chunk.payload_offset()).unwrap();
        assert_eq!(image.plane(0).unwrap().bytes(), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(document.demote_to_flat().unwrap());
        assert_eq!(
            document.flat_image().unwrap().main(),
            &[1, 2, 3, 4, 5, 6, 7, 8]
        );
    }

    #[derive(Clone, Copy)]
    struct TestSection<'a> {
        kind: u16,
        flags: u16,
        offset: u32,
        bytes: &'a [u8],
    }

    fn payload(sections: &[TestSection<'_>], flags: u8) -> Vec<u8> {
        let directory_end = MEDIA_HEADER_LEN + sections.len() * MEDIA_SECTION_LEN;
        let body_end = sections.iter().fold(directory_end, |end, section| {
            end.max(section.offset as usize + section.bytes.len())
        });
        let payload_len = body_end + MEDIA_CRC_LEN;
        let mut out = vec![0; payload_len];
        out[0] = MEDIA_VERSION;
        out[1] = flags;
        write_u16_le(&mut out, 2, sections.len() as u16);

        for (index, section) in sections.iter().enumerate() {
            let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
            write_u16_le(&mut out, entry, section.kind);
            write_u16_le(&mut out, entry + 2, section.flags);
            write_u32_le(&mut out, entry + 4, section.offset);
            write_u32_le(&mut out, entry + 8, section.bytes.len() as u32);
            let start = section.offset as usize;
            out[start..start + section.bytes.len()].copy_from_slice(section.bytes);
        }
        refresh_checksums(&mut out);
        out
    }

    fn reseal(bytes: &mut [u8]) {
        // Malformed directory tests fail structurally before CRC validation.
        if MediaPayload::parse(bytes).is_ok() {
            refresh_checksums(bytes);
        }
    }

    #[test]
    fn section_directory_contains_only_identity_flags_and_range() {
        // Independent wire bytes keep reader/writer agreement from masking
        // accidental additions to the directory schema.
        let entry = [0x05, 0x00, 0x01, 0x80, 0x78, 0x56, 0x34, 0x12, 3, 0, 0, 0];
        assert_eq!(MEDIA_SECTION_LEN, entry.len());
        let descriptor = MediaSectionDescriptor::read(&entry).unwrap();
        assert_eq!(descriptor.kind(), MediaSectionKind::DATA);
        assert_eq!(descriptor.flags().bits(), 0x8001);
        assert_eq!(descriptor.offset(), 0x1234_5678);
        assert_eq!(descriptor.size(), 3);
        for end in 0..entry.len() {
            assert!(MediaSectionDescriptor::read(&entry[..end]).is_none());
        }
    }

    #[test]
    fn coding_and_section_ids_retain_unknown_values() {
        assert_eq!(CodingId::RAW.raw(), 0);
        assert_eq!(CodingId::new(0xbeef).raw(), 0xbeef);
        assert_eq!(MediaSectionKind::new(0), None);
        assert_eq!(
            MediaSectionKind::new(0x8001).map(|kind| kind.raw()),
            Some(0x8001)
        );
    }

    #[test]
    fn opens_and_borrows_ordered_sections_without_allocation() {
        let sections = [
            TestSection {
                kind: MediaSectionKind::SURFACE.raw(),
                flags: MediaSectionFlags::REQUIRED.bits(),
                offset: 80,
                bytes: &[1, 2, 3, 4],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 0xa500,
                offset: 96,
                bytes: &[5, 6, 7],
            },
        ];
        let bytes = payload(&sections, 0xa4);
        let media = MediaPayload::open(&bytes).unwrap();
        let header = media.header();
        assert_eq!(header.flags().bits(), 0xa4);
        assert_eq!(header.section_count(), 2);

        let mut iter = media.sections();
        assert_eq!(iter.len(), 2);
        let surface = iter.next().unwrap();
        assert_eq!(surface.index(), 0);
        assert_eq!(surface.descriptor().kind(), MediaSectionKind::SURFACE);
        assert!(surface.descriptor().flags().is_required());
        assert_eq!(surface.bytes(), &[1, 2, 3, 4]);
        assert_eq!(surface.bytes().as_ptr(), bytes[80..].as_ptr());

        let data = iter.next_back().unwrap();
        assert_eq!(data.index(), 1);
        assert_eq!(data.descriptor().flags().bits(), 0xa500);
        assert_eq!(data.descriptor().size(), 3);
        assert_eq!(data.bytes(), &[5, 6, 7]);
        assert!(iter.next().is_none());
        assert!(iter.next_back().is_none());
    }

    #[test]
    fn file_and_runtime_alignment_are_independent_checks() {
        let sections = [TestSection {
            kind: MediaSectionKind::DATA.raw(),
            flags: MediaSectionFlags::REQUIRED.bits(),
            offset: 64,
            bytes: &[1, 2, 3, 4],
        }];
        let bytes = payload(&sections, 0);
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(
            media.validate_file_alignment(4, 64),
            Err(MediaPayloadError::DataOffsetUnaligned {
                index: 0,
                absolute_offset: 68,
                alignment: 64,
            })
        );

        let data = media.section(MediaSectionKind::DATA).unwrap();
        assert_eq!(
            media.data_addresses_are_aligned(64),
            data.address_is_aligned(64)
        );
        assert!(!data.address_is_aligned(0));
        assert!(!data.address_is_aligned(3));
    }

    #[test]
    fn metadata_and_data_have_disjoint_integrity_coverage() {
        let sections = [
            TestSection {
                kind: MediaSectionKind::SURFACE.raw(),
                flags: 1,
                offset: 80,
                bytes: &[1; 4],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 1,
                offset: 96,
                bytes: &[2; 4],
            },
            TestSection {
                kind: 0x8000,
                flags: 0,
                offset: 108,
                bytes: &[3; 4],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 1,
                offset: 120,
                bytes: &[4; 4],
            },
        ];
        let base = payload(&sections, 0);
        MediaPayload::open(&base).unwrap().validate_data().unwrap();
        for offset in 0..base.len() {
            let mut changed = base.clone();
            changed[offset] ^= 1;
            let metadata = MediaPayload::open(&changed);
            if (96..100).contains(&offset) || (120..124).contains(&offset) {
                assert!(metadata.is_ok(), "DATA byte {offset}");
                assert!(matches!(
                    metadata.unwrap().validate_data(),
                    Err(MediaPayloadError::DataCrcMismatch { .. })
                ));
            } else {
                assert!(metadata.is_err(), "metadata or padding byte {offset}");
            }
        }
    }

    #[test]
    fn header_has_only_identity_count_and_metadata_checksum() {
        let bytes = payload(&[], 0);
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[..4], &[1, 0, 0, 0]);
        assert_eq!(&bytes[8..], &[0; 4]); // CRC32 of no DATA.
        let media = MediaPayload::open(&bytes).unwrap();
        assert_eq!(media.header().metadata_crc32(), media.metadata_crc());
        media.validate_data().unwrap();
    }

    #[test]
    fn rejects_invalid_section_ranges_and_order() {
        let valid = [
            TestSection {
                kind: MediaSectionKind::SURFACE.raw(),
                flags: 0,
                offset: 80,
                bytes: &[0; 8],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 0,
                offset: 96,
                bytes: &[0; 4],
            },
        ];
        let base = payload(&valid, 0);

        let mut zero_kind = base.clone();
        write_u16_le(&mut zero_kind, MEDIA_HEADER_LEN, 0);
        reseal(&mut zero_kind);
        assert_eq!(
            MediaPayload::open(&zero_kind),
            Err(MediaPayloadError::InvalidSectionKind { index: 0 })
        );

        let mut before = base.clone();
        write_u32_le(&mut before, MEDIA_HEADER_LEN + 4, 31);
        reseal(&mut before);
        assert_eq!(
            MediaPayload::open(&before),
            Err(MediaPayloadError::SectionBeforeBodies {
                index: 0,
                offset: 31,
                minimum: 32,
            })
        );

        let mut overlap = base.clone();
        write_u32_le(&mut overlap, MEDIA_HEADER_LEN + MEDIA_SECTION_LEN + 4, 84);
        reseal(&mut overlap);
        assert_eq!(
            MediaPayload::open(&overlap),
            Err(MediaPayloadError::SectionsOverlap {
                previous: 0,
                next: 1,
            })
        );

        let mut reversed = base.clone();
        write_u32_le(&mut reversed, MEDIA_HEADER_LEN + MEDIA_SECTION_LEN + 4, 72);
        reseal(&mut reversed);
        assert_eq!(
            MediaPayload::open(&reversed),
            Err(MediaPayloadError::SectionsOutOfOrder {
                previous: 0,
                next: 1,
            })
        );

        let mut outside = base.clone();
        write_u32_le(&mut outside, MEDIA_HEADER_LEN + MEDIA_SECTION_LEN + 8, 64);
        reseal(&mut outside);
        assert_eq!(
            MediaPayload::open(&outside),
            Err(MediaPayloadError::SectionOutOfBounds {
                index: 1,
                offset: 96,
                size: 64,
            })
        );
    }

    #[test]
    fn unknown_version_is_rejected_before_header_length() {
        assert_eq!(
            MediaPayload::open(&[2]),
            Err(MediaPayloadError::UnsupportedVersion(2))
        );
    }

    #[test]
    fn every_header_prefix_truncation_is_reported() {
        let bytes = payload(&[], 0);
        for available in 0..MEDIA_HEADER_LEN + MEDIA_CRC_LEN {
            assert!(matches!(
                MediaPayload::open(&bytes[..available]),
                Err(MediaPayloadError::Truncated { .. })
                    | Err(MediaPayloadError::UnsupportedVersion(_))
            ));
        }
    }
}
