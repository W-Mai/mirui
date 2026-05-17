use mirui::app::App;
use mirui::ecs::{Entity, World};
use mirui::event::GestureHandler;
use mirui::event::gesture::GestureEvent;
use mirui::layout::*;
use mirui::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use mirui::surface::sdl::SdlSurface;
use mirui::types::{Color, Dimension, Fixed};
use mirui::widget::builder::WidgetBuilder;
use mirui::widget::dirty::Dirty;
use mirui_macros::ui;

extern crate alloc;

struct TapCount(u32);

fn tap_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { .. } => {
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
            let color = colors[(count as usize) % colors.len()];
            if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
                style.set_bg_color(color);
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

fn drag_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragMove { dx, dy, .. } => {
            let base_x = Fixed::from_int(90);
            let base_y = Fixed::from_int(90);
            mirui::widget::set_position(world, entity, base_x + *dx, base_y + *dy);
            true
        }
        GestureEvent::DragEnd { .. } => {
            mirui::widget::set_position(world, entity, Fixed::from_int(90), Fixed::from_int(90));
            true
        }
        _ => false,
    }
}

fn longpress_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::LongPress { .. } => {
            if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
                style.set_bg_color(Color::rgb(255, 50, 50));
            }
            world.insert(entity, Dirty);
            true
        }
        GestureEvent::Tap { .. } => {
            if let Some(style) = world.get_mut::<mirui::widget::Style>(entity) {
                style.set_bg_color(Color::rgb(210, 168, 255));
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

fn main() {
    let backend = SdlSurface::new("mirui - gesture demo", 320, 240);
    let mut app = App::new(backend).with_default_widgets();

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
            left: 20,
            top: 20,
            width: 60,
            height: 60,
            border_radius: 8
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
            left: 90,
            top: 90,
            width: 50,
            height: 50,
            border_radius: 25
        ) {}
    };
    app.world.insert(
        drag_box,
        GestureHandler {
            on_gesture: drag_handler,
        },
    );

    let lp_box = ui! {
        :(
            parent: root
            world: &mut app.world
        :)

        lp_box (
            bg_color: Color::rgb(210, 168, 255),
            position: Position::Absolute,
            left: 200,
            top: 20,
            width: 80,
            height: 80,
            border_radius: 12
        ) {}
    };
    app.world.insert(
        lp_box,
        GestureHandler {
            on_gesture: longpress_handler,
        },
    );

    app.set_root(root);
    app.add_plugin(StdInstantClockPlugin::default())
        .add_plugin(FpsSummaryPlugin::default());
    app.run();
}
