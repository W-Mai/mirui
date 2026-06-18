//! Signed-distance-field font reader.
//!
//! An SDF atlas lives inside a mirx `chunk_type::FONT` payload. The
//! payload starts with [`AtlasHeader`], followed by a `glyph_count` ×
//! [`GlyphMetric`] table sorted by codepoint, then the packed
//! distance bytes (`bytes_per_glyph` per glyph, row-major,
//! `source_size × source_size`).

use crate::render::font::chunk::{FONT_CHUNK_HEADER_LEN, FontChunkHeader};
use crate::render::font::{Font, FontBackend, FontMetrics, FontProvider, Glyph, GlyphKind};
use alloc::rc::Rc;

pub const SUPPORTED_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 32;
pub const METRIC_LEN: usize = 8;

/// Header of an SDF atlas chunk payload. `#[repr(C)]` so a parser can
/// read fields directly off `&[u8]` after a length check.
///
/// No magic — the outer mirx file header already validates with CRC32
/// and the `chunk_type::FONT` discriminator selects this layout.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AtlasHeader {
    pub version: u16,
    /// 4 (4-bit) or 8 (8-bit) — pixels per byte derived from this.
    pub bit_depth: u8,
    pub _pad0: u8,
    /// Square source size, e.g. 32 → 32×32 per glyph.
    pub source_size: u16,
    /// Pixels of zero-distance band around the glyph edge. Larger
    /// `spread` lets the renderer draw thicker outlines / drop shadows
    /// without sampling outside the atlas.
    pub spread: u16,
    pub glyph_count: u32,
    /// Offset from payload start to `GlyphMetric[0]`.
    pub metric_offset: u32,
    /// Offset from payload start to first SDF pixel byte.
    pub data_offset: u32,
    /// Byte size of one glyph's distance buffer (e.g.
    /// `32 × 32 × 4-bit ÷ 8 = 512`).
    pub bytes_per_glyph: u32,
    /// Recommended baseline metrics in `source_size` pixels.
    pub ascender: u16,
    pub descender: u16,
    pub line_height: u16,
    pub _pad1: u16,
}

/// Per-glyph entry in the metric table. Codepoint sorted so the runtime
/// finds glyphs with binary search.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetric {
    pub codepoint: u32,
    /// Advance width in 1/64 px (matches the FreeType convention).
    pub advance: u16,
    pub bearing_x: i8,
    pub bearing_y: i8,
}

const _: () = assert!(core::mem::size_of::<AtlasHeader>() == HEADER_LEN);
const _: () = assert!(core::mem::size_of::<GlyphMetric>() == METRIC_LEN);

/// Why an atlas payload was rejected. Distinct variants exist so the
/// caller can log a useful message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdfFontError {
    /// Payload smaller than the chunk prefix + fixed header.
    PayloadTooShort,
    /// Header `version` field doesn't match what this build understands.
    UnsupportedVersion(u16),
    /// `bit_depth` must be 4 or 8.
    InvalidBitDepth(u8),
    /// `source_size` is zero, or `bytes_per_glyph` doesn't match
    /// `source_size² × bit_depth ÷ 8`.
    InvalidGeometry,
    /// `metric_offset` or `data_offset` references bytes outside the
    /// payload, or the metric / data ranges overlap improperly.
    OffsetsOutOfBounds,
}

/// SDF font provider backed by a borrowed mirx chunk payload.
///
/// `data` (the bulky distance buffer) is borrowed directly from the
/// `&'static [u8]` payload — no allocation, the atlas stays in flash.
/// `header` is copied (one struct) and `metrics` is cloned into a
/// small `Vec` because `include_bytes!` does not guarantee 4-byte
/// alignment, and an unaligned `&[GlyphMetric]` slice would be UB on
/// targets that fault on misaligned loads.
#[derive(Clone, Debug)]
pub struct SdfFontProvider {
    header: AtlasHeader,
    metrics: alloc::vec::Vec<GlyphMetric>,
    data: &'static [u8],
}

impl SdfFontProvider {
    /// Parse an SDF atlas from a prefixed mirx `chunk_type::FONT`
    /// payload. The payload starts with a [`FontChunkHeader`] (kind =
    /// Sdf); the [`AtlasHeader`] and body follow it.
    pub fn from_mirx_chunk(payload: &'static [u8]) -> Result<Self, SdfFontError> {
        FontChunkHeader::parse(payload).ok_or(SdfFontError::PayloadTooShort)?;
        let body = &payload[FONT_CHUNK_HEADER_LEN..];
        if body.len() < HEADER_LEN {
            return Err(SdfFontError::PayloadTooShort);
        }
        let header = read_header_unaligned(&body[..HEADER_LEN]);

        if header.version != SUPPORTED_VERSION {
            return Err(SdfFontError::UnsupportedVersion(header.version));
        }
        if header.bit_depth != 4 && header.bit_depth != 8 {
            return Err(SdfFontError::InvalidBitDepth(header.bit_depth));
        }
        let source = header.source_size as u32;
        if source == 0 {
            return Err(SdfFontError::InvalidGeometry);
        }
        let expected_per_glyph = (source * source * header.bit_depth as u32).div_ceil(8);
        if header.bytes_per_glyph != expected_per_glyph {
            return Err(SdfFontError::InvalidGeometry);
        }

        let metric_off = header.metric_offset as usize;
        let data_off = header.data_offset as usize;
        let metric_end = metric_off
            .checked_add(
                (header.glyph_count as usize)
                    .checked_mul(METRIC_LEN)
                    .ok_or(SdfFontError::OffsetsOutOfBounds)?,
            )
            .ok_or(SdfFontError::OffsetsOutOfBounds)?;
        let data_end = data_off
            .checked_add(
                (header.glyph_count as usize)
                    .checked_mul(header.bytes_per_glyph as usize)
                    .ok_or(SdfFontError::OffsetsOutOfBounds)?,
            )
            .ok_or(SdfFontError::OffsetsOutOfBounds)?;

        if metric_end > body.len() || data_end > body.len() {
            return Err(SdfFontError::OffsetsOutOfBounds);
        }
        if metric_off < HEADER_LEN || data_off < HEADER_LEN {
            return Err(SdfFontError::OffsetsOutOfBounds);
        }

        let mut metrics = alloc::vec::Vec::with_capacity(header.glyph_count as usize);
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

    /// Header view — exposed so the renderer can read `source_size` /
    /// `bit_depth` / `spread` without unpacking a full glyph.
    pub fn header(&self) -> &AtlasHeader {
        &self.header
    }

    /// Codepoint table — useful for tools that walk every glyph.
    pub fn metrics(&self) -> &[GlyphMetric] {
        &self.metrics
    }

    /// Raw distance buffer covering all glyphs, indexed by
    /// `glyph_index * bytes_per_glyph`.
    pub fn data(&self) -> &'static [u8] {
        self.data
    }
}

pub(crate) fn read_header_unaligned(buf: &[u8]) -> AtlasHeader {
    let u16le = |o: usize| u16::from_le_bytes([buf[o], buf[o + 1]]);
    let u32le = |o: usize| u32::from_le_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
    AtlasHeader {
        version: u16le(0),
        bit_depth: buf[2],
        _pad0: buf[3],
        source_size: u16le(4),
        spread: u16le(6),
        glyph_count: u32le(8),
        metric_offset: u32le(12),
        data_offset: u32le(16),
        bytes_per_glyph: u32le(20),
        ascender: u16le(24),
        descender: u16le(26),
        line_height: u16le(28),
        _pad1: u16le(30),
    }
}

pub(crate) fn read_metric_unaligned(buf: &[u8]) -> GlyphMetric {
    GlyphMetric {
        codepoint: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        advance: u16::from_le_bytes([buf[4], buf[5]]),
        bearing_x: buf[6] as i8,
        bearing_y: buf[7] as i8,
    }
}

impl FontProvider for SdfFontProvider {
    // SDF scales one source atlas to any target, so the request size
    // doesn't select a table here.
    fn glyph(&self, ch: char) -> Option<Glyph> {
        let cp = ch as u32;
        let idx = self
            .metrics
            .binary_search_by_key(&cp, |m| m.codepoint)
            .ok()?;
        let m = self.metrics[idx];
        let stride = self.header.bytes_per_glyph as usize;
        let start = idx * stride;
        let atlas = &self.data[start..start + stride];
        Some(Glyph {
            advance: m.advance,
            kind: GlyphKind::Sdf {
                atlas,
                source_size: self.header.source_size,
                bit_depth: self.header.bit_depth,
                spread: self.header.spread,
                bbox_w: self.header.source_size.min(255) as u8,
                bbox_h: self.header.source_size.min(255) as u8,
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

/// Read one packed distance value from `atlas` at integer coordinate
/// `(x, y)` in `source_size`-pixel space. Returns the raw quantized
/// distance:
///
/// - `bit_depth = 4` → low nibble of byte `(y * stride + x) >> 1`,
///   high nibble for odd `x` indices. Range `0..=15`.
/// - `bit_depth = 8` → byte at `(y * stride + x)`. Range `0..=255`.
///
/// Out-of-range coordinates clamp to the edge value, so callers can
/// safely sample at the atlas boundary without bounds-check branches.
// Allowed dead until the sw label SDF blit path consumes it.
#[allow(dead_code)]
#[inline]
pub(crate) fn read_quantized(atlas: &[u8], source_size: u16, bit_depth: u8, x: i32, y: i32) -> u8 {
    let s = source_size as i32;
    let xc = x.clamp(0, s - 1) as usize;
    let yc = y.clamp(0, s - 1) as usize;
    let row_idx = yc * s as usize + xc;
    if bit_depth == 4 {
        let byte = atlas[row_idx >> 1];
        if row_idx & 1 == 0 {
            byte & 0x0F
        } else {
            byte >> 4
        }
    } else {
        atlas[row_idx]
    }
}

/// Convert a quantized distance to signed source-pixel distance: positive
/// means inside the glyph, negative means outside, zero is the edge.
///
/// `spread` carries from [`AtlasHeader::spread`] (atlas-time choice of how
/// many pixels of edge band to encode).
#[inline]
pub fn quantized_to_signed_px(q: u8, bit_depth: u8, spread: u16) -> crate::types::Fixed {
    use crate::types::Fixed;
    // 4-bit: zero crossing at q = 7.5; 8-bit: zero at 127.5. Working in
    // doubled units (q * 2 - zero_x2) keeps the arithmetic in integers.
    let (zero_x2, max_q): (i32, i32) = if bit_depth == 4 { (15, 15) } else { (255, 255) };
    let signed_q_x2 = (q as i32) * 2 - zero_x2;
    Fixed::from_int(signed_q_x2) * Fixed::from_int(spread as i32) / Fixed::from_int(max_q)
}

/// Bilinear-sample the SDF atlas at fractional `(sx, sy)` in source-pixel
/// space and return the signed distance in source pixels (positive inside).
///
/// `sx` / `sy` are clamped before sampling; callers can feed any source-
/// space coordinate without preprocessing.
// Allowed dead until the sw label SDF blit path consumes it.
#[allow(dead_code)]
#[inline]
pub(crate) fn sample_signed_distance(
    atlas: &[u8],
    source_size: u16,
    bit_depth: u8,
    spread: u16,
    sx: crate::types::Fixed,
    sy: crate::types::Fixed,
) -> crate::types::Fixed {
    use crate::types::Fixed;
    let s = source_size as i32;
    let max = Fixed::from_int(s - 1);
    let zero = Fixed::ZERO;
    let sx = sx.max(zero).min(max);
    let sy = sy.max(zero).min(max);

    let x0 = sx.to_int();
    let y0 = sy.to_int();
    let x1 = (x0 + 1).min(s - 1);
    let y1 = (y0 + 1).min(s - 1);
    let fx = sx - Fixed::from_int(x0);
    let fy = sy - Fixed::from_int(y0);
    let one = Fixed::ONE;

    let q00 = read_quantized(atlas, source_size, bit_depth, x0, y0);
    let q10 = read_quantized(atlas, source_size, bit_depth, x1, y0);
    let q01 = read_quantized(atlas, source_size, bit_depth, x0, y1);
    let q11 = read_quantized(atlas, source_size, bit_depth, x1, y1);

    let d00 = quantized_to_signed_px(q00, bit_depth, spread);
    let d10 = quantized_to_signed_px(q10, bit_depth, spread);
    let d01 = quantized_to_signed_px(q01, bit_depth, spread);
    let d11 = quantized_to_signed_px(q11, bit_depth, spread);

    let top = d00 * (one - fx) + d10 * fx;
    let bot = d01 * (one - fx) + d11 * fx;
    top * (one - fy) + bot * fy
}

/// Convenience: wrap a parsed [`SdfFontProvider`] into a [`Font`] ready
/// to register in a `FontRegistry`.
pub fn font_from_mirx_chunk(
    family: &'static str,
    payload: &'static [u8],
) -> Result<Font, SdfFontError> {
    let provider = SdfFontProvider::from_mirx_chunk(payload)?;
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
    use crate::render::font::chunk::FontChunkKind;
    use alloc::vec::Vec;

    fn make_atlas(glyphs: &[(u32, u16, i8, i8)]) -> Vec<u8> {
        // 4-bit, 4×4 source = 8 bytes per glyph.
        let bit_depth: u8 = 4;
        let source_size: u16 = 4;
        let bytes_per_glyph =
            (source_size as u32 * source_size as u32 * bit_depth as u32).div_ceil(8);
        let glyph_count = glyphs.len() as u32;
        let metric_offset = HEADER_LEN as u32;
        let data_offset = metric_offset + glyph_count * METRIC_LEN as u32;
        let total = data_offset as usize + (glyph_count as usize) * bytes_per_glyph as usize;
        let mut out = alloc::vec![0u8; total];

        // Header.
        let header = AtlasHeader {
            version: SUPPORTED_VERSION,
            bit_depth,
            _pad0: 0,
            source_size,
            spread: 1,
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
            core::ptr::write_unaligned(out.as_mut_ptr() as *mut AtlasHeader, header);
        }

        // Metrics, codepoint sorted.
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
                core::ptr::write_unaligned(out.as_mut_ptr().add(off) as *mut GlyphMetric, m);
            }
        }

        // Glyph data: i-th glyph filled with byte = i+1 so we can identify it.
        for i in 0..glyph_count as usize {
            let start = data_offset as usize + i * bytes_per_glyph as usize;
            for b in &mut out[start..start + bytes_per_glyph as usize] {
                *b = (i + 1) as u8;
            }
        }
        // Prefix the body as a FONT chunk so the reader accepts it.
        let mut payload = alloc::vec![0u8; FONT_CHUNK_HEADER_LEN + out.len()];
        FontChunkHeader {
            kind: FontChunkKind::Sdf,
            format: bit_depth,
            size: source_size,
        }
        .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
        payload[FONT_CHUNK_HEADER_LEN..].copy_from_slice(&out);
        payload
    }

    fn leak(bytes: Vec<u8>) -> &'static [u8] {
        Vec::leak(bytes)
    }

    #[test]
    fn parse_minimal_atlas_with_two_glyphs() {
        let bytes = leak(make_atlas(&[
            (b'A' as u32, 5, 1, 2),
            (b'B' as u32, 6, -1, 0),
        ]));
        let provider = SdfFontProvider::from_mirx_chunk(bytes).expect("parse");
        assert_eq!(provider.header.glyph_count, 2);
        assert_eq!(provider.metrics.len(), 2);
        // Sorted: A < B.
        assert_eq!(provider.metrics[0].codepoint, b'A' as u32);
        assert_eq!(provider.metrics[1].codepoint, b'B' as u32);
    }

    #[test]
    fn glyph_returns_sdf_kind_with_atlas_slice() {
        let bytes = leak(make_atlas(&[(b'A' as u32, 5, 1, 2)]));
        let provider = SdfFontProvider::from_mirx_chunk(bytes).expect("parse");
        let g = provider.glyph('A').expect("A glyph");
        assert_eq!(g.advance, 5);
        match &g.kind {
            GlyphKind::Sdf {
                atlas,
                source_size,
                bit_depth,
                ..
            } => {
                assert_eq!(*source_size, 4);
                assert_eq!(*bit_depth, 4);
                assert_eq!(atlas.len(), 8);
                assert!(atlas.iter().all(|&b| b == 1));
            }
            other => panic!("expected Sdf, got {:?}", other),
        }
    }

    #[test]
    fn missing_codepoint_returns_none() {
        let bytes = leak(make_atlas(&[(b'A' as u32, 5, 0, 0)]));
        let provider = SdfFontProvider::from_mirx_chunk(bytes).expect("parse");
        assert!(provider.glyph('Z').is_none());
    }

    #[test]
    fn rejects_payload_smaller_than_header() {
        // Valid Sdf prefix, but the body is shorter than AtlasHeader.
        let mut short = alloc::vec![0u8; FONT_CHUNK_HEADER_LEN + 8];
        FontChunkHeader {
            kind: FontChunkKind::Sdf,
            format: 4,
            size: 4,
        }
        .write(&mut short[..FONT_CHUNK_HEADER_LEN]);
        assert!(matches!(
            SdfFontProvider::from_mirx_chunk(leak(short)),
            Err(SdfFontError::PayloadTooShort)
        ));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut bytes = make_atlas(&[(b'A' as u32, 5, 0, 0)]);
        // Version is the body's first u16, after the 4-byte prefix.
        bytes[FONT_CHUNK_HEADER_LEN] = 99;
        bytes[FONT_CHUNK_HEADER_LEN + 1] = 0;
        let leaked = leak(bytes);
        assert!(matches!(
            SdfFontProvider::from_mirx_chunk(leaked),
            Err(SdfFontError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn rejects_bad_bit_depth() {
        let mut bytes = make_atlas(&[(b'A' as u32, 5, 0, 0)]);
        // bit_depth is body offset 2, i.e. after the 4-byte prefix.
        bytes[FONT_CHUNK_HEADER_LEN + 2] = 5;
        let leaked = leak(bytes);
        assert!(matches!(
            SdfFontProvider::from_mirx_chunk(leaked),
            Err(SdfFontError::InvalidBitDepth(5))
        ));
    }

    #[test]
    fn read_quantized_unpacks_4bit_low_then_high_nibble() {
        // pixel 0 in low nibble, pixel 1 in high nibble of byte 0
        let atlas = [0xAB_u8, 0xCD]; // pixels: 0xB, 0xA, 0xD, 0xC
        assert_eq!(read_quantized(&atlas, 4, 4, 0, 0), 0xB);
        assert_eq!(read_quantized(&atlas, 4, 4, 1, 0), 0xA);
        assert_eq!(read_quantized(&atlas, 4, 4, 2, 0), 0xD);
        assert_eq!(read_quantized(&atlas, 4, 4, 3, 0), 0xC);
    }

    #[test]
    fn read_quantized_unpacks_8bit_one_byte_per_pixel() {
        let atlas = [0x10, 0x20, 0x30, 0x40];
        assert_eq!(read_quantized(&atlas, 2, 8, 0, 0), 0x10);
        assert_eq!(read_quantized(&atlas, 2, 8, 1, 0), 0x20);
        assert_eq!(read_quantized(&atlas, 2, 8, 0, 1), 0x30);
        assert_eq!(read_quantized(&atlas, 2, 8, 1, 1), 0x40);
    }

    #[test]
    fn read_quantized_clamps_out_of_range() {
        let atlas = [0xAB_u8, 0xCD];
        // Off the right edge clamps to x = 3 → high nibble of byte 1 = 0xC.
        assert_eq!(read_quantized(&atlas, 4, 4, 99, 0), 0xC);
        // Negative clamps to (0, 0) → low nibble of byte 0 = 0xB.
        assert_eq!(read_quantized(&atlas, 4, 4, -5, -5), 0xB);
    }

    #[test]
    fn quantized_to_signed_px_zero_at_midpoint() {
        use crate::types::Fixed;
        // 4-bit: q = 7 → signed_q_x2 = -1, q = 8 → +1. Symmetric around 7.5.
        let d7 = quantized_to_signed_px(7, 4, 8);
        let d8 = quantized_to_signed_px(8, 4, 8);
        assert!(d7 < Fixed::ZERO);
        assert!(d8 > Fixed::ZERO);
        assert_eq!(d7 + d8, Fixed::ZERO);
        // Extremes map to ±spread.
        assert_eq!(quantized_to_signed_px(0, 4, 8), -Fixed::from_int(8));
        assert_eq!(quantized_to_signed_px(15, 4, 8), Fixed::from_int(8));
    }

    #[test]
    fn quantized_to_signed_px_8bit_zero_at_127_5() {
        use crate::types::Fixed;
        let d127 = quantized_to_signed_px(127, 8, 8);
        let d128 = quantized_to_signed_px(128, 8, 8);
        assert!(d127 < Fixed::ZERO);
        assert!(d128 > Fixed::ZERO);
        assert_eq!(d127 + d128, Fixed::ZERO);
    }

    #[test]
    fn sample_at_integer_grid_matches_unpack() {
        use crate::types::Fixed;
        // 4×4 atlas, 4-bit; fill quadrant pattern: top-left = 0xF (deep
        // inside), bottom-right = 0x0 (deep outside).
        let mut atlas = alloc::vec![0u8; 8];
        // pixels (x, y) → q. Want: (0,0)=15, (3,3)=0, others mid.
        let set = |buf: &mut [u8], x: usize, y: usize, q: u8| {
            let idx = y * 4 + x;
            let byte_idx = idx >> 1;
            if idx & 1 == 0 {
                buf[byte_idx] = (buf[byte_idx] & 0xF0) | (q & 0x0F);
            } else {
                buf[byte_idx] = (buf[byte_idx] & 0x0F) | ((q & 0x0F) << 4);
            }
        };
        set(&mut atlas, 0, 0, 15);
        set(&mut atlas, 3, 3, 0);

        // Sample at exact integer grid: should equal quantized-to-signed of
        // that pixel.
        let zero = Fixed::ZERO;
        let center = sample_signed_distance(&atlas, 4, 4, 8, zero, zero);
        assert_eq!(center, quantized_to_signed_px(15, 4, 8));

        let far = sample_signed_distance(&atlas, 4, 4, 8, Fixed::from_int(3), Fixed::from_int(3));
        assert_eq!(far, quantized_to_signed_px(0, 4, 8));
    }

    #[test]
    fn sample_clamps_out_of_range_coords() {
        use crate::types::Fixed;
        let atlas = alloc::vec![0xFF_u8; 8]; // all q = 0xF
        // Way out of the atlas — should still return a valid sample by
        // clamping to the edge.
        let d = sample_signed_distance(&atlas, 4, 4, 8, Fixed::from_int(99), Fixed::from_int(-99));
        assert_eq!(d, quantized_to_signed_px(15, 4, 8));
    }
}
