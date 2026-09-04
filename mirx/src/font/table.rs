#![doc = include_str!("../../docs/representation-tables.md")]

use super::{
    FontRepresentationMatch, FontRepresentationRequest, FontRepresentations, FontSelectionError,
    GLYPH_SURFACE_RECORD_LEN, GlyphSurfaceRecord, GlyphSurfaceRecordError,
    REPRESENTATION_RECORD_LEN, RepresentationRecord, RepresentationRecordError,
};
use crate::{
    PayloadLimits,
    image::{ColorDescription, SurfaceDescriptor},
    media::MediaSectionKind,
    wire::read_u16_le,
};

mod iter;
pub use iter::RepresentationIter;

/// Borrowed representation records with checked scalar surface references.
///
/// Selection and ordinal access return inline values. Metrics, glyph-map ranges,
/// storage section references and sample integrity belong to complete face binding.
#[derive(Clone, Copy, Debug)]
pub struct RepresentationTable<'a> {
    records: &'a [u8],
    surfaces: &'a [u8],
    glyph_count: usize,
}

impl<'a> RepresentationTable<'a> {
    /// Validates exact bodies, resource bounds and referenced representation facts.
    /// No decoded metadata array is allocated and no sample bytes are accessed.
    pub fn open(
        records: &'a [u8],
        surfaces: &'a [u8],
        glyph_count: usize,
        limits: &PayloadLimits,
    ) -> Result<Self, RepresentationTableError> {
        u32::try_from(records.len()).map_err(|_| RepresentationTableError::SizeOverflow)?;
        for (bytes, size, kind) in [
            (
                records,
                REPRESENTATION_RECORD_LEN,
                MediaSectionKind::REPRESENTATIONS,
            ),
            (
                surfaces,
                GLYPH_SURFACE_RECORD_LEN,
                MediaSectionKind::SURFACE_GROUPS,
            ),
        ] {
            if bytes.len() % size != 0 {
                return Err(RepresentationTableError::PartialRecords {
                    kind,
                    byte_len: bytes.len(),
                });
            }
        }
        let count = records.len() / REPRESENTATION_RECORD_LEN;
        if count > limits.max_font_representations() as usize {
            return Err(RepresentationTableError::TooManyRepresentations {
                limit: limits.max_font_representations(),
                actual: count,
            });
        }
        if glyph_count > limits.max_font_glyphs() as usize {
            return Err(RepresentationTableError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: glyph_count,
            });
        }
        let surface_count = surfaces.len() / GLYPH_SURFACE_RECORD_LEN;
        if surface_count > usize::from(u16::MAX) + 1 {
            return Err(RepresentationTableError::TooManySurfaces {
                actual: surface_count,
            });
        }
        let table = Self {
            records,
            surfaces,
            glyph_count,
        };
        for index in 0..count {
            table.read(index)?;
        }
        FontRepresentations::validate_by(count, |index| {
            table.get(index).expect("validated record").representation()
        })
        .map_err(RepresentationTableError::Selection)?;
        Ok(table)
    }

    pub const fn len(self) -> usize {
        self.records.len() / REPRESENTATION_RECORD_LEN
    }
    pub const fn is_empty(self) -> bool {
        self.records.is_empty()
    }
    pub const fn glyph_count(self) -> usize {
        self.glyph_count
    }

    /// Resolves one representation in constant time, independent of source alignment.
    pub fn get(self, index: usize) -> Option<RepresentationRecord> {
        (index < self.len()).then(|| self.read(index).expect("validated immutable records"))
    }

    pub fn iter(self) -> RepresentationIter<'a> {
        RepresentationIter::new(self)
    }

    pub fn select(
        self,
        request: FontRepresentationRequest,
    ) -> Result<FontRepresentationMatch, FontSelectionError> {
        request.select_by(self.len(), |index| {
            self.get(index).expect("valid ordinal").representation()
        })
    }

    fn read(self, index: usize) -> Result<RepresentationRecord, RepresentationTableError> {
        let offset = index * REPRESENTATION_RECORD_LEN;
        let bytes = &self.records[offset..offset + REPRESENTATION_RECORD_LEN];
        let surface_index = read_u16_le(bytes, 10).expect("complete representation record");
        let surface_offset = usize::from(surface_index) * GLYPH_SURFACE_RECORD_LEN;
        let surface_bytes = self
            .surfaces
            .get(surface_offset..surface_offset + GLYPH_SURFACE_RECORD_LEN)
            .ok_or(RepresentationTableError::SurfaceOutOfBounds {
                representation: index,
                surface: surface_index,
            })?;
        let fail = |error| RepresentationTableError::Surface {
            index: surface_index,
            error,
        };
        let surface_record = GlyphSurfaceRecord::from_record(surface_bytes).map_err(fail)?;
        let layout = surface_record.sample_layout();
        if !layout.is_alpha() {
            return Err(fail(GlyphSurfaceRecordError::UnsupportedLayout(layout)));
        }
        let (width, height) = surface_record
            .logical_extent(self.glyph_count)
            .map_err(fail)?;
        let surface = SurfaceDescriptor::new(width, height, layout, ColorDescription::NONE)
            .expect("scalar surface layout");
        RepresentationRecord::from_record(bytes, surface)
            .map_err(|error| RepresentationTableError::Record { index, error })
    }
}

/// Invalid table bounds, scalar surface binding or representation identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RepresentationTableError {
    SizeOverflow,
    PartialRecords {
        kind: MediaSectionKind,
        byte_len: usize,
    },
    TooManyRepresentations {
        limit: u32,
        actual: usize,
    },
    TooManyGlyphs {
        limit: u32,
        actual: usize,
    },
    TooManySurfaces {
        actual: usize,
    },
    SurfaceOutOfBounds {
        representation: usize,
        surface: u16,
    },
    Surface {
        index: u16,
        error: GlyphSurfaceRecordError,
    },
    Record {
        index: usize,
        error: RepresentationRecordError,
    },
    Selection(FontSelectionError),
}

#[cfg(test)]
mod tests;
