use mirui::anim::ease;
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::plugins::{InputFeedbackPlugin, StdInstantClockPlugin};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;

extern crate alloc;

fn main() {
    let backend = SdlSurface::new("mirui — input feedback", 640, 360);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.add_plugin(InputFeedbackPlugin::new());

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(18, 22, 32))
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(640),
            height: Dimension::px(360),
            padding: Padding::all(28),
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        column (
            direction: FlexDirection::Column,
            grow: 1.0
        ) {
            title (
                height: 32,
                text: "Input feedback: cursor highlight + rotary / wheel water-drop",
                text_color: Color::rgb(201, 209, 217)
            ) {}
            spacer (height: 20) {}
            row (
                direction: FlexDirection::Row,
                grow: 1.0
            ) {
                card_a (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(34, 74, 44),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(80, 140, 90),
                    text: "Hover A",
                    text_color: Color::rgb(220, 240, 225),
                    padding: Padding::all(20)
                ) {}
                gap1 (width: 20) {}
                card_b (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(38, 58, 96),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(88, 166, 255),
                    text: "Hover B",
                    text_color: Color::rgb(220, 235, 255),
                    padding: Padding::all(20)
                ) {}
                gap2 (width: 20) {}
                card_c (
                    grow: 1.0,
                    height: 180,
                    bg_color: Color::rgb(82, 38, 38),
                    border_radius: 16,
                    border_width: 1,
                    border_color: Color::rgb(248, 81, 73),
                    text: "Hover C",
                    text_color: Color::rgb(255, 225, 225),
                    padding: Padding::all(20)
                ) {}
            }
        }
    };

    let left = Point {
        x: Fixed::from_int(80),
        y: Fixed::from_int(190),
    };
    let mid = Point {
        x: Fixed::from_int(320),
        y: Fixed::from_int(190),
    };
    let right = Point {
        x: Fixed::from_int(560),
        y: Fixed::from_int(190),
    };
    app.world.insert_resource(
        SimTimeline::new(alloc::vec![
            SimAction::move_to(left, mid, 1400, ease::ease_in_out_cubic),
            SimAction::wait(300),
            SimAction::move_to(mid, right, 1400, ease::ease_in_out_cubic),
            SimAction::wait(300),
            SimAction::rotate(8, 50),
            SimAction::wait(600),
            SimAction::rotate(-8, 50),
            SimAction::wait(600),
            SimAction::move_to(right, left, 1600, ease::ease_in_out_cubic),
            SimAction::wait(500),
        ])
        .looping(true),
    );

    app.set_root(root);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin::default());
    app.run();
}
