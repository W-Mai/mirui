//! Render Iris with the software rasterizer at every showcase size.
//!
//! `cargo run -p gallery --example iris_snapshot -- <output-directory> [milliseconds]`
//! writes `iris-<theme>-<width>x<height>.png` for the light and dark palettes
//! after advancing the capture loop to the given time.

use std::env;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mirui::ecs::MonoClock;
use mirui::gallery::demos::iris::{self, IrisRun};
use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;

const SIZES: [(u16, u16); 5] = [(128, 160), (240, 240), (390, 844), (800, 480), (1280, 800)];
const STEP_MS: u64 = 50;

/// A stepped clock: the frame-time system then reports exactly `STEP_MS`.
static NOW_NS: AtomicU64 = AtomicU64::new(0);

fn stepped_clock() -> u64 {
    NOW_NS.load(Ordering::Relaxed)
}

fn write_png(path: &Path, width: u16, height: u16, pixels: &[u8]) {
    let file = BufWriter::new(File::create(path).expect("create snapshot"));
    let mut encoder = png::Encoder::new(file, width.into(), height.into());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("write PNG header");
    writer.write_image_data(pixels).expect("write PNG pixels");
}

fn main() {
    let mut args = env::args().skip(1);
    let output = args.next().map(PathBuf::from).expect("output directory");
    let at_ms: u32 = args
        .next()
        .map(|value| value.parse().expect("milliseconds"))
        .unwrap_or(1_800);
    fs::create_dir_all(&output).expect("create output directory");
    for theme in [iris::DARK_ID, iris::LIGHT_ID] {
        for (width, height) in SIZES {
            let surface =
                FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
            let mut app = App::new(surface);
            app.with_default_widgets().with_default_systems();
            let root = app.spawn_root().id();
            iris::setup(&mut app, root, IrisRun::Capture);
            app.set_root(root);
            app.set_theme(theme).expect("theme");
            app.world.insert_resource(MonoClock::new(stepped_clock));
            for _ in 0..u64::from(at_ms) / STEP_MS {
                NOW_NS.fetch_add(STEP_MS * 1_000_000, Ordering::Relaxed);
                app.systems.run_all(&mut app.world).expect("animation");
                mirui::core::reactive::flush_signal_dirty(&mut app.world);
            }
            app.render().expect("render iris");
            let texture = app.backend.framebuffer();
            let view = texture.read_pixels().expect("read pixels");
            let path = output.join(format!("iris-{theme}-{width}x{height}.png"));
            write_png(&path, width, height, view.bytes());
            println!("wrote {}", path.display());
        }
    }
}
