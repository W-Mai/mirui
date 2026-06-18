//! Regression gate: pixel-buffer hashes for the Mono (Bitmap8x8) and
//! SDF rendering paths. If a refactor changes the rendered bytes, the
//! hash mismatches and the test fails, forcing a deliberate review +
//! baseline update.
//!
//! The atlas under test is `tests/fixtures/misans_regular_ascii_32_4bit.mirx`
//! committed to the repo, so the SDF path is reproducible on any
//! machine.

use mirui::prelude::*;
use mirui::render::font::sdf::SdfFontProvider;
use mirui::render::font::{Font, FontBackend, FontManager, FontToken};
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;
use mirx::{chunk_type, parse_chunk};

const ATLAS_BYTES: &[u8] = include_bytes!("fixtures/misans_regular_ascii_32_4bit.mirx");

/// FNV-1a 64-bit hash. No external crate required, stable forever.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn render_text(text: &str, font_token: FontToken, register_misans: bool) -> Vec<u8> {
    let width: u16 = 240;
    let height: u16 = 48;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    if register_misans {
        let parsed = parse_chunk(ATLAS_BYTES).expect("parse mirx");
        let payload = parsed
            .chunk_payload(ATLAS_BYTES, chunk_type::FONT)
            .expect("FONT chunk");
        let provider = SdfFontProvider::from_mirx_chunk(payload).expect("parse atlas");
        let size = provider.header().source_size;
        let font = Font {
            family: "MiSans-Regular",
            size,
            backend: FontBackend::Custom(std::rc::Rc::new(provider)),
        };
        app.world
            .resource::<FontManager>()
            .expect("FontManager")
            .add_static(FontToken::Heading.cache_key(), font);
    }

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
            font: font_token.clone(),
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

/// Bitmap8x8 baseline. The 8x8 ASCII bitmap font is the source of
/// truth for "no-frills text rendering" — its bytes must not drift
/// across refactors. If a deliberate pipeline change shifts the
/// rendered bytes, regenerate the hash with `cargo test --features
/// std --test sdf_render_regression -- --nocapture` and update the
/// constant below.
#[test]
fn mono_hello_byte_hash_is_stable() {
    let pixels = render_text("Hello mirui!", FontToken::Default, false);
    let hash = fnv1a64(&pixels);
    assert_eq!(pixels.len(), 240 * 48 * 4);
    assert_eq!(
        hash, 0xd517_388e_9ce9_545c,
        "Mono Bitmap8x8 render drifted; eye-check the snapshot before pinning a new value",
    );
}

#[test]
fn sdf_hello_byte_hash_is_stable() {
    let pixels = render_text("Hello!", FontToken::Heading, true);
    let hash = fnv1a64(&pixels);
    assert_eq!(pixels.len(), 240 * 48 * 4);
    assert_eq!(
        hash, 0xf9c6_b198_2b2a_281c,
        "SDF MiSans render drifted; eye-check the snapshot before pinning a new value",
    );
}
