#![doc = include_str!("../../docs/glyph-lookup.md")]

use super::{FontCodepoints, GlyphMetrics, GlyphRaster, LineMetrics, MetricsTable, RawGlyphs};

/// One representation's joined Unicode, metric and RAW sample tables.
///
/// Construction establishes shared ordinal ownership without copying records.
/// Font size and coverage/SDF selection belong to the containing representation.
#[derive(Clone, Copy, Debug)]
pub struct GlyphTable<'meta, 'map, 'data> {
    codepoints: FontCodepoints<'meta>,
    metrics: MetricsTable<'meta>,
    glyphs: RawGlyphs<'map, 'data>,
}

impl<'meta, 'map, 'data> GlyphTable<'meta, 'map, 'data> {
    pub fn new(
        codepoints: FontCodepoints<'meta>,
        metrics: MetricsTable<'meta>,
        glyphs: RawGlyphs<'map, 'data>,
    ) -> Result<Self, GlyphTableError> {
        if codepoints.len() != metrics.len() || codepoints.len() != glyphs.len() {
            return Err(GlyphTableError::Cardinality {
                codepoints: codepoints.len(),
                metrics: metrics.len(),
                glyphs: glyphs.len(),
            });
        }
        Ok(Self {
            codepoints,
            metrics,
            glyphs,
        })
    }

    pub const fn len(self) -> usize {
        self.codepoints.len()
    }

    pub const fn is_empty(self) -> bool {
        self.codepoints.is_empty()
    }

    pub const fn codepoints(self) -> FontCodepoints<'meta> {
        self.codepoints
    }

    /// Returns this table's hinted baseline measurements at design ppem.
    pub fn line_metrics(self) -> LineMetrics {
        self.metrics.line_metrics()
    }

    /// Resolves one shared ordinal in constant time, with no metadata lifetime.
    pub fn get(self, ordinal: usize) -> Option<Glyph<'data>> {
        Some(Glyph {
            raster: self.glyphs.get(ordinal)?,
            metrics: self
                .metrics
                .get(ordinal)
                .expect("validated glyph cardinality"),
        })
    }

    /// Searches the shared Unicode directory; missing characters remain absent.
    pub fn glyph(self, codepoint: char) -> Option<Glyph<'data>> {
        self.get(self.codepoints.binary_search(codepoint).ok()?)
    }
}

/// Copy metric metadata and borrowed raster samples for one resolved glyph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Glyph<'data> {
    metrics: GlyphMetrics,
    raster: GlyphRaster<'data>,
}

impl<'data> Glyph<'data> {
    pub const fn metrics(self) -> GlyphMetrics {
        self.metrics
    }

    pub const fn raster(self) -> GlyphRaster<'data> {
        self.raster
    }
}

/// Independently valid tables do not describe the same set of glyph ordinals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlyphTableError {
    Cardinality {
        codepoints: usize,
        metrics: usize,
        glyphs: usize,
    },
}

#[cfg(test)]
mod tests;
