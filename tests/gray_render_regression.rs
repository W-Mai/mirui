//! Regression gate: pixel-buffer hash for the grayscale rendering
//! path. If a refactor changes the rendered bytes, the hash mismatches
//! and the test fails, forcing a deliberate review + baseline update.
//!
//! The atlas under test is `tests/fixtures/misans_gray_16_4bit.mirx`,
//! committed to the repo, so the path is reproducible on any machine.

use mirui::prelude::*;
use mirui::render::font::FontManager;
use mirui::render::font::gray;
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;
use mirx::{chunk_type, parse_chunk};

const GRAY_ATLAS_BYTES: &[u8] = include_bytes!("fixtures/misans_gray_16_4bit.mirx");

/// FNV-1a 64-bit hash. No external crate required, stable forever.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn render_gray_text(text: &str) -> Vec<u8> {
    let text: String = text.into();
    let width: u16 = 240;
    let height: u16 = 48;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let parsed = parse_chunk(GRAY_ATLAS_BYTES).expect("parse mirx");
    let payload = parsed
        .chunk_payload(GRAY_ATLAS_BYTES, chunk_type::FONT)
        .expect("FONT chunk");
    let font = gray::font_from_mirx_chunk("MiSans-Regular", payload).expect("parse gray atlas");
    app.world
        .resource::<FontManager>()
        .expect("FontManager")
        .add_static(FontToken::Heading.cache_key(), font);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width as i32),
            height: Dimension::px(height as i32),
            padding: Padding::all(Dimension::px(8)),
            justify: JustifyContent::Center,
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        Text (
            text,
            font: FontToken::Heading,
            text_color: Color::rgb(255, 220, 140)
        )
    };

    app.set_root(root);
    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&mut app.world, root, &viewport, &mut renderer);
    }

    let tex = app.backend.framebuffer();
    tex.buf.as_slice().to_vec()
}

/// Grayscale render is the small-text path. Its bytes must not drift
/// across refactors. On a deliberate pipeline change, regenerate the
/// hash with `cargo test --features std --test gray_render_regression
/// -- --nocapture` and update the constant after eye-checking.
#[test]
fn gray_hello_byte_hash_is_stable() {
    let pixels = render_gray_text("Hello!");
    let hash = fnv1a64(&pixels);
    assert_eq!(pixels.len(), 240 * 48 * 4);
    assert_eq!(
        hash, 0x7a1a_4c9e_e1e9_6623,
        "grayscale MiSans render drifted; eye-check the snapshot before pinning a new value (hash={hash:#018x})",
    );
}

/// The grayscale render must put real ink down — guards against a
/// silent regression where the dispatch arm skips drawing.
#[test]
fn gray_render_is_not_blank() {
    let pixels = render_gray_text("Hello!");
    let lit = pixels.chunks_exact(4).filter(|px| px[0] > 40).count();
    assert!(lit > 50, "expected lit pixels, got {lit}");
}
