//! `gen-mirx font` — TTF/OTF → SDF atlas → `chunk_type::FONT` mirx.
//!
//! Pipeline:
//!
//! 1. Load TTF via `ttf-parser`, walk the outline of every charset
//!    glyph into a `mirui::render::path::Path` scaled into the
//!    target `source_size × source_size` cell.
//! 2. Rasterize the path into a `source_size × source_size` coverage
//!    buffer with `scanline_fill(..., FillRule::NonZero, ...)` (CFF /
//!    OpenType-safe).
//! 3. Run a brute-force Euclidean distance transform on the coverage
//!    buffer to produce a signed distance field. The result is
//!    quantized to 4 or 8 bits matching the runtime sampler.
//! 4. Pack `AtlasHeader` + sorted `GlyphMetric` table + glyph
//!    distance bytes into a payload, wrap in `encode_chunk_generic`
//!    with `chunk_type::FONT`, write the file.
//!
//! The DT step is the slow part (`O(N⁴)` per glyph). It runs at
//! atlas-build time on a dev box, not at runtime, so the simple
//! correctness-first implementation stays.

use std::fs;
use std::path::PathBuf;

use mirui::render::font::chunk::{FONT_CHUNK_HEADER_LEN, FontChunkHeader, FontChunkKind};
use mirui::render::font::sdf::{
    AtlasHeader, GlyphMetric, HEADER_LEN, METRIC_LEN, SUPPORTED_VERSION,
};
use mirui::render::path::Path as MirPath;
use mirui::render::raster::{FillRule, flatten, scanline_fill};
use mirui::types::{Fixed, Point};
use mirx::{chunk_type, encode_chunk_generic};
use ttf_parser::{Face, OutlineBuilder};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
struct Args {
    ttf: PathBuf,
    charset: String,
    size: u16,
    bit_depth: u8,
    spread: u16,
    out: PathBuf,
}

pub fn run(args: &[String]) -> Result {
    let parsed = parse_args(args)?;
    let ttf_bytes = fs::read(&parsed.ttf)?;
    let face = Face::parse(&ttf_bytes, 0)?;

    let mut chars: Vec<char> = parsed.charset.chars().collect();
    chars.sort();
    chars.dedup();
    if chars.is_empty() {
        return Err("charset is empty".into());
    }

    let units_per_em = face.units_per_em() as f32;
    let scale = parsed.size as f32 / units_per_em;
    let ascender = (face.ascender() as f32 * scale).round() as i32;
    let descender = (face.descender() as f32 * scale).round() as i32;
    let line_height = (face.height() as f32 * scale).round() as i32;

    let bytes_per_glyph = bytes_per_glyph(parsed.size, parsed.bit_depth);

    let mut metrics: Vec<GlyphMetric> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    let mut skipped: Vec<char> = Vec::new();

    for ch in &chars {
        let Some(gid) = face.glyph_index(*ch) else {
            skipped.push(*ch);
            continue;
        };
        let mut builder = PathBuilder::new(scale, parsed.size as f32);
        let bbox = match face.outline_glyph(gid, &mut builder) {
            Some(b) => b,
            None => {
                // Empty glyph (e.g. space). Still record advance.
                metrics.push(GlyphMetric {
                    codepoint: *ch as u32,
                    advance: face
                        .glyph_hor_advance(gid)
                        .map(|a| (a as f32 * scale).round() as u16)
                        .unwrap_or(parsed.size / 2),
                    bearing_x: 0,
                    bearing_y: 0,
                });
                data.extend(std::iter::repeat_n(0u8, bytes_per_glyph));
                continue;
            }
        };

        let path = builder.finish();
        let coverage = rasterize_to_coverage(&path, parsed.size);
        let signed = euclidean_distance_transform(&coverage, parsed.size, parsed.spread);
        let packed = quantize(&signed, parsed.bit_depth, parsed.spread as f32);
        debug_assert_eq!(packed.len(), bytes_per_glyph);
        data.extend(packed);

        let advance = face
            .glyph_hor_advance(gid)
            .map(|a| (a as f32 * scale).round() as u16)
            .unwrap_or(parsed.size);
        let bearing_x = (bbox.x_min as f32 * scale).round() as i32;
        let bearing_y = (bbox.y_max as f32 * scale).round() as i32;
        metrics.push(GlyphMetric {
            codepoint: *ch as u32,
            advance,
            bearing_x: bearing_x.clamp(-128, 127) as i8,
            bearing_y: bearing_y.clamp(-128, 127) as i8,
        });
    }

    metrics.sort_by_key(|m| m.codepoint);

    let body = pack_payload(
        &metrics,
        &data,
        parsed.size,
        parsed.bit_depth,
        parsed.spread,
        ascender,
        descender,
        line_height,
        bytes_per_glyph,
    );
    let mut payload = vec![0u8; FONT_CHUNK_HEADER_LEN + body.len()];
    FontChunkHeader {
        kind: FontChunkKind::Sdf,
        format: parsed.bit_depth,
        size: parsed.size,
    }
    .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
    payload[FONT_CHUNK_HEADER_LEN..].copy_from_slice(&body);
    let mirx_bytes =
        encode_chunk_generic(chunk_type::FONT, mirx::ChunkEntry::FLAG_CRITICAL, &payload);
    fs::write(&parsed.out, &mirx_bytes)?;

    println!(
        "wrote {} bytes to {} ({} glyphs, source_size={}, bit_depth={}, spread={})",
        mirx_bytes.len(),
        parsed.out.display(),
        metrics.len(),
        parsed.size,
        parsed.bit_depth,
        parsed.spread,
    );
    if !skipped.is_empty() {
        println!("skipped {} chars not in font: {:?}", skipped.len(), skipped);
    }
    Ok(())
}

fn parse_args(args: &[String]) -> Result<Args> {
    let mut ttf: Option<PathBuf> = None;
    let mut charset: Option<String> = None;
    let mut charset_file: Option<PathBuf> = None;
    let mut size: Option<u16> = None;
    let mut bit_depth: u8 = 4;
    let mut spread: Option<u16> = None;
    let mut out: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        let v = || -> Result<&str> {
            args.get(i + 1)
                .map(|s| s.as_str())
                .ok_or_else(|| format!("flag {} needs a value", args[i]).into())
        };
        match args[i].as_str() {
            "--ttf" => {
                ttf = Some(PathBuf::from(v()?));
                i += 2;
            }
            "--charset" => {
                charset = Some(v()?.to_string());
                i += 2;
            }
            "--charset-file" => {
                charset_file = Some(PathBuf::from(v()?));
                i += 2;
            }
            "--size" => {
                size = Some(v()?.parse()?);
                i += 2;
            }
            "--bit-depth" => {
                bit_depth = v()?.parse()?;
                i += 2;
            }
            "--spread" => {
                spread = Some(v()?.parse()?);
                i += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(v()?));
                i += 2;
            }
            other => return Err(format!("unknown flag {other}").into()),
        }
    }

    let charset = match (charset, charset_file) {
        (Some(s), None) => s,
        (None, Some(path)) => fs::read_to_string(&path)?,
        (Some(_), Some(_)) => return Err("--charset and --charset-file are exclusive".into()),
        (None, None) => return Err("need --charset or --charset-file".into()),
    };

    let size = size.ok_or("missing --size")?;
    if !(bit_depth == 4 || bit_depth == 8) {
        return Err(format!("--bit-depth must be 4 or 8, got {bit_depth}").into());
    }
    let spread = spread.unwrap_or((size / 4).max(1));
    Ok(Args {
        ttf: ttf.ok_or("missing --ttf")?,
        charset,
        size,
        bit_depth,
        spread,
        out: out.ok_or("missing --out")?,
    })
}

fn bytes_per_glyph(size: u16, bit_depth: u8) -> usize {
    let pixels = size as usize * size as usize;
    (pixels * bit_depth as usize).div_ceil(8)
}

/// `OutlineBuilder` adapter that maps font units (`y` up, origin at
/// the glyph's baseline) into atlas-source pixel space (`y` down,
/// origin at the cell top-left).
struct PathBuilder {
    path: MirPath,
    scale: f32,
    cell_size: f32,
}

impl PathBuilder {
    fn new(scale: f32, cell_size: f32) -> Self {
        Self {
            path: MirPath::new(),
            scale,
            cell_size,
        }
    }

    fn finish(self) -> MirPath {
        self.path
    }

    fn map(&self, x: f32, y: f32) -> Point {
        // ttf-parser y points up from the baseline; we want y down
        // from the cell top. Drop the glyph onto the baseline at
        // 80% down the cell so descenders don't get clipped.
        let baseline = self.cell_size * 0.8;
        Point {
            x: Fixed::from_f32(x * self.scale),
            y: Fixed::from_f32(baseline - y * self.scale),
        }
    }
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.map(x, y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.map(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(self.map(x1, y1), self.map(x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path
            .cubic_to(self.map(x1, y1), self.map(x2, y2), self.map(x, y));
    }
    fn close(&mut self) {
        self.path.close();
    }
}

/// Rasterize the outline into a `size × size` coverage grid using the
/// non-zero winding rule (TrueType / CFF correct). Returns one byte
/// per pixel where `0xFF` is fully inside and `0` fully outside.
fn rasterize_to_coverage(path: &MirPath, size: u16) -> Vec<u8> {
    let segs = flatten(path);
    let n = size as i32;
    let mut buf = vec![0u8; (n * n) as usize];
    scanline_fill(&segs, 0, 0, n, n, FillRule::NonZero, |x, y, cov| {
        if (0..n).contains(&x) && (0..n).contains(&y) {
            // Coverage in [0, 1]; map to [0, 255].
            let v = (cov * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
            buf[(y * n + x) as usize] = v;
        }
    });
    buf
}

/// Brute-force Euclidean distance transform: for each pixel find the
/// nearest pixel of the opposite class (inside vs outside), measure
/// the Euclidean distance, and clamp to ±`spread`. Pixels with
/// coverage ≥ 128 count as inside.
fn euclidean_distance_transform(cov: &[u8], size: u16, spread: u16) -> Vec<f32> {
    let n = size as i32;
    let mut out = vec![0f32; cov.len()];
    let cap = spread as f32;
    let cap2 = cap * cap;

    for y in 0..n {
        for x in 0..n {
            let inside = cov[(y * n + x) as usize] >= 128;
            let mut best2 = cap2 + 1.0;

            // Bounded scan: only look within ±spread pixels.
            let lo_x = (x - cap as i32).max(0);
            let hi_x = (x + cap as i32 + 1).min(n);
            let lo_y = (y - cap as i32).max(0);
            let hi_y = (y + cap as i32 + 1).min(n);
            for sy in lo_y..hi_y {
                for sx in lo_x..hi_x {
                    let other_inside = cov[(sy * n + sx) as usize] >= 128;
                    if other_inside == inside {
                        continue;
                    }
                    let dx = (sx - x) as f32;
                    let dy = (sy - y) as f32;
                    let d2 = dx * dx + dy * dy;
                    if d2 < best2 {
                        best2 = d2;
                    }
                }
            }

            let d = best2.sqrt().min(cap);
            // Inside → positive, outside → negative.
            out[(y * n + x) as usize] = if inside { d } else { -d };
        }
    }

    out
}

/// Quantize signed distances to `bit_depth` (4 or 8), the exact
/// inverse of the runtime decoder
/// `render::font::sdf::quantized_to_signed_px`:
///
///   q = round(signed / spread * (max_q / 2) + max_q / 2)
///
/// Quantizing against `spread` (not a per-glyph max) is what makes
/// encode and decode agree: `−spread → 0`, `0 → max_q/2`,
/// `+spread → max_q`. A per-glyph renormalization would inflate
/// decoded distances by `spread / max_abs`, which shows up as
/// uneven coverage inside a glyph (thin strokes fade while corners
/// stay solid).
fn quantize(signed: &[f32], bit_depth: u8, spread: f32) -> Vec<u8> {
    let max_q = if bit_depth == 4 { 15.0 } else { 255.0 };
    let zero = max_q / 2.0;
    let scale = zero / spread;
    let bytes_n = if bit_depth == 4 {
        signed.len().div_ceil(2)
    } else {
        signed.len()
    };
    let mut out = vec![0u8; bytes_n];

    for (i, &d) in signed.iter().enumerate() {
        let q = (d.clamp(-spread, spread) * scale + zero)
            .round()
            .clamp(0.0, max_q) as u8;
        if bit_depth == 4 {
            let byte_idx = i >> 1;
            if i & 1 == 0 {
                out[byte_idx] = (out[byte_idx] & 0xF0) | (q & 0x0F);
            } else {
                out[byte_idx] = (out[byte_idx] & 0x0F) | ((q & 0x0F) << 4);
            }
        } else {
            out[i] = q;
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn pack_payload(
    metrics: &[GlyphMetric],
    data: &[u8],
    source_size: u16,
    bit_depth: u8,
    spread: u16,
    ascender: i32,
    descender: i32,
    line_height: i32,
    bytes_per_glyph: usize,
) -> Vec<u8> {
    let glyph_count = metrics.len();
    let metric_offset = HEADER_LEN as u32;
    let data_offset = metric_offset + (glyph_count * METRIC_LEN) as u32;
    let total = data_offset as usize + data.len();
    let mut out = vec![0u8; total];

    let header = AtlasHeader {
        version: SUPPORTED_VERSION,
        bit_depth,
        _pad0: 0,
        source_size,
        spread,
        glyph_count: glyph_count as u32,
        metric_offset,
        data_offset,
        bytes_per_glyph: bytes_per_glyph as u32,
        ascender: ascender.max(0).min(u16::MAX as i32) as u16,
        descender: descender.unsigned_abs().min(u16::MAX as u32) as u16,
        line_height: line_height.max(0).min(u16::MAX as i32) as u16,
        _pad1: 0,
    };
    // Soundness: AtlasHeader is `#[repr(C)]` POD; `out` is freshly
    // allocated and large enough; `write_unaligned` is the safe
    // bridge between owned struct and untyped bytes.
    unsafe {
        std::ptr::write_unaligned(out.as_mut_ptr() as *mut AtlasHeader, header);
    }
    for (i, m) in metrics.iter().enumerate() {
        let off = metric_offset as usize + i * METRIC_LEN;
        unsafe {
            std::ptr::write_unaligned(out.as_mut_ptr().add(off) as *mut GlyphMetric, *m);
        }
    }
    out[data_offset as usize..].copy_from_slice(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mirui::render::font::FontProvider;
    use mirui::render::font::sdf::SdfFontProvider;

    #[test]
    fn quantize_maps_against_spread_not_per_glyph_max() {
        // With spread=2: +spread -> 15, 0 -> 8 (round of 7.5),
        // -spread -> 0. The encoder must NOT renormalize by the
        // per-glyph max (which would map the largest |d| to 15
        // regardless of spread) — that mismatch with the decoder is
        // the uneven-coverage bug.
        let signed = vec![2.0, 0.0, 0.0, -2.0];
        let q4 = quantize(&signed, 4, 2.0);
        assert_eq!(q4.len(), 2);
        assert_eq!(q4[0] & 0x0F, 15, "+spread -> 15");
        assert_eq!(q4[0] >> 4, 8, "0 -> round(7.5) = 8");
        assert_eq!(q4[1] >> 4, 0, "-spread -> 0");
    }

    #[test]
    fn rasterize_simple_square_fills_inside() {
        let mut path = MirPath::new();
        path.move_to(Point {
            x: Fixed::from_int(2),
            y: Fixed::from_int(2),
        });
        path.line_to(Point {
            x: Fixed::from_int(8),
            y: Fixed::from_int(2),
        });
        path.line_to(Point {
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        });
        path.line_to(Point {
            x: Fixed::from_int(2),
            y: Fixed::from_int(8),
        });
        path.close();

        let cov = rasterize_to_coverage(&path, 10);
        // Centre pixel (5,5) should be fully inside.
        assert_eq!(cov[5 * 10 + 5], 255);
        // Outside corner (0,0) should be empty.
        assert_eq!(cov[0], 0);
    }

    #[test]
    fn distance_transform_zero_at_edge_positive_inside() {
        // 4×4 with inside region at row/col 1..=2.
        let mut cov = vec![0u8; 16];
        for y in 1..=2 {
            for x in 1..=2 {
                cov[y * 4 + x] = 255;
            }
        }
        let signed = euclidean_distance_transform(&cov, 4, 4);
        // Centre of the inside square should be positive.
        assert!(signed[1 * 4 + 1] > 0.0);
        // Far outside corner should be negative.
        assert!(signed[0] < 0.0);
    }

    #[test]
    fn pack_and_parse_round_trip() {
        let size: u16 = 4;
        let bit_depth = 4;
        let spread = 1;
        let bpg = bytes_per_glyph(size, bit_depth);
        let metrics = vec![
            GlyphMetric {
                codepoint: b'A' as u32,
                advance: 5,
                bearing_x: 0,
                bearing_y: 0,
            },
            GlyphMetric {
                codepoint: b'B' as u32,
                advance: 6,
                bearing_x: 1,
                bearing_y: -1,
            },
        ];
        let mut data = Vec::new();
        for i in 0..2 {
            data.extend(std::iter::repeat_n((i + 1) as u8, bpg));
        }

        let body = pack_payload(&metrics, &data, size, bit_depth, spread, 3, 1, 5, bpg);
        let mut payload = vec![0u8; FONT_CHUNK_HEADER_LEN + body.len()];
        FontChunkHeader {
            kind: FontChunkKind::Sdf,
            format: bit_depth,
            size,
        }
        .write(&mut payload[..FONT_CHUNK_HEADER_LEN]);
        payload[FONT_CHUNK_HEADER_LEN..].copy_from_slice(&body);
        let mirx_bytes =
            encode_chunk_generic(chunk_type::FONT, mirx::ChunkEntry::FLAG_CRITICAL, &payload);

        // Leak so we can hand the slice to SdfFontProvider, which
        // expects `&'static`. Tests are short-lived.
        let leaked: &'static [u8] = Box::leak(mirx_bytes.into_boxed_slice());
        let parsed = mirx::parse_chunk(leaked).unwrap();
        let got_payload = parsed.chunk_payload(leaked, chunk_type::FONT).unwrap();

        let provider = SdfFontProvider::from_mirx_chunk(got_payload).unwrap();
        assert_eq!(provider.metrics().len(), 2);
        assert_eq!(provider.glyph('A').unwrap().advance, 5);
        assert_eq!(provider.glyph('B').unwrap().advance, 6);
    }
}
