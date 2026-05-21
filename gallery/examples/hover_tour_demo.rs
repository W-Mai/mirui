use mirui::anim::ease;
use mirui::app::App;
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::layout::*;
use mirui::plugins::StdInstantClockPlugin;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Point};
use mirui::widget::builder::WidgetBuilder;
use mirui_macros::ui;

fn main() {
    let backend = SdlSurface::new("mirui — hover tour", 720, 360);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();

    let surface_bg = Color::rgb(13, 17, 23);
    let card_a = Color::rgb(34, 74, 44);
    let card_b = Color::rgb(82, 38, 38);
    let card_c = Color::rgb(34, 56, 86);
    let card_border = Color::rgb(48, 54, 61);
    let title_color = Color::rgb(201, 209, 217);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(surface_bg)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(720),
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
                height: 36,
                text: "MoveTo demo: simulated cursor sweeps the row, hover overlays follow",
                text_color: title_color
            ) {}
            spacer (height: 16) {}
            row (
                direction: FlexDirection::Row,
                grow: 1.0
            ) {
                a_card (
                    bg_color: card_a,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card A",
                    text_color: title_color
                ) {}
                spacer1 (width: 16) {}
                b_card (
                    bg_color: card_b,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card B",
                    text_color: title_color
                ) {}
                spacer2 (width: 16) {}
                c_card (
                    bg_color: card_c,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Card C",
                    text_color: title_color
                ) {}
            }
        }
    };

    let left = Point {
        x: Fixed::from_int(80),
        y: Fixed::from_int(200),
    };
    let right = Point {
        x: Fixed::from_int(640),
        y: Fixed::from_int(200),
    };
    let timeline = SimTimeline::new(alloc::vec![
        SimAction::move_to(left, right, 2400, ease::ease_in_out_cubic),
        SimAction::wait(400),
        SimAction::move_to(right, left, 2400, ease::ease_in_out_cubic),
        SimAction::wait(400),
    ])
    .looping(true);
    app.world.insert_resource(timeline);

    app.set_root(root);
    app.add_system(sim_timeline_system::system());
    app.add_plugin(StdInstantClockPlugin::default());
    app.run();
}

extern crate alloc;
