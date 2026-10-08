//! Bare z-order test. No DropShadow, no offscreen, no widget-texture-
//! ref machinery. Just two `Style` widgets at the same position in
//! known children order. The white square is painted after the purple square.

use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::ui::Theme;

extern crate alloc;

#[compose]
fn overlapping_squares() -> (Entity, Entity) {
    // FIRST in children: a fully opaque purple square at (60, 60) 80x80.
    let purple = ui! {
        View (
            bg_color: Color::rgb(120, 40, 180),
            position: Position::Absolute,
            left: 60,
            top: 60,
            width: 80,
            height: 80
        )
    };

    // SECOND in children: a fully opaque white square at (60, 60) 80x80
    // — exact same position, same size.
    let white = ui! {
        View (
            bg_color: Color::rgb(245, 245, 250),
            position: Position::Absolute,
            left: 60,
            top: 60,
            width: 80,
            height: 80
        )
    };
    (purple, white)
}

fn main() {
    let backend = SdlSurface::new("zorder_baseline", 240, 240);
    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark());

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(240),
            height: Dimension::px(240),
            ..Default::default()
        })
        .id();

    let (purple, white) = app.compose(root, overlapping_squares());

    if let Some(c) = app.world.get::<mirui::ui::Children>(root) {
        eprintln!(
            "children order = {:?} (purple={:?}, white={:?})",
            c.0, purple, white
        );
    }

    app.run();
}
