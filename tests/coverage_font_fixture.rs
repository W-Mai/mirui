//! End-to-end checks for a generated MIRX coverage face.

use mirui::render::font::FontProvider;
use mirui::render::font::mirx::MirxFontProvider;
use mirx::font::FontRepresentationKind;
use mirx::reader::PayloadLimits;

const FONT_BYTES: &[u8] = include_bytes!("fixtures/misans_coverage_16_4bit.mirx");

fn open() -> MirxFontProvider {
    MirxFontProvider::from_mirx(FONT_BYTES, &PayloadLimits::HOST).expect("coverage face")
}

#[test]
fn face_retains_generation_geometry() {
    let provider = open();
    let record = provider.view().representations().get(0).unwrap();
    assert_eq!(record.representation().design_ppem(), 16);
    assert!(matches!(
        record.representation().kind(),
        FontRepresentationKind::Coverage { bits: 4 }
    ));
    assert_eq!(provider.view().face().raster_count(), 69);
}

#[test]
fn resolves_ascii_through_strided_regions() {
    let provider = open();
    for ch in ['A', 'Z', 'a', 'z', '0', '9', '!', '?'] {
        let glyph = provider
            .map_char(ch)
            .and_then(|glyph| provider.raster(glyph, 16, 16))
            .unwrap_or_else(|| panic!("missing glyph {ch:?}"));
        assert!(matches!(
            glyph.representation.kind(),
            FontRepresentationKind::Coverage { bits: 4 }
        ));
        assert_eq!(glyph.surface.stride(), 8);
        let region = glyph.region.unwrap();
        assert_eq!((region.width(), region.height()), (16, 16));
    }
}

#[test]
fn misses_codepoint_outside_charset() {
    assert!(open().map_char('中').is_none());
}

#[test]
fn capital_a_contains_both_coverage_values() {
    let provider = open();
    let glyph = provider
        .raster(provider.map_char('A').unwrap(), 16, 16)
        .unwrap();
    let samples = glyph.surface.samples();
    assert!(samples.iter().any(|byte| *byte != 0));
    assert!(samples.iter().any(|byte| *byte != u8::MAX));
}
