mod snapshot_render;

use std::env;
use std::path::PathBuf;

use mirui::gallery::DemoSize;

fn clamp_axis(preferred: u16, minimum: Option<u16>, maximum: Option<u16>) -> u16 {
    let lower = minimum.unwrap_or(1);
    let upper = maximum.unwrap_or(u16::MAX).max(lower);
    preferred.clamp(lower, upper)
}

fn preview_size(size: DemoSize) -> (u16, u16) {
    if let Some(fixed) = size.fixed_size() {
        return fixed;
    }
    (
        clamp_axis(480, size.min_width, size.max_width),
        clamp_axis(320, size.min_height, size.max_height),
    )
}

fn main() {
    let output = env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("output directory");

    for demo in gallery::DEMOS {
        snapshot_render::render(
            &output.join(format!("{}.png", demo.slug)),
            preview_size(demo.size),
            true,
            |app| gallery::setup_demo(demo.slug, app).expect("registered demo"),
        );
    }
}
