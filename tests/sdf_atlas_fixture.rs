//! End-to-end tests for generated MIRX signed-distance faces.

use mirui::render::font::mirx::MirxFontProvider;
use mirui::render::font::{FontProvider, GlyphKind};
use mirx::PayloadLimits;
use mirx::font::FontRepresentationKind;

const ASCII_FONT: &[u8] = include_bytes!("fixtures/misans_sdf_ascii_32.mirx");
const CJK_FONT: &[u8] = include_bytes!("fixtures/misans_sdf_cjk_32.mirx");

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
        FontRepresentationKind::SignedDistance { bits: 4, spread: 4 }
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
            .glyph(ch, 32)
            .unwrap_or_else(|| panic!("missing glyph {ch:?}"));
        let GlyphKind::Raster {
            stride,
            region,
            representation,
            ..
        } = glyph.kind
        else {
            panic!("raster");
        };
        assert!(matches!(
            representation.kind(),
            FontRepresentationKind::SignedDistance { bits: 4, .. }
        ));
        assert_eq!(stride, 16);
        assert_eq!((region.width(), region.height()), (32, 32));
    }
}

#[test]
fn atlas_contains_a_distance_gradient() {
    let glyph = open(ASCII_FONT).glyph('A', 32).unwrap();
    let GlyphKind::Raster { samples, .. } = glyph.kind else {
        panic!("raster");
    };
    let mut buckets = [0u32; 16];
    for &byte in samples {
        buckets[(byte & 15) as usize] += 1;
        buckets[(byte >> 4) as usize] += 1;
    }
    assert!(buckets.iter().filter(|count| **count > 0).count() >= 4);
}

#[test]
fn cjk_face_resolves_common_glyphs_with_nonzero_advance() {
    let provider = open(CJK_FONT);
    for ch in ['我', '你', '是', '不', '中', '人', 'A', '0'] {
        let glyph = provider
            .glyph(ch, 32)
            .unwrap_or_else(|| panic!("missing glyph {ch:?}"));
        assert!(glyph.advance > mirui::types::Fixed::ZERO);
        assert!(matches!(glyph.kind, GlyphKind::Raster { .. }));
    }
}
