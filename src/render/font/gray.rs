//! Grayscale-coverage font reader.
//!
//! A grayscale atlas lives inside a mirx `chunk_type::FONT` payload
//! that starts with a [`FontChunkHeader`] (kind = Grayscale), then the
//! same body layout as the SDF reader: an [`AtlasHeader`], a
//! codepoint-sorted [`GlyphMetric`] table, and packed coverage bytes
//! (`bytes_per_glyph` per glyph, `bpp`-bit MSB-first, row-major). The
//! stored values are alpha, not signed distances.

use alloc::rc::Rc;
use alloc::vec::Vec;

use super::chunk::{FONT_CHUNK_HEADER_LEN, FontChunkHeader, FontChunkKind};
use super::sdf::{
    AtlasHeader, GlyphMetric, HEADER_LEN, METRIC_LEN, SUPPORTED_VERSION, read_header_unaligned,
    read_metric_unaligned,
};
use super::{Font, FontBackend, FontMetrics, FontProvider, Glyph, GlyphKind};

/// Why a grayscale payload was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrayFontError {
    /// Payload smaller than the chunk prefix + body header.
    PayloadTooShort,
    /// Prefix is missing or its `kind` byte is not Grayscale.
    NotGrayscale,
    /// Header `version` doesn't match this build.
    UnsupportedVersion(u16),
    /// `bit_depth` must be 1, 2, 4, or 8.
    InvalidBitDepth(u8),
    /// `source_size` is zero or `bytes_per_glyph` disagrees with it.
    InvalidGeometry,
    /// A declared offset/length falls outside the payload.
    OffsetsOutOfBounds,
}

/// Grayscale font provider backed by a borrowed mirx chunk payload.
/// Like [`super::sdf::SdfFontProvider`] it borrows the bulky coverage
/// bytes from flash and only copies the small header + metric table.
#[derive(Clone, Debug)]
pub struct GrayFontProvider {
    header: AtlasHeader,
    metrics: Vec<GlyphMetric>,
    data: &'static [u8],
}

impl GrayFontProvider {
    /// Parse a grayscale atlas from a FONT payload whose prefix marks
    /// it Grayscale. `body` offsets are relative to the body start
    /// (after the 4-byte prefix), matching the writer.
    pub fn from_mirx_chunk(payload: &'static [u8]) -> Result<Self, GrayFontError> {
        let prefix = FontChunkHeader::parse(payload).ok_or(GrayFontError::PayloadTooShort)?;
        if prefix.kind != FontChunkKind::Grayscale {
            return Err(GrayFontError::NotGrayscale);
        }
        let body = &payload[FONT_CHUNK_HEADER_LEN..];
        if body.len() < HEADER_LEN {
            return Err(GrayFontError::PayloadTooShort);
        }
        let header = read_header_unaligned(&body[..HEADER_LEN]);

        if header.version != SUPPORTED_VERSION {
            return Err(GrayFontError::UnsupportedVersion(header.version));
        }
        if !matches!(header.bit_depth, 1 | 2 | 4 | 8) {
            return Err(GrayFontError::InvalidBitDepth(header.bit_depth));
        }
        let source = header.source_size as u32;
        if source == 0 {
            return Err(GrayFontError::InvalidGeometry);
        }
        let expected_per_glyph = (source * source * header.bit_depth as u32).div_ceil(8);
        if header.bytes_per_glyph != expected_per_glyph {
            return Err(GrayFontError::InvalidGeometry);
        }

        let metric_off = header.metric_offset as usize;
        let data_off = header.data_offset as usize;
        let metric_end = metric_off
            .checked_add(
                (header.glyph_count as usize)
                    .checked_mul(METRIC_LEN)
                    .ok_or(GrayFontError::OffsetsOutOfBounds)?,
            )
            .ok_or(GrayFontError::OffsetsOutOfBounds)?;
        let data_end = data_off
            .checked_add(
                (header.glyph_count as usize)
                    .checked_mul(header.bytes_per_glyph as usize)
                    .ok_or(GrayFontError::OffsetsOutOfBounds)?,
            )
            .ok_or(GrayFontError::OffsetsOutOfBounds)?;
        if metric_end > body.len() || data_end > body.len() {
            return Err(GrayFontError::OffsetsOutOfBounds);
        }
        if metric_off < HEADER_LEN || data_off < HEADER_LEN {
            return Err(GrayFontError::OffsetsOutOfBounds);
        }

        let mut metrics = Vec::with_capacity(header.glyph_count as usize);
        for i in 0..header.glyph_count as usize {
            let off = metric_off + i * METRIC_LEN;
            metrics.push(read_metric_unaligned(&body[off..off + METRIC_LEN]));
        }
        let data = &body[data_off..data_end];

        Ok(Self {
            header,
            metrics,
            data,
        })
    }

    pub fn header(&self) -> &AtlasHeader {
        &self.header
    }
}

impl FontProvider for GrayFontProvider {
    // One grayscale table is baked at a fixed pixel size, so the
    // request size doesn't pick among tables here — a multi-table
    // provider does that one level up.
    fn glyph(&self, ch: char, _requested_size: u16) -> Option<Glyph> {
        let cp = ch as u32;
        let idx = self
            .metrics
            .binary_search_by_key(&cp, |m| m.codepoint)
            .ok()?;
        let m = self.metrics[idx];
        let stride = self.header.bytes_per_glyph as usize;
        let start = idx * stride;
        let coverage = &self.data[start..start + stride];
        let cell = self.header.source_size.min(255) as u8;
        Some(Glyph {
            advance: m.advance,
            kind: GlyphKind::Grayscale {
                coverage,
                bpp: self.header.bit_depth,
                w: cell,
                h: cell,
                bearing_x: m.bearing_x,
                bearing_y: m.bearing_y,
            },
        })
    }

    fn metrics(&self) -> FontMetrics {
        FontMetrics {
            ascender: self.header.ascender,
            descender: self.header.descender,
            line_height: self.header.line_height,
        }
    }
}

/// Wrap a parsed grayscale atlas into a [`Font`] ready to register.
pub fn font_from_mirx_chunk(
    family: &'static str,
    payload: &'static [u8],
) -> Result<Font, GrayFontError> {
    let provider = GrayFontProvider::from_mirx_chunk(payload)?;
    let size = provider.header.source_size;
    Ok(Font {
        family,
        size,
        backend: FontBackend::Custom(Rc::new(provider)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // 4x4 source, 4-bit → 8 bytes per glyph. Builds a prefixed
    // grayscale payload matching the writer layout.
    fn make_gray_atlas(glyphs: &[(u32, u16, i8, i8)]) -> Vec<u8> {
        let source_size: u16 = 4;
        let bit_depth: u8 = 4;
        let bytes_per_glyph =
            (source_size as u32 * source_size as u32 * bit_depth as u32).div_ceil(8);
        let glyph_count = glyphs.len() as u32;
        let metric_offset = HEADER_LEN as u32;
        let data_offset = metric_offset + glyph_count * METRIC_LEN as u32;
        let body_len = data_offset as usize + (glyph_count as usize) * bytes_per_glyph as usize;

        let mut body = vec![0u8; body_len];
        let header = AtlasHeader {
            version: SUPPORTED_VERSION,
            bit_depth,
            _pad0: 0,
            source_size,
            spread: 0,
            glyph_count,
            metric_offset,
            data_offset,
            bytes_per_glyph,
            ascender: 3,
            descender: 1,
            line_height: 4,
            _pad1: 0,
        };
        unsafe {
            core::ptr::write_unaligned(body.as_mut_ptr() as *mut AtlasHeader, header);
        }
        let mut sorted = glyphs.to_vec();
        sorted.sort_by_key(|g| g.0);
        for (i, (cp, advance, bx, by)) in sorted.iter().enumerate() {
            let off = metric_offset as usize + i * METRIC_LEN;
            let m = GlyphMetric {
                codepoint: *cp,
                advance: *advance,
                bearing_x: *bx,
                bearing_y: *by,
            };
            unsafe {
                core::ptr::write_unaligned(body.as_mut_ptr().add(off) as *mut GlyphMetric, m);
            }
        }
        for i in 0..glyph_count as usize {
            let start = data_offset as usize + i * bytes_per_glyph as usize;
            for b in &mut body[start..start + bytes_per_glyph as usize] {
                *b = ((i + 1) as u8).wrapping_mul(0x11);
            }
        }

        let mut payload = vec![0u8; FONT_CHUNK_HEADER_LEN + body.len()];
        FontChunkHeader {
            kind: FontChunkKind::Grayscale,
            format: bit_depth,
            size: source_size,
        }
        .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
        payload[FONT_CHUNK_HEADER_LEN..].copy_from_slice(&body);
        payload
    }

    fn leak(bytes: Vec<u8>) -> &'static [u8] {
        Vec::leak(bytes)
    }

    #[test]
    fn parses_prefixed_grayscale_and_resolves_glyph() {
        let bytes = leak(make_gray_atlas(&[
            (b'A' as u32, 5, 1, 2),
            (b'B' as u32, 6, -1, 0),
        ]));
        let p = GrayFontProvider::from_mirx_chunk(bytes).expect("parse");
        assert_eq!(p.header().glyph_count, 2);

        let g = p.glyph('A', 16).expect("A");
        assert_eq!(g.advance, 5);
        match g.kind {
            GlyphKind::Grayscale {
                bpp,
                w,
                h,
                coverage,
                bearing_x,
                bearing_y,
            } => {
                assert_eq!(bpp, 4);
                assert_eq!((w, h), (4, 4));
                assert_eq!(coverage.len(), 8);
                assert_eq!((bearing_x, bearing_y), (1, 2));
            }
            other => panic!("expected Grayscale, got {other:?}"),
        }
    }

    #[test]
    fn missing_codepoint_returns_none() {
        let bytes = leak(make_gray_atlas(&[(b'A' as u32, 5, 0, 0)]));
        let p = GrayFontProvider::from_mirx_chunk(bytes).expect("parse");
        assert!(p.glyph('Z', 16).is_none());
    }

    #[test]
    fn rejects_sdf_prefix() {
        let mut bytes = make_gray_atlas(&[(b'A' as u32, 5, 0, 0)]);
        bytes[0] = FontChunkKind::Sdf.to_u8();
        assert!(matches!(
            GrayFontProvider::from_mirx_chunk(leak(bytes)),
            Err(GrayFontError::NotGrayscale)
        ));
    }

    #[test]
    fn rejects_short_payload() {
        assert!(matches!(
            GrayFontProvider::from_mirx_chunk(leak(vec![0u8; 2])),
            Err(GrayFontError::PayloadTooShort)
        ));
    }

    #[test]
    fn font_wrapper_reports_source_size() {
        let bytes = leak(make_gray_atlas(&[(b'A' as u32, 5, 0, 0)]));
        let font = font_from_mirx_chunk("gray-test", bytes).expect("font");
        assert_eq!(font.family, "gray-test");
        assert_eq!(font.size, 4);
    }
}
