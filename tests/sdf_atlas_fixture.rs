//! End-to-end tests for generated MIRX signed-distance faces.

use mirui::render::font::FontProvider;
use mirui::render::font::mirx::MirxFontProvider;
use mirx::font::FontRepresentationKind;
use mirx::reader::PayloadLimits;

const ASCII_FONT: &[u8] = include_bytes!("fixtures/misans_sdf_ascii_32.mirx");
const CJK_FONT: &[u8] = include_bytes!("fixtures/misans_sdf_cjk_32.mirx");
const UI_FONT: &[u8] = include_bytes!("../src/gallery/demos/assets/misans_ui.mirx");

fn open(bytes: &'static [u8]) -> MirxFontProvider {
    MirxFontProvider::from_mirx(bytes, &PayloadLimits::HOST).expect("SDF face")
}

#[test]
fn face_retains_sdf_contract_and_sorted_cmap() {
    let provider = open(ASCII_FONT);
    let view = provider.view();
    let record = view.representations().get(0).unwrap().representation();
    assert_eq!(record.design_ppem(), 32);
    assert!(matches!(
        record.kind(),
        FontRepresentationKind::SignedDistance { bits: 8, spread: 4 }
    ));
    let scalars: Vec<_> = view.cmap().iter().map(|entry| entry.scalar()).collect();
    assert!(scalars.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn resolves_ascii_with_explicit_stride_and_region() {
    let provider = open(ASCII_FONT);
    for ch in [
        'A', 'Z', 'a', 'z', '0', '9', ' ', '.', '!', '?', '@', '#', '&',
    ] {
        let glyph = provider
            .map_char(ch)
            .and_then(|glyph| provider.raster(glyph, 32))
            .unwrap_or_else(|| panic!("missing glyph {ch:?}"));
        assert!(matches!(
            glyph.representation.kind(),
            FontRepresentationKind::SignedDistance { bits: 8, .. }
        ));
        assert_eq!(glyph.surface.stride(), glyph.surface.width());
        if ch == ' ' {
            assert!(glyph.region.is_none());
            continue;
        }
        let region = glyph.region.unwrap();
        assert!(region.width() > 0 && region.height() > 0);
        assert!(region.x() + region.width() <= glyph.surface.width());
        assert!(region.y() + region.height() <= glyph.surface.height());
    }
}

#[test]
fn atlas_contains_a_distance_gradient() {
    let provider = open(ASCII_FONT);
    let glyph = provider
        .raster(provider.map_char('A').unwrap(), 32)
        .unwrap();
    let samples = glyph.surface.samples();
    let mut buckets = [false; 256];
    for &byte in samples {
        buckets[usize::from(byte)] = true;
    }
    assert!(buckets.iter().filter(|used| **used).count() > 16);
}

#[test]
fn cjk_face_resolves_common_glyphs_with_nonzero_advance() {
    let provider = open(CJK_FONT);
    for ch in ['我', '你', '是', '不', '中', '人', 'A', '0'] {
        let glyph = provider
            .map_char(ch)
            .and_then(|glyph| provider.raster(glyph, 32))
            .unwrap_or_else(|| panic!("missing glyph {ch:?}"));
        assert!(
            provider.glyph_advance(provider.map_char(ch).unwrap(), 32)
                > Some(mirui::types::Fixed::ZERO)
        );
        assert!(glyph.region.is_some());
    }
}

#[test]
fn ui_face_routes_small_text_and_scalable_ranges_to_distinct_representations() {
    let provider = open(UI_FONT);
    let glyph = provider.map_char('A').unwrap();
    let cases = [
        (11, FontRepresentationKind::Coverage { bits: 8 }, 11),
        (14, FontRepresentationKind::Coverage { bits: 8 }, 14),
        (
            24,
            FontRepresentationKind::SignedDistance { bits: 8, spread: 4 },
            24,
        ),
        (
            96,
            FontRepresentationKind::SignedDistance { bits: 8, spread: 8 },
            64,
        ),
    ];

    for (ppem, kind, design_ppem) in cases {
        let raster = provider.raster(glyph, ppem).unwrap();
        assert_eq!(raster.representation.kind(), kind);
        assert_eq!(raster.representation.design_ppem(), design_ppem);
    }
}
