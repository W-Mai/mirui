#![doc = include_str!("../../../docs/encoded-glyphs.md")]

use super::{GlyphSurfaceRecord, GlyphSurfaceRecordError};
use crate::{
    PayloadLimits,
    font::GlyphMap,
    image::{
        ColorDescription, CoverageBudget, DecodeError, EncodedImageError, EncodedImageView,
        EncodedSections, ImageDecodePlan, ImageGroups, SurfaceDescriptor, SurfaceRequirements,
        UnitGroup,
    },
    media::MediaPayload,
};

impl GlyphSurfaceRecord {
    /// Binds encoded scalar glyph storage without decoding or scanning DATA.
    /// Section identity, coding-table shape and implicit defaults are checked.
    pub fn encoded_glyphs<'map, 'data>(
        self,
        media: MediaPayload<'data>,
        map: GlyphMap<'map>,
    ) -> Result<EncodedGlyphs<'map, 'data>, EncodedGlyphError> {
        if self.codings_section().is_none() {
            return Err(EncodedGlyphError::ExpectedEncodedStorage);
        }
        self.validate_sections(media)
            .map_err(EncodedGlyphError::Record)?;
        self.validate_map(map).map_err(EncodedGlyphError::Record)?;
        let layout = self.sample_layout();
        if !layout.is_alpha() {
            return Err(EncodedGlyphError::Record(
                GlyphSurfaceRecordError::UnsupportedLayout(layout),
            ));
        }
        let surface =
            SurfaceDescriptor::new(map.width(), map.height(), layout, ColorDescription::NONE)
                .expect("scalar glyph surface");
        let image = EncodedImageView::from_sections(
            media,
            surface,
            EncodedSections {
                codings: self
                    .codings_section()
                    .and_then(|index| media.get(usize::from(index))),
                data: media.get(usize::from(self.data_section())),
                records: self
                    .groups_section()
                    .and_then(|index| media.get(usize::from(index))),
                indexes: self
                    .index_section()
                    .and_then(|index| media.get(usize::from(index))),
                color_table: None,
            },
            None,
        )
        .map_err(EncodedGlyphError::Encoding)?;
        Ok(EncodedGlyphs { map, image })
    }
}

/// Borrowed encoded glyph samples with shared static-image coding and coverage.
#[derive(Clone, Copy, Debug)]
pub struct EncodedGlyphs<'map, 'data> {
    map: GlyphMap<'map>,
    image: EncodedImageView<'data>,
}

impl<'map, 'data> EncodedGlyphs<'map, 'data> {
    pub(crate) fn preflight_in(
        self,
        preflight: &mut crate::image::RasterPreflight<'_>,
    ) -> Result<(), EncodedImageError> {
        preflight.image(self.image)
    }

    pub const fn map(self) -> GlyphMap<'map> {
        self.map
    }
    pub fn len(self) -> usize {
        self.map.len()
    }
    pub fn is_empty(self) -> bool {
        self.map.is_empty()
    }
    pub fn group_count(self) -> usize {
        self.image.group_count()
    }

    /// Declared input alignment, not actual address suitability or codec support.
    pub fn input_alignment(self) -> Result<u32, EncodedGlyphError> {
        self.image
            .input_alignment()
            .map_err(EncodedGlyphError::Encoding)
    }

    /// Retains the containing payload's file/Flash offset for group validation.
    pub const fn with_file_offset(mut self, offset: u32) -> Self {
        self.image = self.image.with_file_offset(offset);
        self
    }

    /// Checks glyph count, static coverage, scalar syntax and complete DATA integrity.
    /// Work and per-unit memory limits use the shared image preflight contract.
    pub fn preflight(self, limits: &PayloadLimits) -> Result<(), EncodedGlyphError> {
        if self.len() > limits.max_font_glyphs() as usize {
            return Err(EncodedGlyphError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: self.len(),
            });
        }
        self.image
            .preflight(limits)
            .map_err(EncodedGlyphError::Encoding)
    }

    /// Prepares groups in caller storage, retaining the matching glyph map.
    /// Capacity errors preserve slots; other failures may overwrite the used prefix.
    pub fn groups_into<'g>(
        self,
        workspace: &'g mut [Option<UnitGroup<'data>>],
        budget: &mut CoverageBudget,
    ) -> Result<GlyphGroups<'map, 'data, 'g>, EncodedGlyphError> {
        let groups = self
            .image
            .groups_into(workspace, budget)
            .map_err(EncodedGlyphError::Encoding)?;
        Ok(GlyphGroups {
            map: self.map,
            groups,
        })
    }
}

/// Prepared encoded groups bound to one glyph map, without decoded sample ownership.
#[derive(Clone, Copy, Debug)]
pub struct GlyphGroups<'map, 'data, 'g> {
    map: GlyphMap<'map>,
    groups: ImageGroups<'data, 'g>,
}

impl<'data, 'g> GlyphGroups<'_, 'data, 'g> {
    pub fn len(self) -> usize {
        self.map.len()
    }
    pub fn is_empty(self) -> bool {
        self.map.is_empty()
    }
    pub const fn group_count(self) -> usize {
        self.groups.len()
    }

    /// Plans one exact glyph region through shared unit preflight and cropped output.
    /// The result does not borrow map metadata; encoded DATA and prepared slots remain borrowed.
    pub fn decode_plan(
        self,
        index: usize,
        requirements: SurfaceRequirements,
        limits: &PayloadLimits,
    ) -> Result<ImageDecodePlan<'data, 'g>, EncodedGlyphError> {
        let region = self
            .map
            .get(index)
            .ok_or(EncodedGlyphError::GlyphOutOfBounds {
                index,
                count: self.len(),
            })?;
        self.groups
            .decode_region_plan(region, requirements, limits)
            .map_err(EncodedGlyphError::Decode)
    }
}

/// Invalid glyph binding, unsupported encoded execution or exhausted request limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EncodedGlyphError {
    ExpectedEncodedStorage,
    Record(GlyphSurfaceRecordError),
    Encoding(EncodedImageError),
    Decode(DecodeError),
    TooManyGlyphs { limit: u32, actual: usize },
    GlyphOutOfBounds { index: usize, count: usize },
}

#[cfg(test)]
mod tests;
