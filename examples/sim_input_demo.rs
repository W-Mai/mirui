use mirui::anim::{FrameClock, ease};
use mirui::app::App;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::event::sim::{SimAction, SimTimeline, sim_timeline_system};
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed, Point};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

extern crate alloc;

struct TapCount(u32);

fn tap_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        let count = world
            .get_mut::<TapCount>(entity)
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
        if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
            style.bg_color = Some(colors[(count as usize) % colors.len()]);
        }
        world.insert(entity, Dirty);
        true
    } else {
        false
    }
}

fn drag_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragMove { dx, dy, .. } => {
            mirui::widget::set_position(
                world,
                entity,
                Fixed::from_int(140) + *dx,
                Fixed::from_int(90) + *dy,
            );
            true
        }
        GestureEvent::DragEnd { .. } => {
            mirui::widget::set_position(world, entity, Fixed::from_int(140), Fixed::from_int(90));
            true
        }
        _ => false,
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - simulated input demo", 320, 240);
    let mut app = App::new(backend);

    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now);
    app.world.insert_resource(FrameClock::new(|| {
        START.get().unwrap().elapsed().as_nanos() as u64
    }));

    app.add_system(mirui::anim::sync_delta_time_ms);
    app.add_system(sim_timeline_system);

    let root = WidgetBuilder::new(&mut app.world)
        .bg_color(Color::rgb(30, 30, 46))
        .layout(LayoutStyle {
            width: Dimension::px(320),
            height: Dimension::px(240),
            ..Default::default()
        })
        .id();

    let tap_box = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        tap_box (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 30,
            top: 30,
            width: 80,
            height: 80,
            border_radius: 10
        ) {}
    };
    app.world.insert(tap_box, TapCount(0));
    app.world.insert(
        tap_box,
        GestureHandler {
            on_gesture: tap_handler,
        },
    );

    let drag_box = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        drag_box (
            bg_color: Color::rgb(88, 166, 255),
            position: Position::Absolute,
            left: 140,
            top: 90,
            width: 60,
            height: 60,
            border_radius: 30
        ) {}
    };
    app.world.insert(
        drag_box,
        GestureHandler {
            on_gesture: drag_handler,
        },
    );

    app.world.insert_resource(
        SimTimeline::new(vec![
            SimAction::Tap(Point::new(70, 70)),
            SimAction::Wait(300),
            SimAction::Tap(Point::new(70, 70)),
            SimAction::Wait(300),
            SimAction::Drag {
                from: Point::new(170, 120),
                to: Point::new(260, 60),
                duration_ms: 600,
                ease: ease::ease_in_out_cubic,
            },
            SimAction::Wait(200),
            SimAction::Drag {
                from: Point::new(170, 120),
                to: Point::new(80, 180),
                duration_ms: 800,
                ease: ease::ease_out_quad,
            },
            SimAction::Wait(500),
            SimAction::Tap(Point::new(70, 70)),
            SimAction::Wait(500),
        ])
        .looping(true),
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
