mod backend_snapshot_support;

use std::env;
use std::path::PathBuf;

use mirui::prelude::*;
use mirui::surface::sdl_gpu::{SdlGpuConfig, SdlGpuFactory, SdlGpuSurface};

use backend_snapshot_support::write_png;
use gallery::backend_parity::{HEIGHT, WIDTH, build};

fn main() {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".local/screenshots/text-parity-sdl.png"));
    let backend = SdlGpuSurface::with_config(
        "mirui text parity",
        WIDTH,
        HEIGHT,
        SdlGpuConfig {
            vsync: false,
            ..Default::default()
        },
    )
    .expect("SDL GPU surface");
    let mut app = App::with_factory(backend, SdlGpuFactory::new());
    build(&mut app);
    let texture = app
        .snapshot()
        .expect("SDL draw")
        .expect("SDL framebuffer readback");
    write_png(&path, &texture);
    eprintln!("saved {}", path.display());
}
