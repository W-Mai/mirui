mod backend_snapshot_support;
mod flip_card_fixture;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::framebuf::FramebufSurface;

use backend_snapshot_support::{capture, write_png};
use flip_card_fixture::build;
use gallery::backend_parity::{HEIGHT, WIDTH};

const SCALE: u16 = 2;

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/flip-card-sw.png"));
    let frames = env::args()
        .nth(2)
        .map_or(45, |s| s.parse().expect("frame count"));
    let backend = FramebufSurface::with_scale_and_format(
        WIDTH * SCALE,
        HEIGHT * SCALE,
        Fixed::from(SCALE),
        ColorFormat::RGBA8888,
        |_, _| {},
    );
    let mut app = App::new(backend);
    let root = build(&mut app, frames);
    let viewport = app.backend.display_info().viewport();

    let texture = capture(&mut app, root, viewport, Rect::new(0, 0, WIDTH, HEIGHT))
        .expect("software draw")
        .expect("software framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
