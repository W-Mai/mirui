extern crate alloc;

use mirui::app::App;
use mirui::components::text_input::{Placeholder, TextInput};
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlSurface::new("TextInput Demo", 480, 200);
    let mut app = App::new(backend).with_default_widgets();

    app.add_system(mirui::anim::sync_delta_time_ms);

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
