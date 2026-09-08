use super::*;
use crate::{
    Fixed,
    font::GlyphMap,
    image::{Region, SampleLayout, SurfaceRequirements},
};

fn wire_fixed(bits: i32) -> Fixed {
    Fixed::from_le_bytes(bits.to_le_bytes())
}

#[test]
fn unicode_lookup_keeps_metrics_and_samples_on_the_same_ordinal() {
    let mut chars = [0; 16];
    for (bytes, cp) in chars
        .chunks_exact_mut(4)
        .zip(['\0', 'A', '中', '\u{10ffff}'])
    {
        bytes.copy_from_slice(&(cp as u32).to_le_bytes());
    }
    let codepoints = FontCodepoints::open(&chars).unwrap();
    let line = LineMetrics::new(wire_fixed(1025), wire_fixed(-513), wire_fixed(2049)).unwrap();
    let records = [
        GlyphMetrics::new(wire_fixed(-1), wire_fixed(i32::MIN), wire_fixed(i32::MAX)),
        GlyphMetrics::new(wire_fixed(769), wire_fixed(-128), wire_fixed(513)),
        GlyphMetrics::new(wire_fixed(1024), wire_fixed(1), wire_fixed(-257)),
        GlyphMetrics::default(),
    ];
    let mut bytes = [0; 60];
    line.encode_record_into(&mut bytes).unwrap();
    for (slot, record) in bytes[12..].chunks_exact_mut(12).zip(records) {
        record.encode_record_into(slot).unwrap();
    }
    let metrics = MetricsTable::open(&bytes).unwrap();
    let map = GlyphMap::glyph_major(1, 1, 4).unwrap();
    let data = [11, 22, 33, 44];
    let glyphs = RawGlyphs::builder(map, SampleLayout::A8)
        .build(&data)
        .unwrap();
    let table = GlyphTable::new(codepoints, metrics, glyphs).unwrap();
    assert_eq!(table.len(), 4);
    assert!(!table.is_empty());
    assert_eq!(table.codepoints(), codepoints);
    assert_eq!(table.line_metrics(), line);
    for (index, cp) in codepoints.into_iter().enumerate() {
        let glyph = table.glyph(cp).unwrap();
        assert_eq!(Some(glyph), table.get(index));
        assert_eq!(glyph.metrics(), records[index]);
        assert_eq!(
            glyph.raster().storage().plane(0).unwrap().bytes(),
            &data[index..index + 1]
        );
    }
    for cp in ['\u{1}', '@', 'B', '\u{d7ff}', '\u{e000}', '\u{10fffe}'] {
        assert_eq!(table.glyph(cp), None);
    }
    assert_eq!(table.get(4), None);
    assert_eq!(table.get(usize::MAX), None);
}

#[test]
fn all_counts_must_agree_before_joined_lookup_exists() {
    let chars = [65, 0, 0, 0, 66, 0, 0, 0];
    let mut bytes = [0; 36];
    LineMetrics::new(wire_fixed(1), wire_fixed(0), wire_fixed(1))
        .unwrap()
        .encode_record_into(&mut bytes)
        .unwrap();
    for codepoint_count in 0..=2 {
        let codepoints = FontCodepoints::open(&chars[..codepoint_count * 4]).unwrap();
        for metric_count in 0..=2 {
            let metrics = MetricsTable::open(&bytes[..12 + metric_count * 12]).unwrap();
            for glyph_count in 0..=2 {
                let map = GlyphMap::glyph_major(1, 1, glyph_count).unwrap();
                let glyphs = RawGlyphs::builder(map, SampleLayout::A8)
                    .build(&[0; 2][..glyph_count])
                    .unwrap();
                let result = GlyphTable::new(codepoints, metrics, glyphs);
                if codepoint_count == metric_count && metric_count == glyph_count {
                    assert_eq!(result.unwrap().len(), glyph_count);
                } else {
                    assert_eq!(
                        result.unwrap_err(),
                        GlyphTableError::Cardinality {
                            codepoints: codepoint_count,
                            metrics: metric_count,
                            glyphs: glyph_count,
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn resolved_glyph_outlives_all_metadata_and_preserves_empty_rasters() {
    let data = [128; 4];
    let (glyph, space, line) = {
        let chars = [32, 0, 0, 0, 65, 0, 0, 0];
        let codepoints = FontCodepoints::open(&chars).unwrap();
        let line = LineMetrics::new(wire_fixed(1024), wire_fixed(-256), wire_fixed(1280)).unwrap();
        let mut bytes = [0; 37];
        line.encode_record_into(&mut bytes[1..]).unwrap();
        GlyphMetrics::new(wire_fixed(512), wire_fixed(0), wire_fixed(0))
            .encode_record_into(&mut bytes[13..])
            .unwrap();
        GlyphMetrics::new(wire_fixed(640), wire_fixed(-128), wire_fixed(256))
            .encode_record_into(&mut bytes[25..])
            .unwrap();
        let metrics = MetricsTable::open(&bytes[1..]).unwrap();
        let regions = [
            Region::new(0, 0, 0, 0).unwrap(),
            Region::new(1, 0, 1, 2).unwrap(),
        ];
        let map = GlyphMap::atlas(2, 2, &regions).unwrap();
        let glyphs = RawGlyphs::builder(map, SampleLayout::A8)
            .build(&data)
            .unwrap();
        let table = GlyphTable::new(codepoints, metrics, glyphs).unwrap();
        (
            table.glyph('A').unwrap(),
            table.glyph(' ').unwrap(),
            table.line_metrics(),
        )
    };
    assert_eq!(glyph.metrics().advance(), wire_fixed(640));
    assert_eq!(space.metrics().advance(), wire_fixed(512));
    assert!(space.raster().region().is_empty());
    assert_eq!(line.line_height(), wire_fixed(1280));
    let mut out = [0xa5; 4];
    for glyph in [space, glyph] {
        let plan = glyph
            .raster()
            .memory_plan(SurfaceRequirements::new())
            .unwrap();
        glyph.raster().copy_into(&mut out, plan).unwrap();
    }
    assert_eq!(out, [128, 128, 0xa5, 0xa5]);
}
