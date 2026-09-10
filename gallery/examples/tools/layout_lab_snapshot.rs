use std::env;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use mirui::gallery::demos::layout_lab;
use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;

fn default_output() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../.local/screenshots/layout-lab.png")
}

fn write_png(path: &Path, width: u16, height: u16, pixels: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create snapshot directory");
    }
    let file = BufWriter::new(File::create(path).expect("create snapshot"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(pixels).expect("write PNG pixels");
}

fn main() {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_output);
    let width = args
        .next()
        .map(|value| value.parse().expect("viewport width"))
        .unwrap_or(layout_lab::VIEWPORT.0);
    let height = args
        .next()
        .map(|value| value.parse().expect("viewport height"))
        .unwrap_or(layout_lab::VIEWPORT.1);
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = app.spawn_root().id();
    layout_lab::setup_app(&mut app, root);
    app.set_root(root);
    app.render();

    let texture = app.backend.framebuffer();
    assert_eq!(texture.stride, usize::from(width) * 4);
    assert_eq!(
        texture.buf.as_slice().len(),
        texture.stride * usize::from(height)
    );
    write_png(&output, width, height, texture.buf.as_slice());
    println!("wrote {}", output.display());
}
