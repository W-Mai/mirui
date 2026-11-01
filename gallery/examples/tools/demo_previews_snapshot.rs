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
    let selected = env::args().nth(2);
    if let Some(slug) = &selected {
        assert!(gallery::lookup_demo(slug).is_some(), "unknown demo: {slug}");
    }

    for demo in gallery::DEMOS {
        if selected.as_ref().is_some_and(|slug| slug != demo.slug) {
            continue;
        }
        snapshot_render::render(
            &output.join(format!("{}.png", demo.slug)),
            if demo.slug == "nearby" && env::var_os("NEARBY_ACTIONS").is_some() {
                (640, 480)
            } else {
                preview_size(demo.size)
            },
            true,
            |app| {
                if demo.slug == "nearby" && env::var_os("NEARBY_ACTIONS").is_some() {
                    let root = app.spawn_root().id();
                    let state = mirui::gallery::demos::nearby::setup_app(app, root);
                    state.launcher().choose_game(0, 0);
                    root
                } else {
                    gallery::setup_demo(demo.slug, app).expect("registered demo")
                }
            },
        );
    }
}
