use super::{
    EncodedGlyphError, EncodedGlyphs, FontMetadata, FontMetadataError, FontRepresentationRequest,
    GLYPH_REGION_LEN, GLYPH_SURFACE_RECORD_LEN, GlyphId, GlyphMap, GlyphPacking,
    GlyphSurfaceRecord, GlyphSurfaceRecordError, RasterMetrics, RawGlyphs, RepresentationRecord,
    RepresentationTable, ShapingData,
};
use crate::{
    ByteAlignment, PayloadLimits,
    image::{CoverageBudget, CoverageError, EncodedImageError, RasterPreflight},
    media::{MediaPayload, MediaPayloadError, MediaSection, MediaSectionKind},
};

/// Borrowed metadata and referenced sample storage for one FONT face.
/// Opening does not scan DATA or establish scalar codec support.
#[derive(Clone, Copy, Debug)]
pub struct FontView<'a> {
    media: MediaPayload<'a>,
    metadata: FontMetadata<'a>,
    surfaces: &'a [u8],
    maps: &'a [u8],
    map_table_len: usize,
    file_offset: Option<u32>,
    metadata_work: u64,
    checksum_bytes: u32,
}

impl<'a> FontView<'a> {
    pub fn open(bytes: &'a [u8], limits: &PayloadLimits) -> Result<Self, FontError> {
        Self::open_inner(bytes, None, limits)
    }

    /// Checks declared file/Flash placement independently of the actual slice address.
    pub fn open_at(
        bytes: &'a [u8],
        offset: u32,
        limits: &PayloadLimits,
    ) -> Result<Self, FontError> {
        Self::open_inner(bytes, Some(offset), limits)
    }

    fn open_inner(
        bytes: &'a [u8],
        file_offset: Option<u32>,
        limits: &PayloadLimits,
    ) -> Result<Self, FontError> {
        let media = MediaPayload::open(bytes).map_err(FontError::Media)?;
        if media.header().flags().unknown_bits() != 0 {
            return Err(FontError::UnknownMediaFlags);
        }
        if let Some(offset) = file_offset {
            offset
                .checked_add(bytes.len() as u32)
                .ok_or(FontError::SizeOverflow)?;
        }
        let mut budget = CoverageBudget::new(limits.max_raster_work());
        budget
            .spend_many(u64::from(media.header().section_count()))
            .map_err(FontError::Work)?;
        let metadata = FontMetadata::open_media(media, limits).map_err(FontError::Metadata)?;
        let glyph_count = metadata.glyph_count();
        let representation_count = metadata.representations().len();
        let surfaces = media
            .section(MediaSectionKind::SURFACE_GROUPS)
            .expect("metadata requires surface groups")
            .bytes();
        let maps = media
            .section(MediaSectionKind::GLYPH_MAPS)
            .map_or(&[][..], MediaSection::bytes);
        if maps.is_empty() && media.section(MediaSectionKind::GLYPH_MAPS).is_some() {
            return Err(FontError::EmptyMapSection);
        }
        let map_table_len = glyph_count
            .checked_mul(GLYPH_REGION_LEN)
            .ok_or(FontError::SizeOverflow)?;
        if maps.len() % map_table_len != 0 {
            return Err(FontError::MapsLength {
                table_len: map_table_len,
                actual: maps.len(),
            });
        }
        let surface_count = surfaces.len() / GLYPH_SURFACE_RECORD_LEN;
        if surface_count > representation_count {
            return Err(FontError::UnreferencedSurface);
        }
        let glyphs = glyph_count as u64;
        let representations = representation_count as u64;
        let surface_count = surface_count as u64;
        let directory_count = u64::from(media.header().section_count());
        let work = glyphs
            + representations * (glyphs + representations + surface_count + directory_count + 16)
            + directory_count * surface_count;
        budget.spend_many(work).map_err(FontError::Work)?;
        let mut view = Self {
            media,
            metadata,
            surfaces,
            maps,
            map_table_len,
            file_offset,
            metadata_work: 0,
            checksum_bytes: media
                .sections_of_kind(MediaSectionKind::DATA)
                .map(|section| section.descriptor().size())
                .sum(),
        };
        for index in 0..representation_count {
            view.resolve_representation(index)?;
        }
        let map_count = maps.len() / map_table_len;
        if map_count > representation_count {
            return Err(FontError::UnreferencedMaps);
        }
        for map_index in 0..map_count {
            let offset = map_index * map_table_len;
            if !metadata.representations().iter().any(|record| {
                record.glyph_map_offset() as usize == offset
                    && metadata.representations().surface(record).packing() == GlyphPacking::Atlas2D
            }) {
                return Err(FontError::UnreferencedMaps);
            }
        }
        for index in 0..view.surface_count() {
            let representation = view
                .surface_representation(index)
                .ok_or(FontError::UnreferencedSurface)?;
            let record = representation.surface();
            record
                .validate_sections(media)
                .map_err(|error| FontError::Surface { index, error })?;
            for reference in [record.codings_section(), record.groups_section()]
                .into_iter()
                .flatten()
            {
                budget
                    .spend_many(u64::from(
                        media
                            .get(usize::from(reference))
                            .expect("validated reference")
                            .descriptor()
                            .size(),
                    ))
                    .map_err(FontError::Work)?;
            }
            let storage = view.bind(representation)?;
            view.check_file(index, record, storage)?;
        }
        for (index, section) in media.sections().enumerate() {
            if is_storage(section.descriptor().kind())
                && !(0..view.surface_count()).any(|surface| {
                    let record = view.surface_record(surface);
                    [
                        Some(record.data_section()),
                        record.planes_section(),
                        record.codings_section(),
                        record.groups_section(),
                        record.index_section(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|reference| usize::from(reference) == index)
                })
            {
                return Err(FontError::UnreferencedSection { index });
            }
        }
        view.metadata_work = limits.max_raster_work() - budget.remaining();
        Ok(view)
    }

    pub const fn media(self) -> MediaPayload<'a> {
        self.media
    }
    pub const fn metadata(self) -> FontMetadata<'a> {
        self.metadata
    }
    pub const fn face(self) -> super::FontFace {
        self.metadata.face()
    }
    pub const fn cmap(self) -> super::CmapIndex<'a> {
        self.metadata.cmap()
    }
    pub const fn shaping_data(self) -> Option<ShapingData<'a>> {
        self.metadata.shaping_data()
    }
    pub const fn representations(self) -> RepresentationTable<'a> {
        self.metadata.representations()
    }
    pub fn map_char(self, scalar: char) -> Option<GlyphId> {
        self.metadata.map_char(scalar)
    }
    pub fn glyph_id(self, ordinal: usize) -> Option<GlyphId> {
        self.metadata.glyph_id(ordinal)
    }
    pub fn raster_ordinal(self, glyph_id: GlyphId) -> Option<usize> {
        self.metadata.raster_ordinal(glyph_id)
    }
    pub fn advance(self, glyph_id: GlyphId) -> Option<crate::Fixed> {
        self.metadata.advance(glyph_id)
    }
    pub fn raster_metrics(self, representation: usize, glyph_id: GlyphId) -> Option<RasterMetrics> {
        self.metadata.raster_metrics(representation, glyph_id)
    }
    pub const fn surface_count(self) -> usize {
        self.surfaces.len() / GLYPH_SURFACE_RECORD_LEN
    }

    /// Resolves sample storage after metadata admission, without DATA verification.
    /// Encoded sample requests verify their declared integrity through decode plans.
    pub fn glyphs(self, representation: usize) -> Option<FontGlyphs<'a>> {
        self.representation(representation)
            .map(|value| self.bind(value).expect("admitted immutable storage"))
    }

    /// Verifies complete DATA coverage without decoding samples.
    pub fn validate_data(self) -> Result<(), FontError> {
        self.media.validate_data().map_err(FontError::Media)
    }

    /// Validates each unique surface under one raster budget, then checks DATA once.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), FontError> {
        if self.metadata.glyph_count() > limits.max_font_glyphs() as usize {
            return Err(FontError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: self.metadata.glyph_count(),
            });
        }
        if self.metadata.cmap().len() > limits.max_font_cmap_entries() as usize {
            return Err(FontError::TooManyCmapEntries {
                limit: limits.max_font_cmap_entries(),
                actual: self.metadata.cmap().len(),
            });
        }
        if self.metadata.representations().len() > limits.max_font_representations() as usize {
            return Err(FontError::TooManyRepresentations {
                limit: limits.max_font_representations(),
                actual: self.metadata.representations().len(),
            });
        }
        if let Some(shaping) = self.metadata.shaping_data() {
            shaping.preflight(limits).map_err(FontError::Shaping)?;
        }
        let mut preflight = RasterPreflight::new(limits, 0).map_err(FontError::Raster)?;
        self.preflight_in(&mut preflight)
    }

    pub(super) fn preflight_in(self, preflight: &mut RasterPreflight<'_>) -> Result<(), FontError> {
        preflight
            .spend(self.metadata_work)
            .map_err(FontError::Raster)?;
        for index in 0..self.surface_count() {
            let representation = self
                .surface_representation(index)
                .expect("referenced surface");
            if let FontGlyphs::Encoded(glyphs) = self.bind(representation)? {
                glyphs
                    .preflight_in(preflight)
                    .map_err(|error| FontError::EncodedSurface { index, error })?;
            }
        }
        preflight
            .data(self.media, self.checksum_bytes)
            .map_err(FontError::Raster)
    }

    /// Maximum declared plane/group alignment; not an actual backing-pointer guarantee.
    pub fn input_alignment(self) -> Result<ByteAlignment, FontError> {
        let mut alignment = ByteAlignment::ONE;
        for index in 0..self.surface_count() {
            let value = self.bind(
                self.surface_representation(index)
                    .expect("referenced surface"),
            )?;
            alignment = alignment.max(match value {
                FontGlyphs::Raw(glyphs) => glyphs.memory_layout().required_alignment(),
                FontGlyphs::Encoded(glyphs) => glyphs
                    .input_alignment()
                    .map_err(|error| FontError::Encoded { index, error })?,
            });
        }
        Ok(alignment)
    }

    pub(super) fn surface_record(self, index: usize) -> GlyphSurfaceRecord {
        GlyphSurfaceRecord::from_record(&self.surfaces[index * GLYPH_SURFACE_RECORD_LEN..])
            .expect("referenced valid surface")
    }

    /// Solves the shared file-base residue, then checks every independent DATA constraint.
    pub(crate) fn aligned_file_offset(
        self,
        cursor: u32,
        container_alignment: u32,
    ) -> Result<u32, FontError> {
        let mut alignment = container_alignment;
        let mut anchor = None;
        for index in 0..self.surface_count() {
            let Some((required, offset)) = self.alignment_constraint(index)? else {
                continue;
            };
            let required = required.get().max(container_alignment);
            if anchor.is_none() || required > alignment {
                alignment = required;
                anchor = Some(offset);
            }
        }
        let alignment = u64::from(alignment);
        let remainder = (u64::from(cursor) + u64::from(anchor.unwrap_or(0))) % alignment;
        let offset = u32::try_from(u64::from(cursor) + (alignment - remainder) % alignment)
            .map_err(|_| FontError::SizeOverflow)?;
        offset
            .checked_add(self.media.payload_len() as u32)
            .ok_or(FontError::SizeOverflow)?;
        let placed = Self {
            file_offset: Some(offset),
            ..self
        };
        for index in 0..self.surface_count() {
            if let Some((required, relative)) = self.alignment_constraint(index)? {
                let absolute = offset
                    .checked_add(relative)
                    .ok_or(FontError::SizeOverflow)?;
                if absolute % required.get().max(container_alignment) != 0 {
                    return Err(FontError::FileAddressUnaligned {
                        surface: index,
                        data_offset: absolute,
                    });
                }
            }
            placed.check_file(
                index,
                self.surface_record(index),
                self.bind(
                    self.surface_representation(index)
                        .expect("referenced surface"),
                )?,
            )?;
        }
        Ok(offset)
    }

    fn alignment_constraint(self, index: usize) -> Result<Option<(ByteAlignment, u32)>, FontError> {
        let record = self.surface_record(index);
        let glyphs = self.bind(
            self.surface_representation(index)
                .expect("referenced surface"),
        )?;
        let data = self
            .media
            .get(usize::from(record.data_section()))
            .expect("DATA reference");
        match glyphs {
            FontGlyphs::Raw(raw) if !raw.as_bytes().is_empty() => Ok(Some((
                raw.memory_layout().required_alignment(),
                data.descriptor()
                    .offset()
                    .checked_add(raw.memory_layout().data_offset())
                    .ok_or(FontError::SizeOverflow)?,
            ))),
            FontGlyphs::Raw(_) => Ok(None),
            FontGlyphs::Encoded(encoded) if data.descriptor().size() != 0 => Ok(Some((
                encoded
                    .input_alignment()
                    .map_err(|error| FontError::Encoded { index, error })?,
                data.descriptor().offset(),
            ))),
            FontGlyphs::Encoded(_) => Ok(None),
        }
    }

    pub fn representation(self, index: usize) -> Option<FontRepresentationView<'a>> {
        self.resolve_representation(index)
            .expect("admitted representation")
    }

    fn resolve_representation(
        self,
        index: usize,
    ) -> Result<Option<FontRepresentationView<'a>>, FontError> {
        let Some(record) = self.metadata.representations().get(index) else {
            return Ok(None);
        };
        let surface = self.metadata.representations().surface(record);
        let map = match surface.packing() {
            GlyphPacking::GlyphMajor => {
                if record.glyph_map_offset() != 0 {
                    return Err(FontError::ImplicitMapOffset {
                        representation: index,
                        offset: record.glyph_map_offset(),
                    });
                }
                GlyphMap::glyph_major(
                    surface.width(),
                    surface.height(),
                    self.metadata.glyph_count(),
                )
                .map_err(|error| FontError::Map {
                    representation: index,
                    error,
                })?
            }
            GlyphPacking::Atlas2D => {
                let offset = record.glyph_map_offset() as usize;
                if offset % self.map_table_len != 0 {
                    return Err(FontError::MapOffset {
                        representation: index,
                        offset: record.glyph_map_offset(),
                    });
                }
                let end = offset
                    .checked_add(self.map_table_len)
                    .ok_or(FontError::SizeOverflow)?;
                let bytes = self
                    .maps
                    .get(offset..end)
                    .ok_or(FontError::MapOutOfBounds {
                        representation: index,
                    })?;
                GlyphMap::from_records(surface.width(), surface.height(), bytes).map_err(
                    |error| FontError::Map {
                        representation: index,
                        error,
                    },
                )?
            }
        };
        Ok(Some(FontRepresentationView {
            index,
            used_fallback: false,
            record,
            surface,
            map,
            metadata: self.metadata,
        }))
    }

    pub fn select(
        self,
        request: FontRepresentationRequest,
    ) -> Result<FontRepresentationView<'a>, FontError> {
        let selected = self
            .metadata
            .representations()
            .select(request)
            .map_err(FontError::Selection)?;
        let mut representation = self
            .representation(selected.index())
            .expect("selected representation");
        representation.used_fallback = selected.used_fallback();
        Ok(representation)
    }

    pub(super) fn surface_representation(self, index: usize) -> Option<FontRepresentationView<'a>> {
        self.metadata
            .representations()
            .iter()
            .position(|record| usize::from(record.surface_index()) == index)
            .and_then(|index| self.representation(index))
    }

    fn bind(self, representation: FontRepresentationView<'a>) -> Result<FontGlyphs<'a>, FontError> {
        let surface = representation.surface();
        let index = usize::from(representation.record().surface_index());
        if surface.codings_section().is_some() {
            let mut glyphs = surface
                .encoded_glyphs(self.media, representation.map())
                .map_err(|error| FontError::Encoded { index, error })?;
            if let Some(offset) = self.file_offset {
                glyphs = glyphs.with_file_offset(offset);
            }
            Ok(FontGlyphs::Encoded(glyphs))
        } else {
            surface
                .raw_glyphs(self.media, representation.map())
                .map(FontGlyphs::Raw)
                .map_err(|error| FontError::Surface { index, error })
        }
    }

    fn check_file(
        self,
        index: usize,
        record: GlyphSurfaceRecord,
        storage: FontGlyphs<'a>,
    ) -> Result<(), FontError> {
        let Some(file_offset) = self.file_offset else {
            return Ok(());
        };
        let data = self
            .media
            .get(usize::from(record.data_section()))
            .expect("validated DATA reference");
        let offset = file_offset
            .checked_add(data.descriptor().offset())
            .ok_or(FontError::SizeOverflow)?;
        let aligned = match storage {
            FontGlyphs::Raw(glyphs) => glyphs.file_address_is_aligned(offset),
            FontGlyphs::Encoded(glyphs) => {
                offset
                    % glyphs
                        .input_alignment()
                        .map_err(|error| FontError::Encoded { index, error })?
                        .get()
                    == 0
            }
        };
        if !aligned {
            return Err(FontError::FileAddressUnaligned {
                surface: index,
                data_offset: offset,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FontRepresentationView<'a> {
    index: usize,
    used_fallback: bool,
    record: RepresentationRecord,
    surface: GlyphSurfaceRecord,
    map: GlyphMap<'a>,
    metadata: FontMetadata<'a>,
}

impl<'a> FontRepresentationView<'a> {
    pub const fn index(self) -> usize {
        self.index
    }
    pub const fn used_fallback(self) -> bool {
        self.used_fallback
    }
    pub const fn record(self) -> RepresentationRecord {
        self.record
    }
    pub const fn surface(self) -> GlyphSurfaceRecord {
        self.surface
    }
    pub const fn map(self) -> GlyphMap<'a> {
        self.map
    }
    pub fn advance(self, glyph_id: GlyphId) -> Option<crate::Fixed> {
        self.metadata.advance(glyph_id)
    }
    pub fn raster_metrics(self, glyph_id: GlyphId) -> Option<RasterMetrics> {
        self.metadata.raster_metrics(self.index, glyph_id)
    }
}

/// Borrowed glyph storage; RAW bytes and encoded streams retain distinct contracts.
#[derive(Clone, Copy, Debug)]
pub enum FontGlyphs<'a> {
    Raw(RawGlyphs<'a, 'a>),
    Encoded(EncodedGlyphs<'a, 'a>),
}

impl<'a> FontGlyphs<'a> {
    pub const fn map(self) -> GlyphMap<'a> {
        match self {
            Self::Raw(glyphs) => glyphs.map(),
            Self::Encoded(glyphs) => glyphs.map(),
        }
    }
}

fn is_storage(kind: MediaSectionKind) -> bool {
    matches!(
        kind,
        MediaSectionKind::DATA
            | MediaSectionKind::PLANES
            | MediaSectionKind::CODINGS
            | MediaSectionKind::UNIT_GROUPS
            | MediaSectionKind::UNIT_INDEX
    )
}

/// Invalid face metadata, storage references, declared placement or raster preflight.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontError {
    UnsupportedSection(MediaSectionKind),
    Integrity(crate::media::IntegrityError),
    Selection(super::FontSelectionError),
    Record(super::RepresentationRecordError),
    Image(crate::image::ImageEncodeError),
    Cardinality,
    GlyphOutOfBounds {
        index: usize,
    },
    OwnedBytesLimitExceeded {
        needed: usize,
        limit: usize,
    },
    SurfaceMismatch,
    SurfaceOutOfBounds {
        representation: usize,
    },
    MapOutOfBounds {
        representation: usize,
    },
    MapMismatch {
        representation: usize,
    },
    UnreferencedStorage,
    AllocationFailed,
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    Media(MediaPayloadError),
    Metadata(FontMetadataError),
    Cmap(super::CmapIndexError),
    GlyphIds(super::GlyphIdsError),
    Shaping(super::ShapingDataError),
    Surface {
        index: usize,
        error: GlyphSurfaceRecordError,
    },
    Encoded {
        index: usize,
        error: EncodedGlyphError,
    },
    EncodedSurface {
        index: usize,
        error: EncodedImageError,
    },
    Raster(EncodedImageError),
    Work(CoverageError),
    UnknownMediaFlags,
    TooManyGlyphs {
        limit: u32,
        actual: usize,
    },
    TooManyCmapEntries {
        limit: u32,
        actual: usize,
    },
    TooManyRepresentations {
        limit: u32,
        actual: usize,
    },
    FileAddressUnaligned {
        surface: usize,
        data_offset: u32,
    },
    EmptyMapSection,
    MapsLength {
        table_len: usize,
        actual: usize,
    },
    UnreferencedMaps,
    ImplicitMapOffset {
        representation: usize,
        offset: u32,
    },
    MapOffset {
        representation: usize,
        offset: u32,
    },
    Map {
        representation: usize,
        error: super::GlyphMapError,
    },
    GlyphIdCount {
        expected: usize,
        actual: usize,
    },
    AdvanceCount {
        expected: usize,
        actual: usize,
    },
    RasterMetricCount {
        expected: usize,
        actual: usize,
    },
    MissingRasterGlyph(GlyphId),
    NonCanonicalGlyphIds,
    ShapingBytesLimitExceeded {
        limit: usize,
        actual: usize,
    },
    UnreferencedSurface,
    UnreferencedSection {
        index: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests;
