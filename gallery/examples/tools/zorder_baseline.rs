//! Bare z-order test. No DropShadow, no offscreen, no widget-texture-
//! ref machinery. Just two `Style` widgets at the same position in
//! known children order. Whichever colour you see on top *is* what
//! z-order does in this engine — no analysis, no excuses.

use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::Theme;

extern crate alloc;

fn main() {
    let backend = SdlSurface::new("zorder_baseline", 240, 240);
    let mut app = App::new(backend);
    app.with_default_widgets()
        .with_default_systems()
        .with_theme(Theme::dark());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 22, 28))
        .layout(LayoutStyle {
            width: Dimension::px(240),
            height: Dimension::px(240),
            ..Default::default()
        })
        .id();

    // FIRST in children: a fully opaque purple square at (60, 60) 80x80.
    let purple = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (
            bg_color: Color::rgb(120, 40, 180),
            position: Position::Absolute,
            left: 60,
            top: 60,
            width: 80,
            height: 80
        ) {}
    };

    // SECOND in children: a fully opaque white square at (60, 60) 80x80
    // — exact same position, same size.
    let white = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (
            bg_color: Color::rgb(245, 245, 250),
            position: Position::Absolute,
            left: 60,
            top: 60,
            width: 80,
            height: 80
        ) {}
    };

    if let Some(c) = app.world.get::<mirui::widget::Children>(root) {
        eprintln!(
            "children order = {:?} (purple={:?}, white={:?})",
            c.0, purple, white
        );
    }

    app.set_root(root);
    app.run();
}
