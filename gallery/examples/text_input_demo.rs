extern crate alloc;

use mirui::components::{Placeholder, TextInput};
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

fn main() {
    let backend = SdlSurface::new("TextInput Demo", 480, 200);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(20, 20, 30))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(200),
            padding: Padding {
                top: Dimension::px(40),
                left: Dimension::px(40),
                right: Dimension::px(40),
                bottom: Dimension::px(40),
            },
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        input (
            bg_color: Color::rgb(40, 40, 56),
            border_color: Color::rgb(80, 80, 100),
            border_radius: 4,
            width: 400,
            height: 28
        ) [
            TextInput::new(),
            Placeholder("type something..."),
        ] {}
    };

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
