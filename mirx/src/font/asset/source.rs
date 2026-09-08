use super::{
    CmapEntry, FontAdvanceSource, FontAsset, FontError, FontFace, GlyphId, GlyphMap,
    GlyphSurfaceAsset, Plan, RasterMetrics, RepresentationAsset, Storage,
};
use crate::{PayloadLimits, image::RasterPreflight, media::INTEGRITY_RECORD_LEN};

/// Inline access for canonical emission; implementations retain metadata storage.
pub(in crate::font) trait Source: Copy {
    fn face(&self) -> FontFace;
    fn cmap(&self) -> &[CmapEntry];
    fn glyph_ids(&self) -> Option<&[GlyphId]>;
    fn advance_source(&self) -> FontAdvanceSource<'_>;
    fn raster_metrics(&self) -> &[RasterMetrics];
    fn representation_count(&self) -> usize;
    fn surface_count(&self) -> usize;
    fn map_count(&self) -> usize;
    fn representation(&self, index: usize) -> Option<RepresentationAsset>;
    fn surface(&self, index: usize) -> Option<GlyphSurfaceAsset<'_>>;
    fn map(&self, index: usize) -> Option<GlyphMap<'_>>;

    fn representations(&self) -> impl ExactSizeIterator<Item = RepresentationAsset> {
        (0..self.representation_count())
            .map(|index| self.representation(index).expect("representation ordinal"))
    }
    fn surfaces(&self) -> impl ExactSizeIterator<Item = GlyphSurfaceAsset<'_>> {
        (0..self.surface_count()).map(|index| self.surface(index).expect("surface ordinal"))
    }
    fn maps(&self) -> impl ExactSizeIterator<Item = GlyphMap<'_>> {
        (0..self.map_count()).map(|index| self.map(index).expect("map ordinal"))
    }

    fn preflight(self, limits: &PayloadLimits) -> Result<(), FontError> {
        let glyphs = usize::from(self.face().raster_count());
        if glyphs > limits.max_font_glyphs() as usize {
            return Err(FontError::TooManyGlyphs {
                limit: limits.max_font_glyphs(),
                actual: glyphs,
            });
        }
        if self.cmap().len() > limits.max_font_cmap_entries() as usize {
            return Err(FontError::TooManyCmapEntries {
                limit: limits.max_font_cmap_entries(),
                actual: self.cmap().len(),
            });
        }
        if self.representation_count() > limits.max_font_representations() as usize {
            return Err(FontError::TooManyRepresentations {
                limit: limits.max_font_representations(),
                actual: self.representation_count(),
            });
        }
        if let FontAdvanceSource::Shaping(bytes) = self.advance_source()
            && bytes.len() > limits.max_font_shaping_bytes()
        {
            return Err(FontError::ShapingBytesLimitExceeded {
                limit: limits.max_font_shaping_bytes(),
                actual: bytes.len(),
            });
        }
        Plan::table_size(self)?;
        let mut preflight = RasterPreflight::new(limits, 0).map_err(FontError::Raster)?;
        let charge = |p: &mut RasterPreflight<'_>, n| p.spend(n).map_err(FontError::Raster);
        let count = self.representation_count() as u64;
        let metadata_work = count
            .checked_mul(
                count + glyphs as u64 + self.surface_count() as u64 + self.map_count() as u64,
            )
            .and_then(|n| n.checked_add(self.cmap().len() as u64))
            .ok_or(FontError::SizeOverflow)?;
        charge(&mut preflight, metadata_work)?;
        for surface in self.surfaces() {
            charge(
                &mut preflight,
                surface
                    .integrity()
                    .partitions()
                    .map_or(0, |p| p.len() as u64 * INTEGRITY_RECORD_LEN as u64),
            )?;
            if let GlyphSurfaceAsset::Encoded { image, .. } = surface {
                charge(&mut preflight, image.codings().len() as u64 * 8)?;
                let bytes = image
                    .codings()
                    .try_fold(0u64, |n, c| n.checked_add(c.params().len() as u64))
                    .ok_or(FontError::SizeOverflow)?;
                charge(
                    &mut preflight,
                    bytes
                        + image.groups().map_or(0, |g| g.len() as u64 * 36)
                        + image.unit_index().len() as u64,
                )?;
            }
        }
        let plan = Plan::metadata(self)?;
        charge(&mut preflight, plan.len as u64)?;
        for surface in self.surfaces() {
            if let Storage::Encoded(storage) = surface.plan()? {
                storage
                    .preflight_in(&mut preflight)
                    .map_err(FontError::Image)?;
            }
        }
        Ok(())
    }
}

impl Source for FontAsset<'_> {
    fn face(&self) -> FontFace {
        self.face
    }
    fn cmap(&self) -> &[CmapEntry] {
        self.cmap
    }
    fn glyph_ids(&self) -> Option<&[GlyphId]> {
        self.glyph_ids
    }
    fn advance_source(&self) -> FontAdvanceSource<'_> {
        self.advance_source
    }
    fn raster_metrics(&self) -> &[RasterMetrics] {
        self.raster_metrics
    }
    fn representation_count(&self) -> usize {
        self.representations.len()
    }
    fn surface_count(&self) -> usize {
        self.surfaces.len()
    }
    fn map_count(&self) -> usize {
        self.maps.len()
    }
    fn representation(&self, index: usize) -> Option<RepresentationAsset> {
        self.representations.get(index).copied()
    }
    fn surface(&self, index: usize) -> Option<GlyphSurfaceAsset<'_>> {
        self.surfaces.get(index).copied()
    }
    fn map(&self, index: usize) -> Option<GlyphMap<'_>> {
        self.maps.get(index).copied()
    }
}
