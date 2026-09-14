mod backend_snapshot_support;
mod flip_card_fixture;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::render::ProjectiveFallback;
use mirui::render::sdl_gpu::SdlGpuFactory;
use mirui::surface::sdl_gpu::SdlGpuSurface;

use backend_snapshot_support::{capture, write_png};
use flip_card_fixture::build;
use gallery::backend_parity::{HEIGHT, WIDTH};

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/flip-card-sdl.png"));
    let frames = env::args()
        .nth(2)
        .map_or(45, |s| s.parse().expect("frame count"));
    let backend = SdlGpuSurface::new("mirui flip card parity", WIDTH, HEIGHT);
    let factory = SdlGpuFactory::new()
        .with_projective_fallback(ProjectiveFallback::new(vec![0; 2 * 1024 * 1024]));
    let mut app = App::with_factory(backend, factory);
    let root = build(&mut app, frames);
    let viewport = app.backend.display_info().viewport();

    let texture = capture(&mut app, root, viewport, Rect::new(0, 0, WIDTH, HEIGHT))
        .expect("SDL GPU draw")
        .expect("SDL GPU framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
