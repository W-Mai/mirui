#![doc = include_str!("../../docs/glyph-surfaces.md")]

use super::{GlyphMap, GlyphMapError, GlyphPacking};
use crate::image::SampleLayout;
use crate::media::{MediaPayload, MediaSectionFlags, MediaSectionKind};
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

pub const GLYPH_SURFACE_RECORD_LEN: usize = 24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Storage {
    Raw {
        planes: Option<u16>,
    },
    Encoded {
        codings: u16,
        groups: Option<(u16, Option<u16>)>,
    },
}

/// Shared glyph geometry and references into a common media directory.
///
/// RAW physical planes and encoded input groups are mutually exclusive.
/// Sample interpretation, section bodies and DATA integrity require binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GlyphSurfaceRecord {
    layout: SampleLayout,
    packing: GlyphPacking,
    width: u32,
    height: u32,
    data: u16,
    storage: Storage,
}

impl GlyphSurfaceRecord {
    /// Creates tight RAW storage. Unknown sample identifiers remain representable.
    /// Width and height describe one cell or the complete atlas, respectively.
    pub fn new(
        layout: SampleLayout,
        packing: GlyphPacking,
        width: u32,
        height: u32,
        data_section: u16,
    ) -> Result<Self, GlyphSurfaceRecordError> {
        SectionRef::new(data_section, MediaSectionKind::DATA)?;
        if packing == GlyphPacking::GlyphMajor {
            GlyphMap::glyph_major(width, height, 0).map_err(GlyphSurfaceRecordError::Map)?;
        }
        Ok(Self {
            layout,
            packing,
            width,
            height,
            data: data_section,
            storage: Storage::Raw { planes: None },
        })
    }

    /// Attaches one shared RAW allocation record; encoded storage rejects it.
    pub fn with_planes(mut self, section: u16) -> Result<Self, GlyphSurfaceRecordError> {
        SectionRef::new(section, MediaSectionKind::PLANES)?;
        let Storage::Raw { planes } = &mut self.storage else {
            return Err(GlyphSurfaceRecordError::ConflictingStorage);
        };
        *planes = Some(section);
        Ok(self)
    }

    /// Selects encoded storage without a RAW physical allocation record.
    /// Existing groups retain their references when replacing a coding table.
    pub fn with_codings(mut self, section: u16) -> Result<Self, GlyphSurfaceRecordError> {
        SectionRef::new(section, MediaSectionKind::CODINGS)?;
        match &mut self.storage {
            Storage::Raw { planes: Some(_) } => {
                return Err(GlyphSurfaceRecordError::ConflictingStorage);
            }
            Storage::Raw { planes: None } => {
                self.storage = Storage::Encoded {
                    codings: section,
                    groups: None,
                };
            }
            Storage::Encoded { codings, .. } => *codings = section,
        }
        Ok(self)
    }

    /// Attaches encoded groups together with their optional shared index body.
    pub fn with_groups(
        mut self,
        section: u16,
        index_section: Option<u16>,
    ) -> Result<Self, GlyphSurfaceRecordError> {
        SectionRef::new(section, MediaSectionKind::UNIT_GROUPS)?;
        if let Some(index) = index_section {
            SectionRef::new(index, MediaSectionKind::UNIT_INDEX)?;
        }
        let Storage::Encoded { groups, .. } = &mut self.storage else {
            return Err(GlyphSurfaceRecordError::MissingCodings);
        };
        *groups = Some((section, index_section));
        Ok(self)
    }

    pub const fn sample_layout(self) -> SampleLayout {
        self.layout
    }
    pub const fn packing(self) -> GlyphPacking {
        self.packing
    }
    pub const fn width(self) -> u32 {
        self.width
    }
    pub const fn height(self) -> u32 {
        self.height
    }
    pub const fn data_section(self) -> u16 {
        self.data
    }

    pub const fn planes_section(self) -> Option<u16> {
        match self.storage {
            Storage::Raw { planes } => planes,
            Storage::Encoded { .. } => None,
        }
    }

    pub const fn codings_section(self) -> Option<u16> {
        match self.storage {
            Storage::Raw { .. } => None,
            Storage::Encoded { codings, .. } => Some(codings),
        }
    }

    pub const fn groups_section(self) -> Option<u16> {
        match self.storage {
            Storage::Encoded {
                groups: Some((groups, _)),
                ..
            } => Some(groups),
            _ => None,
        }
    }

    pub const fn index_section(self) -> Option<u16> {
        match self.storage {
            Storage::Encoded {
                groups: Some((_, index)),
                ..
            } => index,
            _ => None,
        }
    }

    /// Derives logical dimensions through the shared checked glyph-map geometry.
    /// Atlas dimensions do not depend on glyph count.
    pub fn logical_extent(self, glyph_count: usize) -> Result<(u32, u32), GlyphSurfaceRecordError> {
        match self.packing {
            GlyphPacking::GlyphMajor => {
                let map = GlyphMap::glyph_major(self.width, self.height, glyph_count)
                    .map_err(GlyphSurfaceRecordError::Map)?;
                Ok((map.width(), map.height()))
            }
            GlyphPacking::Atlas2D => Ok((self.width, self.height)),
        }
    }

    /// Reads one canonical 24-byte prefix without resolving section references.
    pub fn from_record(bytes: &[u8]) -> Result<Self, GlyphSurfaceRecordError> {
        if bytes.len() < GLYPH_SURFACE_RECORD_LEN {
            return Err(GlyphSurfaceRecordError::Truncated {
                needed: GLYPH_SURFACE_RECORD_LEN,
                available: bytes.len(),
            });
        }
        for offset in [3, 22, 23] {
            if bytes[offset] != 0 {
                return Err(GlyphSurfaceRecordError::ReservedNonZero { offset });
            }
        }
        let packing = match bytes[2] {
            0 => GlyphPacking::GlyphMajor,
            1 => GlyphPacking::Atlas2D,
            value => return Err(GlyphSurfaceRecordError::UnknownPacking(value)),
        };
        let mut record = Self::new(
            SampleLayout::new(read_u16_le(bytes, 0).unwrap()),
            packing,
            read_u32_le(bytes, 4).unwrap(),
            read_u32_le(bytes, 8).unwrap(),
            read_u16_le(bytes, 12).unwrap(),
        )?;
        let planes = read_u16_le(bytes, 14).unwrap();
        let codings = read_u16_le(bytes, 16).unwrap();
        let groups = read_u16_le(bytes, 18).unwrap();
        let index = read_u16_le(bytes, 20).unwrap();
        if planes != u16::MAX {
            record = record.with_planes(planes)?;
        }
        if codings != u16::MAX {
            record = record.with_codings(codings)?;
        }
        if groups != u16::MAX {
            record = record.with_groups(groups, (index != u16::MAX).then_some(index))?;
        } else if index != u16::MAX {
            return Err(GlyphSurfaceRecordError::MissingGroups);
        }
        Ok(record)
    }

    /// Emits canonical references, retaining any output suffix unchanged.
    /// Insufficient capacity leaves all output untouched.
    pub fn encode_record_into(self, out: &mut [u8]) -> Result<usize, GlyphSurfaceRecordError> {
        if out.len() < GLYPH_SURFACE_RECORD_LEN {
            return Err(GlyphSurfaceRecordError::BufferTooSmall {
                needed: GLYPH_SURFACE_RECORD_LEN,
                available: out.len(),
            });
        }
        let mut bytes = [0; GLYPH_SURFACE_RECORD_LEN];
        write_u16_le(&mut bytes, 0, self.layout.raw());
        bytes[2] = match self.packing {
            GlyphPacking::GlyphMajor => 0,
            GlyphPacking::Atlas2D => 1,
        };
        write_u32_le(&mut bytes, 4, self.width);
        write_u32_le(&mut bytes, 8, self.height);
        write_u16_le(&mut bytes, 12, self.data);
        write_u16_le(&mut bytes, 14, self.planes_section().unwrap_or(u16::MAX));
        write_u16_le(&mut bytes, 16, self.codings_section().unwrap_or(u16::MAX));
        write_u16_le(&mut bytes, 18, self.groups_section().unwrap_or(u16::MAX));
        write_u16_le(&mut bytes, 20, self.index_section().unwrap_or(u16::MAX));
        out[..GLYPH_SURFACE_RECORD_LEN].copy_from_slice(&bytes);
        Ok(GLYPH_SURFACE_RECORD_LEN)
    }

    /// Checks direct directory bounds, expected kinds and exact REQUIRED flags.
    /// This does not parse bodies, validate coverage or checksum sample bytes.
    pub fn validate_sections(self, media: MediaPayload<'_>) -> Result<(), GlyphSurfaceRecordError> {
        for (index, kind) in [
            (Some(self.data), MediaSectionKind::DATA),
            (self.planes_section(), MediaSectionKind::PLANES),
            (self.codings_section(), MediaSectionKind::CODINGS),
            (self.groups_section(), MediaSectionKind::UNIT_GROUPS),
            (self.index_section(), MediaSectionKind::UNIT_INDEX),
        ] {
            if let Some(index) = index {
                SectionRef { index, kind }.validate(media)?;
            }
        }
        Ok(())
    }
}

struct SectionRef {
    index: u16,
    kind: MediaSectionKind,
}

impl SectionRef {
    fn new(index: u16, kind: MediaSectionKind) -> Result<Self, GlyphSurfaceRecordError> {
        if index == u16::MAX {
            return Err(GlyphSurfaceRecordError::ReservedSection { kind });
        }
        Ok(Self { index, kind })
    }

    fn validate(self, media: MediaPayload<'_>) -> Result<(), GlyphSurfaceRecordError> {
        let section = media
            .get(usize::from(self.index))
            .ok_or(GlyphSurfaceRecordError::SectionOutOfBounds { index: self.index })?;
        let descriptor = section.descriptor();
        if descriptor.kind() != self.kind {
            return Err(GlyphSurfaceRecordError::SectionKind {
                index: self.index,
                expected: self.kind,
                actual: descriptor.kind(),
            });
        }
        if descriptor.flags() != MediaSectionFlags::REQUIRED {
            return Err(GlyphSurfaceRecordError::SectionFlags {
                index: self.index,
                flags: descriptor.flags(),
            });
        }
        Ok(())
    }
}

/// Invalid glyph geometry, wire defaults, storage combinations or section references.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphSurfaceRecordError {
    Truncated {
        needed: usize,
        available: usize,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    ReservedNonZero {
        offset: usize,
    },
    UnknownPacking(u8),
    Map(GlyphMapError),
    ReservedSection {
        kind: MediaSectionKind,
    },
    ConflictingStorage,
    MissingCodings,
    MissingGroups,
    SectionOutOfBounds {
        index: u16,
    },
    SectionKind {
        index: u16,
        expected: MediaSectionKind,
        actual: MediaSectionKind,
    },
    SectionFlags {
        index: u16,
        flags: MediaSectionFlags,
    },
}

#[cfg(test)]
mod tests;
