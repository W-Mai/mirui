mod backend_snapshot_support;
mod projective_demo_fixture;

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use mirui::prelude::*;
use mirui::render::{RenderError, wgpu::WgpuRendererFactory};
use mirui::surface::wgpu_surface::WgpuSurface;

use backend_snapshot_support::{capture, write_png};
use gallery::backend_parity::{HEIGHT, WIDTH};
use projective_demo_fixture::{Demo, build};

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/projective-wgpu.png"));
    let frames = env::args()
        .nth(2)
        .map_or(45, |s| s.parse().expect("frame count"));
    let demo = Demo::parse(&env::args().nth(3).unwrap_or_else(|| "flip_card".into()));
    let backend = WgpuSurface::new("mirui projective parity", WIDTH, HEIGHT);
    let mut app = App::with_factory(backend, WgpuRendererFactory::new());
    let root = build(&mut app, demo, frames);

    let full = Rect::new(0, 0, WIDTH, HEIGHT);
    let mut texture = None;
    for _ in 0..16 {
        while app.backend.poll_event().is_some() {}
        let viewport = app.backend.display_info().viewport();
        match capture(&mut app, root, viewport, full) {
            Ok(Some(frame)) => {
                texture = Some(frame);
                break;
            }
            Err(RenderError::BackendFailure) => {
                app.backend.flush(&full);
                std::thread::sleep(Duration::from_millis(16));
            }
            Ok(None) => panic!("WGPU framebuffer readback was empty"),
            Err(error) => panic!("WGPU snapshot failed: {error:?}"),
        }
    }
    let texture = texture.expect("WGPU swapchain remained unavailable");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
