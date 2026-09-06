//! End-to-end representation selection in one MIRX FONT face.

#![cfg(feature = "std")]

use mirui::render::font::mirx::MirxFontProvider;
use mirui::render::font::{FontProvider, GlyphKind};
use mirx::{
    FontRepresentationFallback, FontRepresentationKind, FontRepresentationRequest, PayloadLimits,
};

const BUNDLE: &[u8] = include_bytes!("../src/gallery/demos/assets/multi_font_bundle.mirx");

fn open() -> MirxFontProvider {
    MirxFontProvider::from_mirx(BUNDLE, &PayloadLimits::HOST).expect("face")
}

#[test]
fn face_holds_all_representations() {
    assert_eq!(open().view().tables().len(), 3);
}

#[test]
fn fixed_size_routes_to_coverage() {
    let glyph = open().glyph('2', 12).expect("glyph");
    let GlyphKind::Raster { representation, .. } = glyph.kind else {
        panic!("raster");
    };
    assert!(matches!(
        representation.kind(),
        FontRepresentationKind::Coverage { .. }
    ));
}

#[test]
fn oversized_request_routes_to_sdf() {
    let glyph = open().glyph('2', 96).expect("glyph");
    let GlyphKind::Raster { representation, .. } = glyph.kind else {
        panic!("raster");
    };
    assert!(matches!(
        representation.kind(),
        FontRepresentationKind::SignedDistance { .. }
    ));
}

#[test]
fn metrics_and_glyphs_select_the_same_representation_for_every_size_class() {
    let provider = open();
    for requested in [10, 11, 12, 64] {
        let selected = provider
            .view()
            .tables()
            .select(
                FontRepresentationRequest::new(requested)
                    .with_fallback(FontRepresentationFallback::Nearest),
            )
            .unwrap();
        let glyph = provider.glyph('2', requested).unwrap();
        let GlyphKind::Raster { representation, .. } = glyph.kind else {
            panic!("raster");
        };
        assert_eq!(representation, selected.record().representation());

        let source = selected.metrics().line_metrics();
        let scale = mirui::types::Fixed::from_int(i32::from(requested))
            / mirui::types::Fixed::from_int(i32::from(representation.design_ppem()));
        let metrics = provider.metrics(requested);
        assert_eq!(
            metrics.ascender,
            mirui::types::Fixed::from(source.ascent()) * scale
        );
        assert_eq!(
            metrics.descender,
            mirui::types::Fixed::from(source.descent()) * scale
        );
        assert_eq!(
            metrics.line_height,
            mirui::types::Fixed::from(source.line_height()) * scale
        );
    }
}
