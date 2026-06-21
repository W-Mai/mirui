//! Multi-representation font: one mirx bundle holding several FONT
//! chunks (e.g. a 12px pixel table and a 24px SDF), with the right one
//! picked per requested size.
//!
//! Every chunk in a bundle carries a [`FontChunkHeader`] prefix — SDF
//! included — so the reader can tell kinds apart by peeking the prefix
//! (a standalone SDF file stays prefix-less; only bundles add it). At
//! lookup time [`select_font_chunk`] chooses by size: a pixel table
//! while the request fits one, the SDF beyond that.

use alloc::rc::Rc;
use alloc::vec::Vec;

use super::chunk::{FontChunkHeader, FontChunkKind, select_font_chunk};
use super::gray::GrayFontProvider;
use super::sdf::SdfFontProvider;
use super::{Font, FontBackend, FontFormat, FontMetrics, FontProvider, Glyph};

/// A parsed sub-table inside a bundle. Stored as an enum, not a boxed
/// `dyn FontProvider`, to avoid an allocation per representation.
#[derive(Clone, Debug)]
enum Repr {
    Sdf(SdfFontProvider),
    Gray(GrayFontProvider),
}

impl Repr {
    fn glyph(&self, ch: char, size: u16) -> Option<Glyph> {
        match self {
            Repr::Sdf(p) => p.glyph(ch, size),
            Repr::Gray(p) => p.glyph(ch, size),
        }
    }

    fn metrics(&self) -> FontMetrics {
        // Qualify the trait method: SdfFontProvider also has an inherent
        // metrics() returning the glyph table, which would shadow this.
        match self {
            Repr::Sdf(p) => FontProvider::metrics(p),
            Repr::Gray(p) => FontProvider::metrics(p),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiFontError {
    /// The bundle holds no FONT chunks, or none parsed.
    NoFontChunks,
    /// A chunk's prefix was missing or malformed.
    BadChunkHeader,
}

/// Font provider over a multi-chunk mirx bundle.
#[derive(Clone, Debug)]
pub struct MultiFontProvider {
    headers: Vec<FontChunkHeader>,
    reprs: Vec<Repr>,
}

impl MultiFontProvider {
    /// Parse every prefixed FONT chunk in `bundle` into a sub-table.
    /// Both gray and SDF chunks carry a `FontChunkHeader` prefix, so
    /// each provider peels its own and the bundle just dispatches by
    /// the peeked kind.
    pub fn from_mirx(bundle: &'static [u8]) -> Result<Self, MultiFontError> {
        let parsed = mirx::parse_chunk(bundle).map_err(|_| MultiFontError::NoFontChunks)?;
        let mut headers = Vec::new();
        let mut reprs = Vec::new();
        for payload in parsed.chunk_payloads(bundle, mirx::chunk_type::FONT) {
            let header = FontChunkHeader::parse(payload).ok_or(MultiFontError::BadChunkHeader)?;
            match header.kind {
                FontChunkKind::Grayscale => {
                    if let Ok(p) = GrayFontProvider::from_mirx_chunk(payload) {
                        headers.push(header);
                        reprs.push(Repr::Gray(p));
                    }
                }
                FontChunkKind::Sdf => {
                    if let Ok(p) = SdfFontProvider::from_mirx_chunk(payload) {
                        headers.push(header);
                        reprs.push(Repr::Sdf(p));
                    }
                }
            }
        }
        if reprs.is_empty() {
            return Err(MultiFontError::NoFontChunks);
        }
        Ok(Self { headers, reprs })
    }

    /// The size of the largest grayscale table, or the first SDF source
    /// size — a reasonable default the `Font` wrapper reports as `size`.
    pub fn default_size(&self) -> u16 {
        self.headers
            .iter()
            .filter(|h| h.kind == FontChunkKind::Grayscale)
            .map(|h| h.size)
            .max()
            .or_else(|| self.headers.first().map(|h| h.size))
            .unwrap_or(16)
    }

    pub fn repr_count(&self) -> usize {
        self.reprs.len()
    }
}

impl FontProvider for MultiFontProvider {
    fn glyph(&self, ch: char, requested_size: u16) -> Option<Glyph> {
        let idx = select_font_chunk(&self.headers, requested_size, FontFormat::Auto)?;
        self.reprs[idx].glyph(ch, requested_size)
    }

    fn metrics(&self) -> FontMetrics {
        // Trait metrics carry no size, so report the representation a
        // mid-range request would land on; layout sizing stays close
        // enough and per-glyph rendering still uses the real size.
        let idx = select_font_chunk(&self.headers, self.default_size(), FontFormat::Auto);
        idx.map(|i| self.reprs[i].metrics()).unwrap_or(FontMetrics {
            ascender: 0,
            descender: 0,
            line_height: 0,
        })
    }
}

/// Wrap a bundle into a [`Font`]. `size` defaults to the largest pixel
/// table; callers set a different `Font::size` to request another size,
/// which selects the matching chunk at render time.
pub fn font_from_mirx_bundle(
    family: &'static str,
    bundle: &'static [u8],
) -> Result<Font, MultiFontError> {
    let provider = MultiFontProvider::from_mirx(bundle)?;
    let size = provider.default_size();
    Ok(Font {
        family,
        size,
        backend: FontBackend::Custom(Rc::new(provider)),
    })
}

#[cfg(test)]
mod tests {
    use super::super::GlyphKind;
    use super::super::chunk::FONT_CHUNK_HEADER_LEN;
    use super::super::sdf::{AtlasHeader, GlyphMetric, HEADER_LEN, METRIC_LEN, SUPPORTED_VERSION};
    use super::*;

    // Build the shared atlas body (header + one 'A' metric + zeroed
    // glyph data) for a given geometry, then prefix it as a FONT chunk.
    fn prefixed_chunk(kind: FontChunkKind, source: u16, bit_depth: u8, spread: u16) -> Vec<u8> {
        let bpg = (source as u32 * source as u32 * bit_depth as u32).div_ceil(8);
        let metric_offset = HEADER_LEN as u32;
        let data_offset = metric_offset + METRIC_LEN as u32;
        let mut body = alloc::vec![0u8; data_offset as usize + bpg as usize];
        let header = AtlasHeader {
            version: SUPPORTED_VERSION,
            bit_depth,
            _pad0: 0,
            source_size: source,
            spread,
            glyph_count: 1,
            metric_offset,
            data_offset,
            bytes_per_glyph: bpg,
            ascender: 3,
            descender: 1,
            line_height: 4,
            _pad1: 0,
        };
        unsafe {
            core::ptr::write_unaligned(body.as_mut_ptr() as *mut AtlasHeader, header);
            core::ptr::write_unaligned(
                body.as_mut_ptr().add(metric_offset as usize) as *mut GlyphMetric,
                GlyphMetric {
                    codepoint: b'A' as u32,
                    advance: source,
                    bearing_x: 0,
                    bearing_y: 0,
                },
            );
        }
        let mut payload = alloc::vec![0u8; FONT_CHUNK_HEADER_LEN + body.len()];
        FontChunkHeader {
            kind,
            format: bit_depth,
            size: source,
        }
        .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
        payload[FONT_CHUNK_HEADER_LEN..].copy_from_slice(&body);
        payload
    }

    fn leak(v: Vec<u8>) -> &'static [u8] {
        Vec::leak(v)
    }

    // gray@12 + sdf@24 bundle.
    fn two_table_bundle() -> &'static [u8] {
        let gray = prefixed_chunk(FontChunkKind::Grayscale, 12, 4, 0);
        let sdf = prefixed_chunk(FontChunkKind::Sdf, 24, 4, 4);
        let bytes = mirx::encode_chunks(&[
            (
                mirx::chunk_type::FONT,
                mirx::ChunkEntry::FLAG_CRITICAL,
                &gray,
            ),
            (
                mirx::chunk_type::FONT,
                mirx::ChunkEntry::FLAG_CRITICAL,
                &sdf,
            ),
        ]);
        leak(bytes)
    }

    #[test]
    fn parses_both_tables_from_bundle() {
        let p = MultiFontProvider::from_mirx(two_table_bundle()).expect("parse");
        assert_eq!(p.reprs.len(), 2);
        assert_eq!(p.default_size(), 12);
    }

    #[test]
    fn routes_small_to_gray_large_to_sdf() {
        let p = MultiFontProvider::from_mirx(two_table_bundle()).expect("parse");
        // ≤ gray's 12 → grayscale table.
        match p.glyph('A', 12).expect("small").kind {
            GlyphKind::Grayscale { .. } => {}
            other => panic!("expected Grayscale at 12, got {other:?}"),
        }
        // Far past it → the SDF table.
        match p.glyph('A', 96).expect("large").kind {
            GlyphKind::Sdf { .. } => {}
            other => panic!("expected Sdf at 96, got {other:?}"),
        }
    }

    #[test]
    fn missing_glyph_is_none() {
        let p = MultiFontProvider::from_mirx(two_table_bundle()).expect("parse");
        assert!(p.glyph('Z', 12).is_none());
    }

    #[test]
    fn empty_bundle_errors() {
        let bytes = leak(mirx::encode_chunks(&[]));
        assert!(matches!(
            MultiFontProvider::from_mirx(bytes),
            Err(MultiFontError::NoFontChunks)
        ));
    }

    #[test]
    fn font_wrapper_reports_default_size() {
        let font = font_from_mirx_bundle("multi", two_table_bundle()).expect("font");
        assert_eq!(font.size, 12);
    }
}
