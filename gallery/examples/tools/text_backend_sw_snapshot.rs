mod backend_snapshot_support;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;

use backend_snapshot_support::write_png;
use gallery::backend_parity::{HEIGHT, WIDTH, build};

const SCALE: u16 = 2;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/text-parity-sw.png"));
    let physical_width = WIDTH * SCALE;
    let physical_height = HEIGHT * SCALE;
    let backend = FramebufSurface::with_scale_and_format(
        physical_width,
        physical_height,
        Fixed::from(SCALE),
        ColorFormat::RGBA8888,
        |_, _| {},
    );
    let mut app = App::new(backend);
    build(&mut app);
    let texture = app
        .snapshot()
        .expect("software draw")
        .expect("software framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
