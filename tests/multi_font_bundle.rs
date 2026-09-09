//! End-to-end representation selection in one MIRX FONT face.

#![cfg(feature = "std")]

use mirui::render::font::FontProvider;
use mirui::render::font::mirx::MirxFontProvider;
use mirx::{
    font::{FontRepresentationFallback, FontRepresentationKind, FontRepresentationRequest},
    reader::PayloadLimits,
};

const BUNDLE: &[u8] = include_bytes!("../src/gallery/demos/assets/multi_font_bundle.mirx");

fn open() -> MirxFontProvider {
    MirxFontProvider::from_mirx(BUNDLE, &PayloadLimits::HOST).expect("face")
}

#[test]
fn face_holds_all_representations() {
    assert_eq!(open().view().representations().len(), 3);
}

#[test]
fn fixed_size_routes_to_coverage() {
    let provider = open();
    let glyph = provider
        .raster(provider.map_char('2').unwrap(), 12)
        .unwrap();
    assert!(matches!(
        glyph.representation.kind(),
        FontRepresentationKind::Coverage { .. }
    ));
}

#[test]
fn oversized_request_routes_to_sdf() {
    let provider = open();
    let glyph = provider
        .raster(provider.map_char('2').unwrap(), 96)
        .unwrap();
    assert!(matches!(
        glyph.representation.kind(),
        FontRepresentationKind::SignedDistance { .. }
    ));
}

#[test]
fn glyphs_follow_representation_selection_while_face_metrics_scale() {
    let provider = open();
    for requested in [10, 11, 12, 64] {
        let selected = provider
            .view()
            .select(
                FontRepresentationRequest::new(requested)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .unwrap();
        let glyph = provider
            .raster(provider.map_char('2').unwrap(), requested)
            .unwrap();
        assert_eq!(glyph.representation, selected.record().representation());

        let source = provider.view().face();
        let scale = f32::from(requested) / f32::from(source.units_per_em());
        let ascender = mirui::types::Fixed::from_f32(source.ascender().to_f32() * scale);
        let descender = mirui::types::Fixed::from_f32(source.descender().to_f32() * scale);
        let line_gap = mirui::types::Fixed::from_f32(source.line_gap().to_f32() * scale);
        let metrics = provider.metrics(requested);
        assert_eq!(metrics.ascender, ascender);
        assert_eq!(metrics.descender, descender);
        assert_eq!(metrics.line_height, ascender - descender + line_gap);
    }
}
