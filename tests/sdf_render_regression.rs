//! Pixel-buffer regression gates for bitmap and SDF text rendering.

use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;

const UI_FONT_BYTES: &[u8] = include_bytes!("../src/gallery/demos/assets/misans_ui.mirx");

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
        let font = Font::from_mirx(
            "MiSans-Regular",
            size,
            UI_FONT_BYTES,
            &mirx::reader::PayloadLimits::HOST,
        )
        .expect("parse font");
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
        render_system::render(&mut app.world, root, &viewport, &mut renderer).unwrap();
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
fn real_font_size_matrix_byte_hashes_are_stable() {
    let hashes = [10, 12, 14, 18, 24, 36, 48, 96].map(|size| {
        let pixels = render_text("Ag", FontToken::Heading, Some(size));
        assert_eq!(
            pixels.len(),
            240 * usize::from(size.saturating_mul(2).max(48)) * 4
        );
        fnv1a64(&pixels)
    });
    assert_eq!(
        hashes,
        [
            0x1e48_de85_444e_197d,
            0x14c7_b990_cf8a_740c,
            0xb5b6_74e6_a854_c85c,
            0x394b_cdc5_6885_2730,
            0xf409_9bb3_6484_9342,
            0x9b55_27bc_4dec_e4b1,
            0xe48c_f827_6451_1b69,
            0xddf1_b7f4_6d14_78ea,
        ],
        "production UI font pixels drifted; inspect the size matrix before updating",
    );
}
