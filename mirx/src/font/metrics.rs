#![doc = include_str!("../../docs/font-metrics.md")]

use core::{iter::FusedIterator, slice::ChunksExact};

use crate::Fixed;
use crate::wire::{read_u32_le, write_u32_le};

pub const LINE_METRICS_LEN: usize = 12;
pub const GLYPH_METRICS_LEN: usize = 12;

/// Hinted line measurements in design-ppem pixels, represented as signed 24.8.
///
/// Ascent is nonnegative, descent is nonpositive, and line height is the
/// positive baseline advance. Line height need not equal ascent minus descent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineMetrics {
    ascent: Fixed,
    descent: Fixed,
    line_height: Fixed,
}

impl LineMetrics {
    pub const fn new(
        ascent: Fixed,
        descent: Fixed,
        line_height: Fixed,
    ) -> Result<Self, MetricsError> {
        if ascent.raw() < 0 {
            return Err(MetricsError::NegativeAscent(ascent));
        }
        if descent.raw() > 0 {
            return Err(MetricsError::PositiveDescent(descent));
        }
        if line_height.raw() <= 0 {
            return Err(MetricsError::NonPositiveLineHeight(line_height));
        }
        Ok(Self {
            ascent,
            descent,
            line_height,
        })
    }

    pub const fn ascent(self) -> Fixed {
        self.ascent
    }

    pub const fn descent(self) -> Fixed {
        self.descent
    }

    pub const fn line_height(self) -> Fixed {
        self.line_height
    }

    /// Reads one little-endian record without requiring an aligned address.
    pub fn from_record(bytes: &[u8]) -> Result<Self, MetricsError> {
        if bytes.len() < LINE_METRICS_LEN {
            return Err(MetricsError::Truncated {
                needed: LINE_METRICS_LEN,
                available: bytes.len(),
            });
        }
        Self::new(
            Fixed::from_raw(read_u32_le(bytes, 0).unwrap() as i32),
            Fixed::from_raw(read_u32_le(bytes, 4).unwrap() as i32),
            Fixed::from_raw(read_u32_le(bytes, 8).unwrap() as i32),
        )
    }

    /// Writes one record, preserving the output suffix and all bytes on error.
    pub fn encode_record_into(self, out: &mut [u8]) -> Result<usize, MetricsError> {
        if out.len() < LINE_METRICS_LEN {
            return Err(MetricsError::BufferTooSmall {
                needed: LINE_METRICS_LEN,
                available: out.len(),
            });
        }
        out[..LINE_METRICS_LEN].copy_from_slice(&self.encode_record());
        Ok(LINE_METRICS_LEN)
    }

    pub(crate) fn encode_record(self) -> [u8; LINE_METRICS_LEN] {
        let mut record = [0; LINE_METRICS_LEN];
        write_u32_le(&mut record, 0, self.ascent.raw() as u32);
        write_u32_le(&mut record, 4, self.descent.raw() as u32);
        write_u32_le(&mut record, 8, self.line_height.raw() as u32);
        record
    }
}

/// One glyph's hinted advance and complete raster origin in design-ppem pixels.
///
/// All fields use signed 24.8. Positive x points right; positive bearing y
/// points above the baseline. Bearings locate the complete mapped sample
/// rectangle, including any SDF border, not an ink box inside that rectangle.
/// Zero and negative advances are retained. The shared codepoint table owns
/// Unicode values; the glyph map or surface owns raster dimensions and storage.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GlyphMetrics {
    advance: Fixed,
    bearing_x: Fixed,
    bearing_y: Fixed,
}

impl GlyphMetrics {
    pub const fn new(advance: Fixed, bearing_x: Fixed, bearing_y: Fixed) -> Self {
        Self {
            advance,
            bearing_x,
            bearing_y,
        }
    }

    pub const fn advance(self) -> Fixed {
        self.advance
    }

    pub const fn bearing_x(self) -> Fixed {
        self.bearing_x
    }

    pub const fn bearing_y(self) -> Fixed {
        self.bearing_y
    }

    /// Reads one little-endian record; every complete signed tuple is valid.
    pub fn from_record(bytes: &[u8]) -> Result<Self, MetricsError> {
        if bytes.len() < GLYPH_METRICS_LEN {
            return Err(MetricsError::Truncated {
                needed: GLYPH_METRICS_LEN,
                available: bytes.len(),
            });
        }
        Ok(Self::new(
            Fixed::from_raw(read_u32_le(bytes, 0).unwrap() as i32),
            Fixed::from_raw(read_u32_le(bytes, 4).unwrap() as i32),
            Fixed::from_raw(read_u32_le(bytes, 8).unwrap() as i32),
        ))
    }

    /// Writes one record, preserving the output suffix and all bytes on error.
    pub fn encode_record_into(self, out: &mut [u8]) -> Result<usize, MetricsError> {
        if out.len() < GLYPH_METRICS_LEN {
            return Err(MetricsError::BufferTooSmall {
                needed: GLYPH_METRICS_LEN,
                available: out.len(),
            });
        }
        out[..GLYPH_METRICS_LEN].copy_from_slice(&self.encode_record());
        Ok(GLYPH_METRICS_LEN)
    }

    pub(crate) fn encode_record(self) -> [u8; GLYPH_METRICS_LEN] {
        let mut record = [0; GLYPH_METRICS_LEN];
        write_u32_le(&mut record, 0, self.advance.raw() as u32);
        write_u32_le(&mut record, 4, self.bearing_x.raw() as u32);
        write_u32_le(&mut record, 8, self.bearing_y.raw() as u32);
        record
    }
}

/// Borrowed line prefix and glyph records for one font representation.
///
/// Opening takes constant time: the prefix is validated and record count is
/// derived from length. Glyph records are read on demand, without aligned
/// casts, a decoded array or allocation. The complete face must check that
/// this count matches its shared codepoint table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetricsTable<'a> {
    bytes: &'a [u8],
}

impl<'a> MetricsTable<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, MetricsError> {
        LineMetrics::from_record(bytes)?;
        if (bytes.len() - LINE_METRICS_LEN) % GLYPH_METRICS_LEN != 0 {
            return Err(MetricsError::PartialRecord {
                byte_len: bytes.len(),
            });
        }
        Ok(Self { bytes })
    }

    pub const fn as_bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub fn line_metrics(self) -> LineMetrics {
        LineMetrics::from_record(self.bytes).expect("validated line metrics")
    }

    pub const fn len(self) -> usize {
        (self.bytes.len() - LINE_METRICS_LEN) / GLYPH_METRICS_LEN
    }

    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn get(self, index: usize) -> Option<GlyphMetrics> {
        let offset = index
            .checked_mul(GLYPH_METRICS_LEN)?
            .checked_add(LINE_METRICS_LEN)?;
        GlyphMetrics::from_record(self.bytes.get(offset..)?).ok()
    }

    pub fn iter(self) -> GlyphMetricsIter<'a> {
        GlyphMetricsIter {
            records: self.bytes[LINE_METRICS_LEN..].chunks_exact(GLYPH_METRICS_LEN),
        }
    }
}

impl<'a> IntoIterator for MetricsTable<'a> {
    type Item = GlyphMetrics;
    type IntoIter = GlyphMetricsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Exact-size glyph metric iteration with constant-time forward/backward skips.
#[derive(Clone, Debug)]
pub struct GlyphMetricsIter<'a> {
    records: ChunksExact<'a, u8>,
}

impl GlyphMetricsIter<'_> {
    fn record(bytes: &[u8]) -> GlyphMetrics {
        GlyphMetrics::from_record(bytes).expect("complete glyph metric record")
    }
}

impl Iterator for GlyphMetricsIter<'_> {
    type Item = GlyphMetrics;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(Self::record)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.records.nth(n).map(Self::record)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.records.size_hint()
    }

    fn count(self) -> usize {
        self.records.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for GlyphMetricsIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.records.next_back().map(Self::record)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.records.nth_back(n).map(Self::record)
    }
}

impl ExactSizeIterator for GlyphMetricsIter<'_> {
    fn len(&self) -> usize {
        self.records.len()
    }
}

impl FusedIterator for GlyphMetricsIter<'_> {}

/// Invalid line semantics, incomplete records, or insufficient output space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MetricsError {
    Truncated { needed: usize, available: usize },
    BufferTooSmall { needed: usize, available: usize },
    PartialRecord { byte_len: usize },
    NegativeAscent(Fixed),
    PositiveDescent(Fixed),
    NonPositiveLineHeight(Fixed),
}

#[cfg(test)]
mod tests;
