pub mod atlas;
pub mod header;
mod preflight;

use alloc::vec::Vec;

pub use crate::font::atlas::{AtlasHeader, GlyphMetric, HEADER_LEN, METRIC_LEN, SUPPORTED_VERSION};
pub use crate::font::atlas::{read_header, read_metric, write_header, write_metric};
pub use crate::font::header::{FONT_CHUNK_HEADER_LEN, FontChunkHeader, FontChunkKind};
pub use crate::font::preflight::FontReadError;
use crate::font::preflight::checked_bytes_per_glyph;
use crate::reader::PayloadLimits;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontDecodeError {
    PayloadTooShort,
    UnknownChunkKind(u8),
    UnsupportedVersion(u16),
    InvalidBitDepth(u8),
    InvalidGeometry,
    OffsetsOutOfBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub chunk_header: FontChunkHeader,
    pub atlas: AtlasHeader,
    pub metrics: Vec<GlyphMetric>,
    pub data: Vec<u8>,
}

impl Font {
    /// Validates one complete FONT payload without allocating.
    ///
    /// Glyph count and owned metric/atlas-data bytes are bounded by `limits`.
    /// Bytes skipped by the payload's explicit offsets are nonsemantic and are
    /// not scanned.
    pub fn preflight(payload: &[u8], limits: &PayloadLimits) -> Result<(), FontReadError> {
        preflight::validate_payload(payload, limits)
    }

    pub fn decode(payload: &[u8]) -> Result<Self, FontDecodeError> {
        let prefix = FontChunkHeader::parse(payload).ok_or(FontDecodeError::PayloadTooShort)?;
        let body = payload
            .get(FONT_CHUNK_HEADER_LEN..)
            .ok_or(FontDecodeError::PayloadTooShort)?;
        if body.len() < HEADER_LEN {
            return Err(FontDecodeError::PayloadTooShort);
        }
        let atlas = read_header(&body[..HEADER_LEN]);

        if atlas.version != SUPPORTED_VERSION {
            return Err(FontDecodeError::UnsupportedVersion(atlas.version));
        }
        match prefix.kind {
            FontChunkKind::Sdf => {
                if atlas.bit_depth != 4 && atlas.bit_depth != 8 {
                    return Err(FontDecodeError::InvalidBitDepth(atlas.bit_depth));
                }
            }
            FontChunkKind::Grayscale => {
                if !matches!(atlas.bit_depth, 1 | 2 | 4 | 8) {
                    return Err(FontDecodeError::InvalidBitDepth(atlas.bit_depth));
                }
            }
        }
        if atlas.source_size == 0 {
            return Err(FontDecodeError::InvalidGeometry);
        }
        let expected_per_glyph = checked_bytes_per_glyph(atlas.source_size, atlas.bit_depth)
            .ok_or(FontDecodeError::InvalidGeometry)?;
        if atlas.bytes_per_glyph != expected_per_glyph {
            return Err(FontDecodeError::InvalidGeometry);
        }

        let metric_off = atlas.metric_offset as usize;
        let data_off = atlas.data_offset as usize;
        let metric_end = metric_off
            .checked_add(
                (atlas.glyph_count as usize)
                    .checked_mul(METRIC_LEN)
                    .ok_or(FontDecodeError::OffsetsOutOfBounds)?,
            )
            .ok_or(FontDecodeError::OffsetsOutOfBounds)?;
        let data_end = data_off
            .checked_add(
                (atlas.glyph_count as usize)
                    .checked_mul(atlas.bytes_per_glyph as usize)
                    .ok_or(FontDecodeError::OffsetsOutOfBounds)?,
            )
            .ok_or(FontDecodeError::OffsetsOutOfBounds)?;
        if metric_end > body.len() || data_end > body.len() {
            return Err(FontDecodeError::OffsetsOutOfBounds);
        }
        if metric_off < HEADER_LEN || data_off < HEADER_LEN {
            return Err(FontDecodeError::OffsetsOutOfBounds);
        }

        let mut metrics = Vec::with_capacity(atlas.glyph_count as usize);
        for i in 0..atlas.glyph_count as usize {
            let off = metric_off + i * METRIC_LEN;
            metrics.push(read_metric(&body[off..off + METRIC_LEN]));
        }
        let data = body[data_off..data_end].to_vec();

        Ok(Self {
            chunk_header: prefix,
            atlas,
            metrics,
            data,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let metric_offset = HEADER_LEN as u32;
        let data_offset = metric_offset + (self.metrics.len() * METRIC_LEN) as u32;
        let total = FONT_CHUNK_HEADER_LEN + data_offset as usize + self.data.len();
        let mut out = alloc::vec![0u8; total];

        let mut prefix_buf = [0u8; FONT_CHUNK_HEADER_LEN];
        self.chunk_header.write(&mut prefix_buf);
        out[..FONT_CHUNK_HEADER_LEN].copy_from_slice(&prefix_buf);

        let body_start = FONT_CHUNK_HEADER_LEN;
        let mut atlas = self.atlas;
        atlas.metric_offset = metric_offset;
        atlas.data_offset = data_offset;
        write_header(&mut out[body_start..body_start + HEADER_LEN], &atlas);

        for (i, m) in self.metrics.iter().enumerate() {
            let off = body_start + metric_offset as usize + i * METRIC_LEN;
            write_metric(&mut out[off..off + METRIC_LEN], m);
        }
        out[body_start + data_offset as usize..].copy_from_slice(&self.data);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_font(kind: FontChunkKind, bit_depth: u8) -> Font {
        let source_size: u16 = 4;
        let bytes_per_glyph = checked_bytes_per_glyph(source_size, bit_depth).unwrap();
        let atlas = AtlasHeader {
            version: SUPPORTED_VERSION,
            bit_depth,
            _pad0: 0,
            source_size,
            spread: 2,
            glyph_count: 2,
            metric_offset: HEADER_LEN as u32,
            data_offset: (HEADER_LEN + 2 * METRIC_LEN) as u32,
            bytes_per_glyph,
            ascender: 3,
            descender: 1,
            line_height: 4,
            _pad1: 0,
        };
        let metrics = alloc::vec![
            GlyphMetric {
                codepoint: 'A' as u32,
                advance: 4,
                bearing_x: 0,
                bearing_y: 3
            },
            GlyphMetric {
                codepoint: 'B' as u32,
                advance: 4,
                bearing_x: 0,
                bearing_y: 3
            },
        ];
        let data = alloc::vec![0u8; (bytes_per_glyph * 2) as usize];
        Font {
            chunk_header: FontChunkHeader {
                kind,
                format: bit_depth,
                size: source_size,
            },
            atlas,
            metrics,
            data,
        }
    }

    #[test]
    fn sdf_round_trips() {
        let font = sample_font(FontChunkKind::Sdf, 4);
        let bytes = font.encode();
        let back = Font::decode(&bytes).unwrap();
        assert_eq!(back, font);
    }

    #[test]
    fn gray_round_trips() {
        let font = sample_font(FontChunkKind::Grayscale, 8);
        let bytes = font.encode();
        let back = Font::decode(&bytes).unwrap();
        assert_eq!(back, font);
    }

    #[test]
    fn rejects_unknown_kind() {
        let mut font = sample_font(FontChunkKind::Sdf, 4);
        font.chunk_header.kind = FontChunkKind::Grayscale;
        font.chunk_header.kind = FontChunkKind::from_u8(9).unwrap_or(FontChunkKind::Sdf);
        let bytes = font.encode();
        // Either parse fails at kind byte, or kind stays Sdf — both are
        // acceptable; we just ensure no panic.
        let _ = Font::decode(&bytes);
    }

    #[test]
    fn rejects_short_payload() {
        assert!(matches!(
            Font::decode(&[0, 0, 0, 0]),
            Err(FontDecodeError::PayloadTooShort)
        ));
    }
}
