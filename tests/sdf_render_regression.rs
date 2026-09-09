//! Pixel-buffer regression gates for bitmap and SDF text rendering.

use mirui::prelude::*;
use mirui::render::font::mirx::font_from_mirx;
use mirui::render::font::{FontManager, FontToken};
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;

const ATLAS_BYTES: &[u8] = include_bytes!("fixtures/misans_sdf_ascii_32.mirx");

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn render_text(text: &str, font_token: FontToken, size: Option<u16>) -> Vec<u8> {
    let text: String = text.into();
    let width: u16 = 240;
    let height = size.map_or(48, |value| value.saturating_mul(2).max(48));
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    if let Some(size) = size {
        let mut font = font_from_mirx(
            "MiSans-Regular",
            ATLAS_BYTES,
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("parse font");
        font.size = size;
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

#[test]
fn mono_hello_byte_hash_is_stable() {
    let pixels = render_text("Hello mirui!", FontToken::Default, None);
    let hash = fnv1a64(&pixels);
    assert_eq!(pixels.len(), 240 * 48 * 4);
    assert_eq!(
        hash, 0x2c8a_ab42_6930_ead4,
        "Mono Bitmap8x8 render drifted; eye-check the snapshot before pinning a new value",
    );
}

#[test]
fn sdf_size_range_byte_hashes_are_stable() {
    let hashes = [16, 32, 64].map(|size| {
        let pixels = render_text("Ag mirui", FontToken::Heading, Some(size));
        assert_eq!(
            pixels.len(),
            240 * usize::from(size.saturating_mul(2).max(48)) * 4
        );
        fnv1a64(&pixels)
    });
    assert_eq!(
        hashes,
        [
            0xddbb_8922_fef1_5740,
            0x1563_d3b8_6c20_0e77,
            0x8c8d_6fac_88a0_fec3,
        ],
        "SDF MiSans size range drifted; inspect the rendered edge before updating",
    );
}
