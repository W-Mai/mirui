//! Common wire vocabulary for sectioned media payloads.
//!
//! Sectioned payloads share a fixed header, directory, coding identifiers,
//! and independent metadata/DATA checksum coverage. Typed payload modules
//! define section contents and physical storage requirements.

use core::iter::FusedIterator;

pub(crate) mod coding;
mod data_scan;
pub(crate) mod index;
pub(crate) mod integrity;
mod layout;
pub use data_scan::WholeDataScan;
pub use layout::{MediaLayout, SectionScan};
mod metadata_scan;
use metadata_scan::MetadataCoverage;
pub use metadata_scan::MetadataScan;
pub(crate) mod selection;
pub(crate) use coding::{CODING_RECORD_LEN, CodingRecord, CodingTable, CodingTableError};
pub(crate) use index::{UnitIndex, UnitIndexEncoding, UnitIndexError};
pub(crate) use integrity::DataIntegrity;
pub use integrity::{
    DataCheckPlan, INTEGRITY_RECORD_LEN, IntegrityError, IntegrityRange, IntegrityRanges,
    IntegrityTable,
};
pub(crate) use selection::{UnitSelection, UnitSelectionEncoding, UnitSelectionError};

use crate::crc32::Crc32;
use crate::wire::{read_u16_le, read_u32_le};

pub const MEDIA_HEADER_LEN: usize = 8;
pub const MEDIA_SECTION_LEN: usize = 12;
pub const MEDIA_CRC_LEN: usize = 4;
pub const MEDIA_VERSION: u8 = 1;

pub(crate) mod output;

/// Open media-payload flags.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MediaFlags(u8);

impl MediaFlags {
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

    pub const FACE: Self = Self(0x0010);
    pub const CMAP_INDEX: Self = Self(0x0011);
    pub const REPRESENTATIONS: Self = Self(0x0012);
    pub const ADVANCES: Self = Self(0x0013);
    pub const ATLAS_MAPS: Self = Self(0x0014);
    pub const SURFACE_GROUPS: Self = Self(0x0015);
    pub const GLYPH_IDS: Self = Self(0x0016);
    pub const RASTER_METRICS: Self = Self(0x0017);
    pub const SHAPING: Self = Self(0x0018);

    pub const SEQUENCE: Self = Self(0x0021);
    pub const FRAME_MAP: Self = Self(0x0022);
    pub const FRAME_TIMING: Self = Self(0x0023);
    pub const FRAME_COMPOSITION: Self = Self(0x0024);
    pub const KEYFRAME_INDEX: Self = Self(0x0025);

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
    #[cfg(test)]
    const fn metadata_crc32(self) -> u32 {
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
        alignment: crate::ByteAlignment,
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
        let layout = MediaLayout::parse(payload, payload.len())?;
        let header = layout.header();
        let directory = &payload[layout.directory_range()];
        let mut scan = layout.sections();
        for record in directory.chunks_exact(MEDIA_SECTION_LEN) {
            scan.accept(record)?;
        }
        let mut media = Self {
            payload,
            header,
            directory,
            integrity: None,
        };
        let mut tables = media.sections_of_kind(MediaSectionKind::INTEGRITY);
        let section = tables.next();
        integrity::check_table(
            header.flags.has_indexed_integrity(),
            section.map(|section| section.descriptor()),
            tables.next().is_some(),
        )?;
        if let Some(section) = section {
            let table =
                IntegrityTable::open(section.bytes()).map_err(MediaPayloadError::Integrity)?;
            table
                .validate_coverage(media)
                .map_err(MediaPayloadError::Integrity)?;
            media.integrity = Some(table);
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
        data_scan::verify_checksum(expected, actual)
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
        let mut check = range.check();
        check.accept(bytes)?;
        check.finish()
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
        let mut coverage = MetadataCoverage::new(self.payload.len());
        for section in self.sections_of_kind(MediaSectionKind::DATA) {
            crc.update(&self.payload[coverage.before_data(section.descriptor)]);
        }
        crc.update(&self.payload[coverage.tail()]);
        crc.finish()
    }

    pub const fn header(self) -> MediaHeader {
        self.header
    }

    /// Borrows one directory entry by ordinal in constant time.
    ///
    /// The index is not the nth occurrence of a kind. DATA integrity remains a
    /// separate check, and no decoded directory array or allocation is created.
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

    #[cfg(test)]
    fn data_addresses_are_aligned(self, alignment: crate::ByteAlignment) -> bool {
        self.sections_of_kind(MediaSectionKind::DATA)
            .all(|section| section.address_is_aligned(alignment))
    }

    /// Checks DATA file offsets against an explicit placement requirement.
    /// Per-plane and actual runtime addresses are separate typed checks.
    pub fn validate_file_alignment(
        self,
        payload_file_offset: u32,
        alignment: crate::ByteAlignment,
    ) -> Result<(), MediaPayloadError> {
        for section in self.sections_of_kind(MediaSectionKind::DATA) {
            let absolute_offset = payload_file_offset
                .checked_add(section.descriptor.offset)
                .ok_or(MediaPayloadError::SizeOverflow)?;
            if absolute_offset % alignment.get() != 0 {
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
    #[cfg(test)]
    const fn index(self) -> u16 {
        self.index
    }

    pub const fn descriptor(self) -> MediaSectionDescriptor {
        self.descriptor
    }

    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    #[cfg(test)]
    fn address_is_aligned(self, alignment: crate::ByteAlignment) -> bool {
        let alignment = usize::try_from(alignment.get()).expect("u32 fits usize");
        (self.bytes.as_ptr() as usize) % alignment == 0
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
#[path = "../tests/unit/media/tests.rs"]
mod tests;
