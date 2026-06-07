use mirui::event::scroll::{ScrollConfig, ScrollOffset};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

fn main() {
    let backend = SdlSurface::new("mirui - scroll demo", 480, 320);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            ..Default::default()
        })
        .id();

    let colors = [
        ("Item 0", Color::rgb(88, 166, 255)),
        ("Item 1", Color::rgb(63, 185, 80)),
        ("Item 2", Color::rgb(248, 81, 73)),
        ("Item 3", Color::rgb(210, 168, 255)),
        ("Item 4", Color::rgb(255, 200, 50)),
        ("Item 5", Color::rgb(150, 100, 200)),
        ("Item 6", Color::rgb(100, 200, 150)),
        ("Item 7", Color::rgb(200, 100, 100)),
    ];

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        View (direction: FlexDirection::Column, bg_color: Color::rgb(40, 40, 60), grow: 1.0) [
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
            ScrollConfig {
                direction: mirui::event::scroll::ScrollAxis::Vertical,
                elastic: true,
                content_height: Fixed::from_int(480),
                content_width: Fixed::ZERO,
            },
        ] {
            walk colors.iter() with item {
                Row (bg_color: item.1, height: 60, border_radius: 4, text: item.0) {}
            }
        }
    };

    app.set_root(root);
    app.run();
}
