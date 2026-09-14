//! Render the production UI font at the reference size matrix.

extern crate alloc;

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::font::{Font, FontManager, FontToken};
use mirui::render::sw::SwRenderer;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;
use mirui::ui::render_system;
use mirui::ui::widgets::Text;

const FONT_BYTES: &[u8] = include_bytes!("../../../src/gallery/demos/assets/misans_ui.mirx");
const FONT: FontToken = FontToken::Custom("real_font_snapshot");
const SIZES: [u16; 8] = [10, 12, 14, 18, 24, 36, 48, 96];

fn main() {
    let out_path = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        let mut path = PathBuf::from(".local/screenshots");
        path.push("real-font-sizes.png");
        path
    });
    let width = 1_280u16;
    let height = 560u16;
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let font = Font::from_mirx(
        "MiSans UI",
        16,
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("parse production UI font");
    app.world
        .resource::<FontManager>()
        .expect("FontManager")
        .add_static(FONT.cache_key(), font);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(16, 18, 27))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(width.into()),
            height: Dimension::px(height.into()),
            padding: Padding::all(Dimension::px(20)),
            row_gap: Dimension::px(6),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        walk SIZES.iter() with size {
            Row (
                height: i32::from((*size).max(24)) + 10,
                align: AlignItems::Center,
                column_gap: 18
            ) {
                Text (
                    format!("{:>2} px", size),
                    width: 58,
                    font: FONT,
                    font_size: 14,
                    text_color: Color::rgb(112, 126, 150)
                )
                Text (
                    "Ag AVATAR mirui 0123",
                    font: FONT,
                    font_size: *size,
                    text_color: Color::rgb(255, 220, 140)
                )
            }
        }
    };

    app.set_root(root);
    let viewport = Viewport::new(width, height, Fixed::ONE);
    render_system::update_layout(&mut app.world, root, &viewport);
    {
        let texture = app.backend.framebuffer();
        let mut renderer = SwRenderer::new(texture);
        renderer.viewport = viewport;
        render_system::render(&mut app.world, root, &viewport, &mut renderer).unwrap();
    }

    let texture = app.backend.framebuffer();
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    let file = BufWriter::new(File::create(&out_path).expect("create snapshot"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer
        .write_image_data(texture.buf.as_slice())
        .expect("write PNG pixels");
    eprintln!("saved {}", out_path.display());
}
