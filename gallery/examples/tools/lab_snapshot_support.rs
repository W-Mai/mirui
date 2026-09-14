use std::env;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;

type SnapshotSurface = FramebufSurface<fn(&[u8], &Rect)>;
pub type SnapshotApp = App<SnapshotSurface>;

fn default_output(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.local/screenshots")
        .join(name)
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

pub fn run(output_name: &str, viewport: (u16, u16), setup: impl FnOnce(&mut SnapshotApp, Entity)) {
    let mut args = env::args().skip(1);
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_output(output_name));
    let width = args
        .next()
        .map(|value| value.parse().expect("viewport width"))
        .unwrap_or(viewport.0);
    let height = args
        .next()
        .map(|value| value.parse().expect("viewport height"))
        .unwrap_or(viewport.1);
    let flush: fn(&[u8], &Rect) = |_, _| {};
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, flush);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = app.spawn_root().id();
    setup(&mut app, root);
    app.set_root(root);
    app.systems.run_all(&mut app.world);
    app.render().unwrap();

    let texture = app.backend.framebuffer();
    assert_eq!(texture.stride, usize::from(width) * 4);
    assert_eq!(
        texture.buf.as_slice().len(),
        texture.stride * usize::from(height)
    );
    write_png(&output, width, height, texture.buf.as_slice());
    println!("wrote {}", output.display());
}
