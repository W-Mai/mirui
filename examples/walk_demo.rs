use mirui::app::App;
use mirui::backend::sdl::SdlBackend;
use mirui::layout::*;
use mirui::types::Color;
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlBackend::new("mirui - walk demo", 480, 320);
    let mut app = App::new(backend);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Some(480),
            height: Some(320),
            ..Default::default()
        })
        .id();

    let colors = [
        Color::rgb(88, 166, 255),
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
        Color::rgb(210, 168, 255),
        Color::rgb(255, 200, 50),
    ];

    let show_footer = true;

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        container (direction: FlexDirection::Column, grow: 1.0) {
            walk colors.iter() with color {
                item (bg_color: *color, grow: 1.0, border_radius: 4) {}
            }
            if show_footer {
                footer (bg_color: Color::rgb(50, 50, 70), height: 30, text: "conditional!") {}
            }
        }
    };

    app.set_root(root);
    app.run();
}
