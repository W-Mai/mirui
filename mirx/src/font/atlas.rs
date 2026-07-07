pub const SUPPORTED_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 32;
pub const METRIC_LEN: usize = 8;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasHeader {
    pub version: u16,
    pub bit_depth: u8,
    pub _pad0: u8,
    pub source_size: u16,
    pub spread: u16,
    pub glyph_count: u32,
    pub metric_offset: u32,
    pub data_offset: u32,
    pub bytes_per_glyph: u32,
    pub ascender: u16,
    pub descender: u16,
    pub line_height: u16,
    pub _pad1: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphMetric {
    pub codepoint: u32,
    pub advance: u16,
    pub bearing_x: i8,
    pub bearing_y: i8,
}

const _: () = assert!(core::mem::size_of::<AtlasHeader>() == HEADER_LEN);
const _: () = assert!(core::mem::size_of::<GlyphMetric>() == METRIC_LEN);

pub fn read_header(buf: &[u8]) -> AtlasHeader {
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

pub fn write_header(out: &mut [u8], h: &AtlasHeader) {
    out[0..2].copy_from_slice(&h.version.to_le_bytes());
    out[2] = h.bit_depth;
    out[3] = h._pad0;
    out[4..6].copy_from_slice(&h.source_size.to_le_bytes());
    out[6..8].copy_from_slice(&h.spread.to_le_bytes());
    out[8..12].copy_from_slice(&h.glyph_count.to_le_bytes());
    out[12..16].copy_from_slice(&h.metric_offset.to_le_bytes());
    out[16..20].copy_from_slice(&h.data_offset.to_le_bytes());
    out[20..24].copy_from_slice(&h.bytes_per_glyph.to_le_bytes());
    out[24..26].copy_from_slice(&h.ascender.to_le_bytes());
    out[26..28].copy_from_slice(&h.descender.to_le_bytes());
    out[28..30].copy_from_slice(&h.line_height.to_le_bytes());
    out[30..32].copy_from_slice(&h._pad1.to_le_bytes());
}

pub fn read_metric(buf: &[u8]) -> GlyphMetric {
    GlyphMetric {
        codepoint: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        advance: u16::from_le_bytes([buf[4], buf[5]]),
        bearing_x: buf[6] as i8,
        bearing_y: buf[7] as i8,
    }
}

pub fn write_metric(out: &mut [u8], m: &GlyphMetric) {
    out[0..4].copy_from_slice(&m.codepoint.to_le_bytes());
    out[4..6].copy_from_slice(&m.advance.to_le_bytes());
    out[6] = m.bearing_x as u8;
    out[7] = m.bearing_y as u8;
}
