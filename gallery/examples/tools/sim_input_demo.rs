use mirui::anim::ease;
use mirui::app::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::input::event::gesture::DragMove;
use mirui::input::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::prelude::*;
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, DimPoint, Dimension, Fixed};

extern crate alloc;

#[component]
struct TapCount(u32);

#[event]
fn tap_handler(entity: entity!(), counters: query!(mut TapCount), commands: commands!()) -> bool {
    let count = counters
        .get(entity)
        .map(|c| {
            c.0 += 1;
            c.0
        })
        .unwrap_or(0);
    let colors = [
        Color::rgb(63, 185, 80),
        Color::rgb(248, 81, 73),
        Color::rgb(210, 168, 255),
        Color::rgb(88, 166, 255),
        Color::rgb(255, 200, 50),
    ];
    commands.set::<mirui::ui::property::prop::BackgroundColor>(
        entity,
        colors[(count as usize) % colors.len()].into(),
    );
    true
}

#[event]
fn drag_move(entity: entity!(), input: event!(DragMove), commands: commands!()) -> bool {
    commands.set::<mirui::ui::property::prop::Left>(
        entity,
        Dimension::Px(Fixed::from_int(140) + input.dx),
    );
    commands.set::<mirui::ui::property::prop::Top>(
        entity,
        Dimension::Px(Fixed::from_int(90) + input.dy),
    );
    true
}

#[event]
fn drag_end(entity: entity!(), commands: commands!()) -> bool {
    commands.set::<mirui::ui::property::prop::Left>(entity, Dimension::px(140));
    commands.set::<mirui::ui::property::prop::Top>(entity, Dimension::px(90));
    true
}

#[compose]
fn input_targets() -> (Entity, Entity) {
    let tap_box = ui! {
        View (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 30,
            top: 30,
            width: 80,
            height: 80,
            border_radius: 10
        ) [
            TapCount(0),
        ] on Tap(tap_handler::callback())
    };

    let drag_box = ui! {
        View (
            bg_color: Color::rgb(88, 166, 255),
            position: Position::Absolute,
            left: 140,
            top: 90,
            width: 60,
            height: 60,
            border_radius: 30
        ) on DragMove(drag_move::callback()) on DragEnd(drag_end::callback())
    };
    (tap_box, drag_box)
}

fn main() {
    let backend = SdlSurface::new("mirui - simulated input demo", 320, 240);
    let mut app = App::new(backend);
    app.with_default_widgets().with_default_systems();
    app.with_event_capacity(
        EventCapacity::new().commands(
            CommandCapacity::new()
                .set::<mirui::ui::property::prop::BackgroundColor>(1)
                .set::<mirui::ui::property::prop::Left>(1)
                .set::<mirui::ui::property::prop::Top>(1),
        ),
    );

    app.add_system(sim_timeline_system::system());

    let root = app
        .spawn_root()
        .ignore_safe_area()
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            width: Dimension::px(320),
            height: Dimension::px(240),
            ..Default::default()
        })
        .id();

    let (tap_box, drag_box) = app.compose(root, input_targets());

    app.add_resource(
        SimTimeline::new(vec![
            SimAction::tap(DimPoint::CENTER).on(tap_box),
            SimAction::wait(300),
            SimAction::tap(DimPoint::CENTER).on(tap_box),
            SimAction::wait(300),
            SimAction::drag(DimPoint::CENTER, (120, -30), 600, ease::ease_in_out_cubic)
                .on(drag_box),
            SimAction::wait(200),
            SimAction::drag(DimPoint::CENTER, (-60, 90), 800, ease::ease_out_quad).on(drag_box),
            SimAction::wait(500),
            SimAction::tap(DimPoint::CENTER).on(tap_box),
            SimAction::wait(500),
        ])
        .looping(true),
    );

    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
