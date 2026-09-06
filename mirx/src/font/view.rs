use super::{
    EncodedGlyphError, EncodedGlyphs, FaceRepresentation, FaceTables, FaceTablesError,
    FontCodepointError, FontCodepoints, GLYPH_SURFACE_RECORD_LEN, GlyphMap, GlyphSurfaceRecord,
    GlyphSurfaceRecordError, REPRESENTATION_RECORD_LEN, RawGlyphs, RepresentationTable,
    RepresentationTableError,
};
use crate::{
    ByteAlignment, PayloadLimits,
    image::{CoverageBudget, CoverageError, EncodedImageError, RasterPreflight},
    media::{MediaPayload, MediaPayloadError, MediaSection, MediaSectionFlags, MediaSectionKind},
};

/// Borrowed metadata and referenced sample storage for one FONT face.
/// Opening does not scan DATA or establish scalar codec support.
#[derive(Clone, Copy, Debug)]
pub struct FontView<'a> {
    media: MediaPayload<'a>,
    tables: FaceTables<'a>,
    surfaces: &'a [u8],
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
        let sections = Sections::open(media)?;
        let codepoint_bytes = sections.required(0, MediaSectionKind::CODEPOINTS)?;
        let record_bytes = sections.required(1, MediaSectionKind::REPRESENTATIONS)?;
        let metrics = sections.required(2, MediaSectionKind::METRICS)?;
        let surfaces = sections.required(3, MediaSectionKind::SURFACE_GROUPS)?;
        let maps = sections.slots[4].map_or(&[][..], MediaSection::bytes);
        let glyph_count = codepoint_bytes.len() / 4;
        let representation_count = record_bytes.len() / REPRESENTATION_RECORD_LEN;
        Self::check_counts(glyph_count, representation_count, limits)?;
        if glyph_count == 0 {
            return Err(FontError::EmptyGlyphTable);
        }
        let surface_count = surfaces.len() / GLYPH_SURFACE_RECORD_LEN;
        if surface_count > representation_count {
            return Err(FontError::UnreferencedSurface);
        }
        // Admission covers Unicode/map scans, repeated inline record resolution,
        // reference ownership and each encoded body's own bounded parsing.
        let glyphs = glyph_count as u64;
        let representations = representation_count as u64;
        let surface_count = surface_count as u64;
        let directory_count = u64::from(media.header().section_count());
        let work = glyphs
            + representations * (glyphs + representations + surface_count + directory_count + 16)
            + directory_count * surface_count;
        budget.spend_many(work).map_err(FontError::Work)?;
        let codepoints = FontCodepoints::open(codepoint_bytes).map_err(FontError::Codepoints)?;
        let representations =
            RepresentationTable::open(record_bytes, surfaces, glyph_count, limits)
                .map_err(FontError::Representations)?;
        let tables = FaceTables::new(codepoints, representations, metrics, maps)
            .map_err(FontError::Tables)?;
        let mut view = Self {
            media,
            tables,
            surfaces,
            file_offset,
            metadata_work: 0,
            checksum_bytes: media
                .sections_of_kind(MediaSectionKind::DATA)
                .map(|section| section.descriptor().size())
                .sum(),
        };
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
            if Sections::is_storage(section.descriptor().kind())
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

    fn check_counts(
        glyphs: usize,
        representations: usize,
        limits: &PayloadLimits,
    ) -> Result<(), FontError> {
        if glyphs > limits.max_font_glyphs() as usize {
            return Err(FontError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: glyphs,
            });
        }
        if representations > limits.max_font_representations() as usize {
            return Err(FontError::TooManyRepresentations {
                limit: limits.max_font_representations(),
                actual: representations,
            });
        }
        Ok(())
    }

    pub const fn media(self) -> MediaPayload<'a> {
        self.media
    }
    pub const fn tables(self) -> FaceTables<'a> {
        self.tables
    }
    pub const fn surface_count(self) -> usize {
        self.surfaces.len() / GLYPH_SURFACE_RECORD_LEN
    }

    /// Resolves sample storage after metadata admission, without DATA verification.
    /// Encoded sample requests verify their declared integrity through decode plans.
    pub fn glyphs(self, representation: usize) -> Option<FontGlyphs<'a>> {
        self.tables
            .get(representation)
            .map(|value| self.bind(value).expect("admitted immutable storage"))
    }

    /// Verifies complete DATA coverage without decoding samples.
    pub fn validate_data(self) -> Result<(), FontError> {
        self.media.validate_data().map_err(FontError::Media)
    }

    /// Validates each unique surface under one raster budget, then checks DATA once.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), FontError> {
        Self::check_counts(self.tables.glyph_count(), self.tables.len(), limits)?;
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

    pub(super) fn surface_representation(self, index: usize) -> Option<FaceRepresentation<'a>> {
        self.tables
            .representations()
            .iter()
            .position(|record| usize::from(record.surface_index()) == index)
            .and_then(|index| self.tables.get(index))
    }

    fn bind(self, representation: FaceRepresentation<'a>) -> Result<FontGlyphs<'a>, FontError> {
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

struct Sections<'a> {
    slots: [Option<MediaSection<'a>>; 5],
}

impl<'a> Sections<'a> {
    fn open(media: MediaPayload<'a>) -> Result<Self, FontError> {
        let mut result = Self { slots: [None; 5] };
        for section in media.sections() {
            let descriptor = section.descriptor();
            let kind = descriptor.kind();
            let slot = match kind {
                MediaSectionKind::CODEPOINTS => Some(0),
                MediaSectionKind::REPRESENTATIONS => Some(1),
                MediaSectionKind::METRICS => Some(2),
                MediaSectionKind::SURFACE_GROUPS => Some(3),
                MediaSectionKind::GLYPH_MAPS => Some(4),
                MediaSectionKind::SURFACE | MediaSectionKind::COLOR_TABLE => {
                    return Err(FontError::UnexpectedSection(kind));
                }
                _ => None,
            };
            let known =
                slot.is_some() || Self::is_storage(kind) || kind == MediaSectionKind::INTEGRITY;
            if known && descriptor.flags() != MediaSectionFlags::REQUIRED {
                return Err(FontError::SectionFlags(kind));
            }
            if !known && descriptor.flags().is_required() {
                return Err(FontError::UnknownRequiredSection(kind));
            }
            if let Some(slot) = slot {
                if result.slots[slot].replace(section).is_some() {
                    return Err(FontError::DuplicateSection(kind));
                }
                if slot == 4 && section.bytes().is_empty() {
                    return Err(FontError::EmptyMapSection);
                }
            }
        }
        Ok(result)
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
    fn required(&self, index: usize, kind: MediaSectionKind) -> Result<&'a [u8], FontError> {
        self.slots[index]
            .map(MediaSection::bytes)
            .ok_or(FontError::MissingSection(kind))
    }
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
    RepresentationOutOfBounds {
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
    Codepoints(FontCodepointError),
    Representations(RepresentationTableError),
    Tables(FaceTablesError),
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
    MissingSection(MediaSectionKind),
    DuplicateSection(MediaSectionKind),
    UnexpectedSection(MediaSectionKind),
    UnknownRequiredSection(MediaSectionKind),
    SectionFlags(MediaSectionKind),
    UnknownMediaFlags,
    TooManyGlyphs {
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
    EmptyGlyphTable,
    EmptyMapSection,
    UnreferencedSurface,
    UnreferencedSection {
        index: usize,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests;
