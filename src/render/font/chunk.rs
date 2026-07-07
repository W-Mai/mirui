//! Format-agnostic FONT chunk header + representation selection.

pub use mirx::{FONT_CHUNK_HEADER_LEN, FontChunkHeader, FontChunkKind};

use super::FontFormat;

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
    fn auto_prefers_closest_grayscale() {
        let hs = vec![
            h(FontChunkKind::Sdf, 32),
            h(FontChunkKind::Grayscale, 12),
            h(FontChunkKind::Grayscale, 16),
        ];
        assert_eq!(select_font_chunk(&hs, 15, FontFormat::Auto), Some(2));
        assert_eq!(select_font_chunk(&hs, 13, FontFormat::Auto), Some(1));
    }

    #[test]
    fn auto_switches_to_sdf_beyond_largest_gray() {
        let hs = vec![
            h(FontChunkKind::Grayscale, 12),
            h(FontChunkKind::Grayscale, 16),
            h(FontChunkKind::Sdf, 24),
        ];
        assert_eq!(select_font_chunk(&hs, 16, FontFormat::Auto), Some(1));
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
