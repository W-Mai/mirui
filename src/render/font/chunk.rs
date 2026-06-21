//! Format-agnostic FONT chunk header + representation selection.
//!
//! One `.mirx` font file can hold several FONT chunks — a 16px
//! grayscale bitmap, a 32px SDF, etc. Each FONT payload begins with a
//! [`FontChunkHeader`] so a reader can pick the right representation
//! for a requested size + format without parsing the (format-specific)
//! body. The bytes after the header are the SDF [`AtlasHeader`] or a
//! grayscale header, decided by `kind`.

use super::FontFormat;

/// Length of the shared prefix every FONT payload starts with.
pub const FONT_CHUNK_HEADER_LEN: usize = 4;

/// Rasterization scheme stored in a FONT chunk, parallel to
/// [`super::GlyphKind`] / [`FontFormat`]. Serialized as the first byte
/// of a FONT payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontChunkKind {
    Grayscale,
    Sdf,
}

impl FontChunkKind {
    fn from_u8(b: u8) -> Option<Self> {
        match b {
            0 => Some(FontChunkKind::Grayscale),
            1 => Some(FontChunkKind::Sdf),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            FontChunkKind::Grayscale => 0,
            FontChunkKind::Sdf => 1,
        }
    }
}

/// Shared 4-byte prefix on every FONT chunk payload.
///
/// `size` is the fixed pixel height for grayscale tables; for SDF
/// (which scales one atlas to any target) it carries the source size
/// and selection treats SDF as covering all sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontChunkHeader {
    pub kind: FontChunkKind,
    /// bpp for grayscale, bit_depth for SDF.
    pub format: u8,
    pub size: u16,
}

impl FontChunkHeader {
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < FONT_CHUNK_HEADER_LEN {
            return None;
        }
        Some(FontChunkHeader {
            kind: FontChunkKind::from_u8(payload[0])?,
            format: payload[1],
            size: u16::from_le_bytes([payload[2], payload[3]]),
        })
    }

    pub fn write(&self, out: &mut [u8]) {
        out[0] = self.kind.to_u8();
        out[1] = self.format;
        out[2..4].copy_from_slice(&self.size.to_le_bytes());
    }
}

/// Pick the best FONT chunk for `requested_size` + `want`.
///
/// Selection rules:
/// - `FontFormat::Sdf` → first SDF chunk (one atlas serves every size).
/// - `FontFormat::Grayscale` → grayscale chunk whose `size` is closest
///   to `requested_size`.
/// - `FontFormat::Auto` → a grayscale table when the request fits one
///   (`requested_size` ≤ the largest gray design size), since a fixed
///   pixel table is crisp there; beyond that it switches to SDF, which
///   actually resamples. A grayscale glyph renders at its baked cell
///   size and ignores the request, so picking gray for a far-larger
///   size would render the wrong size, not just a soft one — hence the
///   cutoff. Falls back to whichever kind exists if the preferred one
///   is absent.
///
/// `headers` is the parsed header of each candidate, in chunk order;
/// returns the index of the chosen candidate, or `None` if empty.
pub fn select_font_chunk(
    headers: &[FontChunkHeader],
    requested_size: u16,
    want: FontFormat,
) -> Option<usize> {
    let first_sdf = || headers.iter().position(|h| h.kind == FontChunkKind::Sdf);
    let closest_gray = || {
        headers
            .iter()
            .enumerate()
            .filter(|(_, h)| h.kind == FontChunkKind::Grayscale)
            .min_by_key(|(_, h)| (h.size as i32 - requested_size as i32).unsigned_abs())
            .map(|(i, _)| i)
    };
    let max_gray_size = || {
        headers
            .iter()
            .filter(|h| h.kind == FontChunkKind::Grayscale)
            .map(|h| h.size)
            .max()
    };
    match want {
        FontFormat::Sdf => first_sdf(),
        FontFormat::Grayscale => closest_gray(),
        FontFormat::Auto => match max_gray_size() {
            Some(max) if requested_size <= max => closest_gray(),
            _ => first_sdf().or_else(closest_gray),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn h(kind: FontChunkKind, size: u16) -> FontChunkHeader {
        FontChunkHeader {
            kind,
            format: 4,
            size,
        }
    }

    #[test]
    fn header_round_trips() {
        let orig = FontChunkHeader {
            kind: FontChunkKind::Grayscale,
            format: 4,
            size: 16,
        };
        let mut buf = [0u8; FONT_CHUNK_HEADER_LEN];
        orig.write(&mut buf);
        assert_eq!(FontChunkHeader::parse(&buf), Some(orig));
    }

    #[test]
    fn parse_rejects_short_payload() {
        assert_eq!(FontChunkHeader::parse(&[0, 0, 0]), None);
    }

    #[test]
    fn parse_rejects_unknown_kind() {
        assert_eq!(FontChunkHeader::parse(&[9, 4, 16, 0]), None);
    }

    #[test]
    fn auto_prefers_closest_grayscale() {
        let hs = vec![
            h(FontChunkKind::Sdf, 32),
            h(FontChunkKind::Grayscale, 12),
            h(FontChunkKind::Grayscale, 16),
        ];
        // requested 15 → grayscale 16 is closest.
        assert_eq!(select_font_chunk(&hs, 15, FontFormat::Auto), Some(2));
        // requested 13 → grayscale 12 is closest.
        assert_eq!(select_font_chunk(&hs, 13, FontFormat::Auto), Some(1));
    }

    #[test]
    fn auto_switches_to_sdf_beyond_largest_gray() {
        let hs = vec![
            h(FontChunkKind::Grayscale, 12),
            h(FontChunkKind::Grayscale, 16),
            h(FontChunkKind::Sdf, 24),
        ];
        // ≤ largest gray (16): a gray table.
        assert_eq!(select_font_chunk(&hs, 16, FontFormat::Auto), Some(1));
        // Past it (100px) is beyond any pixel table's design size, so
        // the result is the resizable SDF.
        assert_eq!(select_font_chunk(&hs, 100, FontFormat::Auto), Some(2));
    }

    #[test]
    fn auto_falls_back_to_sdf_without_grayscale() {
        let hs = vec![h(FontChunkKind::Sdf, 32)];
        assert_eq!(select_font_chunk(&hs, 16, FontFormat::Auto), Some(0));
    }

    #[test]
    fn explicit_sdf_skips_grayscale() {
        let hs = vec![h(FontChunkKind::Grayscale, 16), h(FontChunkKind::Sdf, 32)];
        assert_eq!(select_font_chunk(&hs, 16, FontFormat::Sdf), Some(1));
    }

    #[test]
    fn explicit_grayscale_without_any_is_none() {
        let hs = vec![h(FontChunkKind::Sdf, 32)];
        assert_eq!(select_font_chunk(&hs, 16, FontFormat::Grayscale), None);
    }

    #[test]
    fn empty_is_none() {
        assert_eq!(select_font_chunk(&[], 16, FontFormat::Auto), None);
    }
}
