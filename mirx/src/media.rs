//! Common wire vocabulary for sectioned media payloads.
//!
//! IMAGE and FONT use the same fixed header, section directory, coding
//! identifiers, checksum boundary, and alignment checks. Typed payload modules
//! define the contents of each section.

use core::iter::FusedIterator;

use crate::payload::envelope::{Envelope, EnvelopeError};
use crate::wire::{read_u16_le, read_u32_le};

pub const MEDIA_HEADER_LEN: usize = 32;
pub const MEDIA_SECTION_LEN: usize = 16;
pub const MEDIA_CRC_LEN: usize = 4;
pub const MEDIA_VERSION: u8 = 1;

/// Open identifier for one stored coding profile.
///
/// Unknown values remain representable so raw editing never depends on the
/// decoder profiles compiled into the current application.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodingId(u16);

impl CodingId {
    /// Uncoded bytes described directly by the payload's memory records.
    pub const RAW: Self = Self(0);

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
    pub const CODING_PARAMS: Self = Self(0x0003);
    pub const ACCESS_UNITS: Self = Self(0x0004);
    pub const DATA: Self = Self(0x0005);

    pub const FACE: Self = Self(0x0010);
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

/// Decoded common header of an IMAGE or FONT payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaHeader {
    flags: MediaFlags,
    section_count: u16,
    required_alignment_log2: u8,
    payload_size: u32,
    decoded_bytes_bound: u32,
    scratch_bytes_bound: u32,
    default_coding: CodingId,
    profile_revision: u16,
}

impl MediaHeader {
    pub const fn flags(self) -> MediaFlags {
        self.flags
    }

    pub const fn section_count(self) -> u16 {
        self.section_count
    }

    pub const fn required_alignment_log2(self) -> u8 {
        self.required_alignment_log2
    }

    pub const fn required_alignment(self) -> u32 {
        1u32 << self.required_alignment_log2
    }

    pub const fn payload_size(self) -> u32 {
        self.payload_size
    }

    pub const fn decoded_bytes_bound(self) -> u32 {
        self.decoded_bytes_bound
    }

    pub const fn scratch_bytes_bound(self) -> u32 {
        self.scratch_bytes_bound
    }

    pub const fn default_coding(self) -> CodingId {
        self.default_coding
    }

    pub const fn profile_revision(self) -> u16 {
        self.profile_revision
    }

    fn read(bytes: &[u8]) -> Self {
        Self {
            flags: MediaFlags::from_bits_retain(bytes[1]),
            section_count: read_u16_le(bytes, 2).expect("validated media header"),
            required_alignment_log2: bytes[6],
            payload_size: read_u32_le(bytes, 12).expect("validated media header"),
            decoded_bytes_bound: read_u32_le(bytes, 16).expect("validated media header"),
            scratch_bytes_bound: read_u32_le(bytes, 20).expect("validated media header"),
            default_coding: CodingId::new(read_u16_le(bytes, 24).expect("validated media header")),
            profile_revision: read_u16_le(bytes, 26).expect("validated media header"),
        }
    }
}

/// Decoded directory record for one section body.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaSectionDescriptor {
    kind: MediaSectionKind,
    flags: MediaSectionFlags,
    offset: u32,
    stored_size: u32,
    decoded_size: u32,
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

    pub const fn stored_size(self) -> u32 {
        self.stored_size
    }

    pub const fn decoded_size(self) -> u32 {
        self.decoded_size
    }

    fn read(bytes: &[u8]) -> Option<Self> {
        Some(Self {
            kind: MediaSectionKind::new(read_u16_le(bytes, 0)?)?,
            flags: MediaSectionFlags::from_bits_retain(read_u16_le(bytes, 2)?),
            offset: read_u32_le(bytes, 4)?,
            stored_size: read_u32_le(bytes, 8)?,
            decoded_size: read_u32_le(bytes, 12)?,
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
    ReservedNonZero {
        offset: usize,
    },
    SectionTableOffsetMismatch {
        expected: u32,
        actual: u32,
    },
    SectionEntrySizeMismatch {
        expected: u16,
        actual: u16,
    },
    RequiredAlignmentTooLarge {
        log2: u8,
    },
    PayloadLengthMismatch {
        expected: usize,
        actual: usize,
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
    CrcMismatch {
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
}

impl<'a> MediaPayload<'a> {
    /// Opens a payload without assuming where it sits in an outer MIRX file.
    ///
    /// Use [`Self::open_at`] when validating the payload's file-relative DATA
    /// alignment promise.
    pub fn open(payload: &'a [u8]) -> Result<Self, MediaPayloadError> {
        let envelope = Envelope::open_v1(payload, MEDIA_HEADER_LEN).map_err(map_envelope_error)?;
        let covered = envelope.covered();
        validate_header_bytes(covered)?;
        let header = MediaHeader::read(covered);
        let expected =
            usize::try_from(header.payload_size).map_err(|_| MediaPayloadError::SizeOverflow)?;
        if expected != payload.len() {
            return Err(MediaPayloadError::PayloadLengthMismatch {
                expected,
                actual: payload.len(),
            });
        }

        let directory_len = usize::from(header.section_count)
            .checked_mul(MEDIA_SECTION_LEN)
            .ok_or(MediaPayloadError::SizeOverflow)?;
        let directory_end = MEDIA_HEADER_LEN
            .checked_add(directory_len)
            .ok_or(MediaPayloadError::SizeOverflow)?;
        if directory_end > covered.len() {
            return Err(MediaPayloadError::SectionTableOutOfBounds);
        }
        let directory = &covered[MEDIA_HEADER_LEN..directory_end];
        validate_sections(covered, directory, header.section_count, directory_end)?;

        let exact = envelope
            .validate_exact_end(covered.len())
            .map_err(map_envelope_error)?;
        exact.validate_crc().map_err(map_envelope_error)?;
        Ok(Self {
            payload,
            header,
            directory,
        })
    }

    /// Opens a payload and validates every DATA section's alignment relative
    /// to the beginning of the outer MIRX file.
    pub fn open_at(payload: &'a [u8], payload_file_offset: u32) -> Result<Self, MediaPayloadError> {
        let media = Self::open(payload)?;
        media.validate_file_alignment(payload_file_offset)?;
        Ok(media)
    }

    pub const fn header(self) -> MediaHeader {
        self.header
    }

    pub fn sections(self) -> MediaSections<'a> {
        MediaSections {
            payload: self.payload,
            directory: self.directory,
            front: 0,
            back: usize::from(self.header.section_count),
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

    pub fn validate_file_alignment(
        self,
        payload_file_offset: u32,
    ) -> Result<(), MediaPayloadError> {
        let alignment = self.header.required_alignment();
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Debug)]
pub struct MediaSections<'a> {
    payload: &'a [u8],
    directory: &'a [u8],
    front: usize,
    back: usize,
}

impl<'a> MediaSections<'a> {
    fn read(&self, index: usize) -> MediaSection<'a> {
        let entry_start = index * MEDIA_SECTION_LEN;
        let descriptor = MediaSectionDescriptor::read(
            &self.directory[entry_start..entry_start + MEDIA_SECTION_LEN],
        )
        .expect("validated media section descriptor");
        let start = usize::try_from(descriptor.offset).expect("validated section offset");
        let size = usize::try_from(descriptor.stored_size).expect("validated section size");
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

fn validate_header_bytes(bytes: &[u8]) -> Result<(), MediaPayloadError> {
    let section_entry_size = read_u16_le(bytes, 4).expect("validated media header");
    if section_entry_size != MEDIA_SECTION_LEN as u16 {
        return Err(MediaPayloadError::SectionEntrySizeMismatch {
            expected: MEDIA_SECTION_LEN as u16,
            actual: section_entry_size,
        });
    }
    let section_table_offset = read_u32_le(bytes, 8).expect("validated media header");
    if section_table_offset != MEDIA_HEADER_LEN as u32 {
        return Err(MediaPayloadError::SectionTableOffsetMismatch {
            expected: MEDIA_HEADER_LEN as u32,
            actual: section_table_offset,
        });
    }
    if bytes[6] > 31 {
        return Err(MediaPayloadError::RequiredAlignmentTooLarge { log2: bytes[6] });
    }
    for offset in [7, 28, 29, 30, 31] {
        if bytes[offset] != 0 {
            return Err(MediaPayloadError::ReservedNonZero { offset });
        }
    }
    Ok(())
}

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
            .checked_add(descriptor.stored_size)
            .ok_or(MediaPayloadError::SizeOverflow)?;
        if end > covered_len {
            return Err(MediaPayloadError::SectionOutOfBounds {
                index,
                offset: descriptor.offset,
                size: descriptor.stored_size,
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

fn map_envelope_error(error: EnvelopeError) -> MediaPayloadError {
    match error {
        EnvelopeError::Truncated { needed, available } => {
            MediaPayloadError::Truncated { needed, available }
        }
        EnvelopeError::UnsupportedVersion(version) => {
            MediaPayloadError::UnsupportedVersion(version)
        }
        EnvelopeError::PayloadLengthMismatch { expected, actual } => {
            MediaPayloadError::PayloadLengthMismatch { expected, actual }
        }
        EnvelopeError::CrcMismatch { expected, actual } => {
            MediaPayloadError::CrcMismatch { expected, actual }
        }
        EnvelopeError::SizeOverflow => MediaPayloadError::SizeOverflow,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::crc32;
    use crate::wire::{write_u16_le, write_u32_le};

    #[derive(Clone, Copy)]
    struct TestSection<'a> {
        kind: u16,
        flags: u16,
        offset: u32,
        decoded_size: u32,
        bytes: &'a [u8],
    }

    fn payload(
        sections: &[TestSection<'_>],
        alignment_log2: u8,
        flags: u8,
        coding: u16,
    ) -> Vec<u8> {
        let directory_end = MEDIA_HEADER_LEN + sections.len() * MEDIA_SECTION_LEN;
        let body_end = sections.iter().fold(directory_end, |end, section| {
            end.max(section.offset as usize + section.bytes.len())
        });
        let payload_len = body_end + MEDIA_CRC_LEN;
        let mut out = vec![0; payload_len];
        out[0] = MEDIA_VERSION;
        out[1] = flags;
        write_u16_le(&mut out, 2, sections.len() as u16);
        write_u16_le(&mut out, 4, MEDIA_SECTION_LEN as u16);
        out[6] = alignment_log2;
        write_u32_le(&mut out, 8, MEDIA_HEADER_LEN as u32);
        write_u32_le(&mut out, 12, payload_len as u32);
        write_u32_le(&mut out, 16, 4096);
        write_u32_le(&mut out, 20, 128);
        write_u16_le(&mut out, 24, coding);
        write_u16_le(&mut out, 26, 3);

        for (index, section) in sections.iter().enumerate() {
            let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
            write_u16_le(&mut out, entry, section.kind);
            write_u16_le(&mut out, entry + 2, section.flags);
            write_u32_le(&mut out, entry + 4, section.offset);
            write_u32_le(&mut out, entry + 8, section.bytes.len() as u32);
            write_u32_le(&mut out, entry + 12, section.decoded_size);
            let start = section.offset as usize;
            out[start..start + section.bytes.len()].copy_from_slice(section.bytes);
        }
        let crc_offset = out.len() - MEDIA_CRC_LEN;
        let crc = crc32::compute(&out[..crc_offset]);
        out[crc_offset..].copy_from_slice(&crc.to_le_bytes());
        out
    }

    fn reseal(bytes: &mut [u8]) {
        let crc_offset = bytes.len() - MEDIA_CRC_LEN;
        let crc = crc32::compute(&bytes[..crc_offset]);
        bytes[crc_offset..].copy_from_slice(&crc.to_le_bytes());
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
                decoded_size: 4,
                bytes: &[1, 2, 3, 4],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 0xa500,
                offset: 96,
                decoded_size: 12,
                bytes: &[5, 6, 7],
            },
        ];
        let bytes = payload(&sections, 4, 0xa5, 0xbeef);
        let media = MediaPayload::open_at(&bytes, 0).unwrap();
        let header = media.header();
        assert_eq!(header.flags().bits(), 0xa5);
        assert_eq!(header.section_count(), 2);
        assert_eq!(header.required_alignment(), 16);
        assert_eq!(header.default_coding(), CodingId::new(0xbeef));
        assert_eq!(header.profile_revision(), 3);

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
        assert_eq!(data.descriptor().decoded_size(), 12);
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
            decoded_size: 4,
            bytes: &[1, 2, 3, 4],
        }];
        let bytes = payload(&sections, 6, 0, CodingId::RAW.raw());
        let media = MediaPayload::open_at(&bytes, 0).unwrap();
        assert_eq!(
            MediaPayload::open_at(&bytes, 4),
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
    fn rejects_header_schema_and_reserved_bytes_before_sections() {
        let base = payload(&[], 0, 0, 0);

        let mut entry_size = base.clone();
        write_u16_le(&mut entry_size, 4, 12);
        reseal(&mut entry_size);
        assert_eq!(
            MediaPayload::open(&entry_size),
            Err(MediaPayloadError::SectionEntrySizeMismatch {
                expected: 16,
                actual: 12,
            })
        );

        let mut table_offset = base.clone();
        write_u32_le(&mut table_offset, 8, 36);
        reseal(&mut table_offset);
        assert_eq!(
            MediaPayload::open(&table_offset),
            Err(MediaPayloadError::SectionTableOffsetMismatch {
                expected: 32,
                actual: 36,
            })
        );

        for offset in [7, 28, 29, 30, 31] {
            let mut reserved = base.clone();
            reserved[offset] = 1;
            reseal(&mut reserved);
            assert_eq!(
                MediaPayload::open(&reserved),
                Err(MediaPayloadError::ReservedNonZero { offset })
            );
        }
    }

    #[test]
    fn rejects_invalid_section_ranges_and_order() {
        let valid = [
            TestSection {
                kind: MediaSectionKind::SURFACE.raw(),
                flags: 0,
                offset: 80,
                decoded_size: 8,
                bytes: &[0; 8],
            },
            TestSection {
                kind: MediaSectionKind::DATA.raw(),
                flags: 0,
                offset: 96,
                decoded_size: 4,
                bytes: &[0; 4],
            },
        ];
        let base = payload(&valid, 0, 0, 0);

        let mut zero_kind = base.clone();
        write_u16_le(&mut zero_kind, MEDIA_HEADER_LEN, 0);
        reseal(&mut zero_kind);
        assert_eq!(
            MediaPayload::open(&zero_kind),
            Err(MediaPayloadError::InvalidSectionKind { index: 0 })
        );

        let mut before = base.clone();
        write_u32_le(&mut before, MEDIA_HEADER_LEN + 4, 63);
        reseal(&mut before);
        assert_eq!(
            MediaPayload::open(&before),
            Err(MediaPayloadError::SectionBeforeBodies {
                index: 0,
                offset: 63,
                minimum: 64,
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
    fn exact_payload_size_and_crc_are_enforced() {
        let mut bytes = payload(&[], 0, 0, 0);
        let actual_len = bytes.len();
        write_u32_le(&mut bytes, 12, (actual_len + 1) as u32);
        reseal(&mut bytes);
        assert_eq!(
            MediaPayload::open(&bytes),
            Err(MediaPayloadError::PayloadLengthMismatch {
                expected: actual_len + 1,
                actual: actual_len,
            })
        );

        write_u32_le(&mut bytes, 12, actual_len as u32);
        reseal(&mut bytes);
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        assert!(matches!(
            MediaPayload::open(&bytes),
            Err(MediaPayloadError::CrcMismatch { .. })
        ));
    }

    #[test]
    fn every_header_prefix_truncation_is_reported() {
        let bytes = payload(&[], 0, 0, 0);
        for available in 0..MEDIA_HEADER_LEN + MEDIA_CRC_LEN {
            assert!(matches!(
                MediaPayload::open(&bytes[..available]),
                Err(MediaPayloadError::Truncated { .. })
                    | Err(MediaPayloadError::UnsupportedVersion(_))
            ));
        }
    }
}
