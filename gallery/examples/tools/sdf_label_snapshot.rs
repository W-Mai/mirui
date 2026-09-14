//! Render a string with the Gallery MiSans font bundle to PNG.
//!
//! Usage:
//!   cargo run --release -p gallery --example sdf_label_snapshot -- \
//!     [text] [out.png] [size] [scale] [font.mirx]
//!
//! Defaults to "Hello mirui!" → `.local/screenshots/sdf-hello.png`.

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::ui::widgets::Text;

const FONT_BYTES: &[u8] = include_bytes!("../../../src/gallery/demos/assets/misans_ui.mirx");

fn load_misans_font(size: u16, path: Option<&str>) -> Font {
    let bytes = path.map_or(FONT_BYTES, |path| {
        Box::leak(std::fs::read(path).expect("read font").into_boxed_slice())
    });
    Font::from_mirx(
        "MiSans-Regular",
        size,
        bytes,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("parse font")
}

fn main() {
    let mut args = env::args().skip(1);
    let text: String = args.next().unwrap_or_else(|| "Hello mirui!".into());
    let out_path: PathBuf = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let dir = std::env::var("MIRUI_SNAPSHOT_DIR")
            .unwrap_or_else(|_| format!("{}/../.local/screenshots", env!("CARGO_MANIFEST_DIR")));
        std::fs::create_dir_all(&dir).ok();
        let mut p = PathBuf::from(dir);
        p.push("sdf-hello.png");
        p
    });
    let size = args
        .next()
        .map(|value| value.parse::<u16>().expect("size must be a u16"))
        .unwrap_or(32);
    let scale = args
        .next()
        .map(|value| {
            value
                .parse::<f32>()
                .expect("scale must be a positive number")
        })
        .unwrap_or(1.0);
    assert!(scale.is_finite() && scale > 0.0);
    let font_path = args.next();

    let logical_width: u16 = 640;
    let logical_height = size.saturating_mul(2).max(80);
    let width = physical_extent(logical_width, scale);
    let height = physical_extent(logical_height, scale);
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    {
        let mgr = app.world.resource::<FontManager>().expect("FontManager");
        mgr.add_static(
            FontToken::Heading.cache_key(),
            load_misans_font(size, font_path.as_deref()),
        );
    }

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(logical_width as i32),
            height: Dimension::px(logical_height as i32),
            padding: Padding::all(Dimension::px(16)),
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
            text.clone(),
            font: FontToken::Heading,
            text_color: Color::rgb(255, 220, 140)
        )
    };

    app.set_root(root);

    use mirui::render::sw::SwRenderer;
    use mirui::types::Viewport;
    use mirui::ui::render_system;

    let viewport = Viewport::new(width, height, Fixed::from_f32(scale));
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        use mirui::surface::FramebufferAccess;
        let tex = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(tex);
        renderer.viewport = viewport;
        render_system::render(&mut app.world, root, &viewport, &mut renderer).unwrap();
    }

    use mirui::surface::FramebufferAccess;
    let tex = app.backend.framebuffer();
    let pixels = tex.buf.as_slice();
    assert_eq!(tex.stride, usize::from(width) * 4);
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let file = BufWriter::new(File::create(&out_path).expect("create PNG"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(pixels).expect("write PNG pixels");
    eprintln!("saved {}", out_path.display());
}

fn physical_extent(logical: u16, scale: f32) -> u16 {
    let physical = f32::from(logical) * scale;
    assert!(physical >= 1.0 && physical <= f32::from(u16::MAX));
    physical.ceil() as u16
}
