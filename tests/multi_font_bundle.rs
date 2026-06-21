//! End-to-end check that a real `gen-mirx bundle` artifact loads and
//! routes by size. The bundle merges a 12px grayscale pixel table and a
//! 24px SDF table; `MultiFontProvider` must pick gray for small sizes
//! and SDF once the request outgrows the largest pixel table.

#![cfg(feature = "std")]

use mirui::render::font::FontProvider;
use mirui::render::font::GlyphKind;
use mirui::render::font::multi::MultiFontProvider;

const BUNDLE: &[u8] = include_bytes!("../src/gallery/demos/assets/multi_font_bundle.mirx");

fn open() -> MultiFontProvider {
    MultiFontProvider::from_mirx(BUNDLE).expect("parse bundle")
}

#[test]
fn bundle_holds_all_representations() {
    let p = open();
    assert_eq!(p.repr_count(), 3);
}

#[test]
fn small_size_routes_to_grayscale() {
    let p = open();
    let g = p.glyph('2', 12).expect("glyph at 12");
    assert!(
        matches!(g.kind, GlyphKind::Grayscale { .. } | GlyphKind::Mono { .. }),
        "expected a pixel table at 12, got {:?}",
        g.kind
    );
}

#[test]
fn oversized_request_routes_to_sdf() {
    let p = open();
    let g = p.glyph('2', 96).expect("glyph at 96");
    assert!(
        matches!(g.kind, GlyphKind::Sdf { .. }),
        "expected the SDF table at 96, got {:?}",
        g.kind
    );
}
