mod text_backend_fixture;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::surface::sdl_gpu::{SdlGpuFactory, SdlGpuSurface};

use text_backend_fixture::{HEIGHT, WIDTH, build, capture, write_png};

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/text-parity-sdl.png"));
    let backend = SdlGpuSurface::new_with_vsync("mirui text parity", WIDTH, HEIGHT, false);
    let mut app = App::with_factory(backend, SdlGpuFactory::new());
    let root = build(&mut app);
    let viewport = app.backend.display_info().viewport();
    let texture = capture(&mut app, root, viewport)
        .expect("SDL draw")
        .expect("SDL framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
