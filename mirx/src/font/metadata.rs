use super::{
    ADVANCE_RECORD_LEN, Advances, CmapIndex, CmapIndexError, FACE_RECORD_LEN, FontFace,
    FontFaceError, GLYPH_ID_RECORD_LEN, GlyphId, GlyphIds, GlyphIdsError,
    RASTER_METRICS_RECORD_LEN, RasterMetrics, RasterMetricsTable, RepresentationTable,
    RepresentationTableError, ShapingData, ShapingDataError,
};
use crate::{
    Fixed, PayloadLimits,
    media::{MediaPayload, MediaPayloadError, MediaSection, MediaSectionFlags, MediaSectionKind},
};

pub(crate) const FONT_FACE_SECTION_ID: u16 = 0x0010;
pub(crate) const FONT_CMAP_INDEX_SECTION_ID: u16 = 0x0011;
pub(crate) const FONT_REPRESENTATIONS_SECTION_ID: u16 = 0x0012;
pub(crate) const FONT_ADVANCES_SECTION_ID: u16 = 0x0013;
pub(crate) const FONT_GLYPH_MAPS_SECTION_ID: u16 = 0x0014;
pub(crate) const FONT_SURFACE_GROUPS_SECTION_ID: u16 = 0x0015;
pub(crate) const FONT_GLYPH_IDS_SECTION_ID: u16 = 0x0016;
pub(crate) const FONT_RASTER_METRICS_SECTION_ID: u16 = 0x0017;
pub(crate) const FONT_SHAPING_SECTION_ID: u16 = 0x0018;

const FONT_SECTION_COUNT: usize = 9;

#[derive(Clone, Copy, Debug)]
pub struct FontMetadata<'a> {
    face: FontFace,
    cmap: CmapIndex<'a>,
    glyph_ids: Option<GlyphIds<'a>>,
    advances: Option<Advances<'a>>,
    shaping: Option<ShapingData<'a>>,
    representations: RepresentationTable<'a>,
    raster_metrics: RasterMetricsTable<'a>,
}

impl<'a> FontMetadata<'a> {
    pub fn open(bytes: &'a [u8], limits: &PayloadLimits) -> Result<Self, FontMetadataError> {
        let media = MediaPayload::open(bytes).map_err(FontMetadataError::Media)?;
        let sections = Sections::open(media)?;
        let face =
            FontFace::from_record(sections.required(FACE)?).map_err(FontMetadataError::Face)?;
        let glyph_count = usize::from(face.raster_count());
        if glyph_count > limits.max_font_glyphs() as usize {
            return Err(FontMetadataError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: glyph_count,
            });
        }

        let cmap = CmapIndex::open(sections.required(CMAP)?).map_err(FontMetadataError::Cmap)?;
        if cmap.len() > limits.max_font_codepoints() as usize {
            return Err(FontMetadataError::TooManyCodepoints {
                limit: limits.max_font_codepoints(),
                actual: cmap.len(),
            });
        }

        let glyph_ids = sections
            .optional(GLYPH_IDS)
            .map(GlyphIds::open)
            .transpose()
            .map_err(FontMetadataError::GlyphIds)?;
        if let Some(glyph_ids) = glyph_ids
            && glyph_ids.len() != glyph_count
        {
            return Err(FontMetadataError::GlyphIdCount {
                expected: glyph_count,
                actual: glyph_ids.len(),
            });
        }

        let advances = sections
            .optional(ADVANCES)
            .map(Advances::open)
            .transpose()
            .map_err(FontMetadataError::Placement)?;
        let shaping = sections
            .optional(SHAPING)
            .map(ShapingData::open)
            .transpose()
            .map_err(FontMetadataError::Shaping)?;
        match (advances, shaping) {
            (Some(_), Some(_)) => return Err(FontMetadataError::ConflictingAdvanceSources),
            (None, None) => return Err(FontMetadataError::MissingAdvanceSource),
            _ => {}
        }
        if let Some(advances) = advances
            && advances.len() != glyph_count
        {
            return Err(FontMetadataError::AdvanceCount {
                expected: glyph_count,
                actual: advances.len(),
            });
        }
        if let Some(shaping) = shaping {
            shaping
                .preflight(limits)
                .map_err(FontMetadataError::Shaping)?;
        }

        let representations = RepresentationTable::open(
            sections.required(REPRESENTATIONS)?,
            sections.required(SURFACE_GROUPS)?,
            glyph_count,
            limits,
        )
        .map_err(FontMetadataError::Representations)?;
        if representations.is_empty() {
            return Err(FontMetadataError::EmptyRepresentations);
        }
        let raster_metrics = RasterMetricsTable::open(sections.required(RASTER_METRICS)?)
            .map_err(FontMetadataError::Placement)?;
        let expected_metrics = representations
            .len()
            .checked_mul(glyph_count)
            .ok_or(FontMetadataError::SizeOverflow)?;
        if raster_metrics.len() != expected_metrics {
            return Err(FontMetadataError::RasterMetricCount {
                expected: expected_metrics,
                actual: raster_metrics.len(),
            });
        }

        let metadata = Self {
            face,
            cmap,
            glyph_ids,
            advances,
            shaping,
            representations,
            raster_metrics,
        };
        metadata.require_raster(face.default_glyph())?;
        for entry in cmap.iter() {
            metadata.require_raster(entry.glyph_id())?;
        }
        Ok(metadata)
    }

    pub const fn face(self) -> FontFace {
        self.face
    }

    pub const fn cmap(self) -> CmapIndex<'a> {
        self.cmap
    }

    pub const fn shaping_data(self) -> Option<ShapingData<'a>> {
        self.shaping
    }

    pub const fn representations(self) -> RepresentationTable<'a> {
        self.representations
    }

    pub fn map_char(self, scalar: char) -> Option<GlyphId> {
        self.cmap.lookup(scalar)
    }

    pub fn glyph_id(self, ordinal: usize) -> Option<GlyphId> {
        match self.glyph_ids {
            Some(glyph_ids) => glyph_ids.get(ordinal),
            None => u16::try_from(ordinal)
                .ok()
                .filter(|_| ordinal < usize::from(self.face.raster_count()))
                .map(GlyphId::new),
        }
    }

    pub fn raster_ordinal(self, glyph_id: GlyphId) -> Option<usize> {
        match self.glyph_ids {
            Some(glyph_ids) => glyph_ids.ordinal(glyph_id).ok(),
            None => {
                let ordinal = usize::from(glyph_id.get());
                (ordinal < usize::from(self.face.raster_count())).then_some(ordinal)
            }
        }
    }

    pub fn advance(self, glyph_id: GlyphId) -> Option<Fixed> {
        self.advances?.get(self.raster_ordinal(glyph_id)?)
    }

    pub fn raster_metrics(self, representation: usize, glyph_id: GlyphId) -> Option<RasterMetrics> {
        let ordinal = self.raster_ordinal(glyph_id)?;
        let index = representation
            .checked_mul(usize::from(self.face.raster_count()))?
            .checked_add(ordinal)?;
        self.raster_metrics.get(index)
    }

    fn require_raster(self, glyph_id: GlyphId) -> Result<(), FontMetadataError> {
        if self.raster_ordinal(glyph_id).is_some() {
            Ok(())
        } else {
            Err(FontMetadataError::MissingRasterGlyph(glyph_id))
        }
    }
}

const FACE: usize = 0;
const CMAP: usize = 1;
const REPRESENTATIONS: usize = 2;
const ADVANCES: usize = 3;
const GLYPH_MAPS: usize = 4;
const SURFACE_GROUPS: usize = 5;
const GLYPH_IDS: usize = 6;
const RASTER_METRICS: usize = 7;
const SHAPING: usize = 8;

struct Sections<'a> {
    slots: [Option<MediaSection<'a>>; FONT_SECTION_COUNT],
}

impl<'a> Sections<'a> {
    fn open(media: MediaPayload<'a>) -> Result<Self, FontMetadataError> {
        let mut sections = Self {
            slots: [None; FONT_SECTION_COUNT],
        };
        for section in media.sections() {
            let descriptor = section.descriptor();
            let kind = descriptor.kind();
            let slot = match kind.raw() {
                FONT_FACE_SECTION_ID => Some(FACE),
                FONT_CMAP_INDEX_SECTION_ID => Some(CMAP),
                FONT_REPRESENTATIONS_SECTION_ID => Some(REPRESENTATIONS),
                FONT_ADVANCES_SECTION_ID => Some(ADVANCES),
                FONT_GLYPH_MAPS_SECTION_ID => Some(GLYPH_MAPS),
                FONT_SURFACE_GROUPS_SECTION_ID => Some(SURFACE_GROUPS),
                FONT_GLYPH_IDS_SECTION_ID => Some(GLYPH_IDS),
                FONT_RASTER_METRICS_SECTION_ID => Some(RASTER_METRICS),
                FONT_SHAPING_SECTION_ID => Some(SHAPING),
                _ => None,
            };
            let known = slot.is_some() || Self::is_storage(kind);
            if known && descriptor.flags() != MediaSectionFlags::REQUIRED {
                return Err(FontMetadataError::SectionFlags(kind));
            }
            if !known && descriptor.flags().is_required() {
                return Err(FontMetadataError::UnknownRequiredSection(kind));
            }
            if let Some(slot) = slot
                && sections.slots[slot].replace(section).is_some()
            {
                return Err(FontMetadataError::DuplicateSection(kind));
            }
        }
        Ok(sections)
    }

    fn required(&self, slot: usize) -> Result<&'a [u8], FontMetadataError> {
        self.optional(slot)
            .ok_or(FontMetadataError::MissingSection(section_kind(slot)))
    }

    fn optional(&self, slot: usize) -> Option<&'a [u8]> {
        self.slots[slot].map(MediaSection::bytes)
    }

    fn is_storage(kind: MediaSectionKind) -> bool {
        matches!(
            kind,
            MediaSectionKind::PLANES
                | MediaSectionKind::CODINGS
                | MediaSectionKind::UNIT_GROUPS
                | MediaSectionKind::UNIT_INDEX
                | MediaSectionKind::DATA
                | MediaSectionKind::INTEGRITY
        )
    }
}

fn section_kind(slot: usize) -> MediaSectionKind {
    let raw = [
        FONT_FACE_SECTION_ID,
        FONT_CMAP_INDEX_SECTION_ID,
        FONT_REPRESENTATIONS_SECTION_ID,
        FONT_ADVANCES_SECTION_ID,
        FONT_GLYPH_MAPS_SECTION_ID,
        FONT_SURFACE_GROUPS_SECTION_ID,
        FONT_GLYPH_IDS_SECTION_ID,
        FONT_RASTER_METRICS_SECTION_ID,
        FONT_SHAPING_SECTION_ID,
    ][slot];
    MediaSectionKind::new(raw).expect("nonzero FONT section ID")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontMetadataError {
    Media(MediaPayloadError),
    Face(FontFaceError),
    Cmap(CmapIndexError),
    GlyphIds(GlyphIdsError),
    Placement(super::PlacementError),
    Shaping(ShapingDataError),
    Representations(RepresentationTableError),
    MissingSection(MediaSectionKind),
    DuplicateSection(MediaSectionKind),
    UnknownRequiredSection(MediaSectionKind),
    SectionFlags(MediaSectionKind),
    TooManyGlyphs { limit: u32, actual: usize },
    TooManyCodepoints { limit: u32, actual: usize },
    GlyphIdCount { expected: usize, actual: usize },
    AdvanceCount { expected: usize, actual: usize },
    RasterMetricCount { expected: usize, actual: usize },
    MissingAdvanceSource,
    ConflictingAdvanceSources,
    MissingRasterGlyph(GlyphId),
    EmptyRepresentations,
    SizeOverflow,
}

const _: () = {
    assert!(FACE_RECORD_LEN == 20);
    assert!(ADVANCE_RECORD_LEN == 4);
    assert!(GLYPH_ID_RECORD_LEN == 2);
    assert!(RASTER_METRICS_RECORD_LEN == 8);
};

#[cfg(test)]
mod tests;
