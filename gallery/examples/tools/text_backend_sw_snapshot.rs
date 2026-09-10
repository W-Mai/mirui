mod text_backend_fixture;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Viewport;

use text_backend_fixture::{HEIGHT, WIDTH, build, capture, write_png};

const SCALE: u16 = 2;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/text-parity-sw.png"));
    let physical_width = WIDTH * SCALE;
    let physical_height = HEIGHT * SCALE;
    let backend = FramebufSurface::with_format(
        physical_width,
        physical_height,
        ColorFormat::RGBA8888,
        |_, _| {},
    );
    let mut app = App::new(backend);
    let root = build(&mut app);
    let viewport = Viewport::new(physical_width, physical_height, Fixed::from(SCALE));
    let texture = capture(&mut app, root, viewport).expect("software framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
