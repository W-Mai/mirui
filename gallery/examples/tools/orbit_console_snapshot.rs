//! Render the Orbit Console showcase to a deterministic PNG.

use std::env;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use mirui::ecs::DeltaTimeMs;
use mirui::gallery::demos::orbit_console::{self, DemoRunMode};
use mirui::prelude::*;
use mirui::render::texture::ColorFormat;
use mirui::surface::FramebufferAccess;
use mirui::surface::framebuf::FramebufSurface;

const DEFAULT_WIDTH: u16 = orbit_console::VIEWPORT.0;
const DEFAULT_HEIGHT: u16 = orbit_console::VIEWPORT.1;

#[derive(Debug, Default)]
struct PaletteCounts {
    non_background: usize,
    mint: usize,
    blue: usize,
    violet: usize,
    amber: usize,
    bright: usize,
}

fn default_output() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/assets/orbit-console.png")
}

fn count_palette(pixels: &[u8], stride: usize, width: u16, height: u16) -> PaletteCounts {
    let mut counts = PaletteCounts::default();
    for y in 0..height as usize {
        for pixel in pixels[y * stride..y * stride + width as usize * 4].chunks_exact(4) {
            let [r, g, b, _] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            counts.non_background += usize::from(r > 12 || g > 22 || b > 35);
            counts.mint += usize::from(g > 145 && b > 115 && g > r.saturating_add(30));
            counts.blue += usize::from(b > 130 && b > r.saturating_add(25));
            counts.violet += usize::from(r > 85 && b > 115 && b > g.saturating_add(20));
            counts.amber += usize::from(r > 90 && g > 65 && b < 100 && r > g.saturating_add(10));
            counts.bright += usize::from(r > 180 && g > 180 && b > 180);
        }
    }
    counts
}

fn luminance_range(
    pixels: &[u8],
    stride: usize,
    x_range: core::ops::Range<usize>,
    y_range: core::ops::Range<usize>,
) -> u16 {
    let mut low = u16::MAX;
    let mut high = 0;
    for y in y_range {
        for x in x_range.clone() {
            let offset = y * stride + x * 4;
            let luma = (pixels[offset] as u16 * 3
                + pixels[offset + 1] as u16 * 6
                + pixels[offset + 2] as u16)
                / 10;
            low = low.min(luma);
            high = high.max(luma);
        }
    }
    high.saturating_sub(low)
}

fn validate(pixels: &[u8], stride: usize, width: u16, height: u16) {
    assert_eq!(stride, width as usize * 4, "unexpected framebuffer stride");
    assert_eq!(
        pixels.len(),
        stride * height as usize,
        "unexpected byte count"
    );

    let counts = count_palette(pixels, stride, width, height);
    let area = usize::from(width) * usize::from(height);
    println!("palette {counts:?}");
    assert!(
        counts.non_background > area / 4,
        "showcase rendered too little content"
    );
    assert!(counts.mint > area / 200, "mint accent is missing");
    assert!(counts.blue > area / 200, "blue accent is missing");
    assert!(counts.violet > area / 1_200, "violet accent is missing");
    assert!(counts.amber > area / 5_000, "amber accent is missing");
    assert!(counts.bright > area / 800, "high-contrast text is missing");

    assert!(
        luminance_range(
            pixels,
            stride,
            0..usize::from(width),
            0..usize::from(height)
        ) > 120,
        "showcase has insufficient luminance range"
    );
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
        .unwrap_or(DEFAULT_WIDTH);
    let height = args
        .next()
        .map(|value| value.parse().expect("viewport height"))
        .unwrap_or(DEFAULT_HEIGHT);
    let ticks = args
        .next()
        .map(|value| value.parse().expect("animation ticks"))
        .unwrap_or(0);
    let backend = FramebufSurface::with_format(width, height, ColorFormat::RGBA8888, |_, _| {});
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = app.spawn_root().id();
    orbit_console::setup(&mut app, root, DemoRunMode::Capture);
    app.set_root(root);
    app.world.insert_resource(DeltaTimeMs(100));
    for _ in 0..ticks {
        orbit_console::console_animation_system(&mut app.world);
        mirui::core::reactive::flush_signal_dirty(&mut app.world);
    }
    app.render().unwrap();

    let texture = app.backend.framebuffer();
    let stride = texture.stride;
    let pixels = texture.buf.as_slice();
    validate(pixels, stride, width, height);
    write_png(&output, width, height, pixels);
    println!("wrote {}", output.display());
}
