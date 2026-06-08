extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget;
use crate::widget::dirty::Dirty;

pub struct TapCount(pub u32);

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
            if let Some(style) = world.get_mut::<widget::Style>(entity) {
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
            widget::set_position(world, entity, base_x + *dx, base_y + *dy);
            true
        }
        GestureEvent::DragEnd { .. } => {
            widget::set_position(world, entity, Fixed::from_int(90), Fixed::from_int(90));
            true
        }
        _ => false,
    }
}

fn longpress_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::LongPress { .. } => {
            if let Some(style) = world.get_mut::<widget::Style>(entity) {
                style.set_bg_color(Color::rgb(255, 50, 50));
            }
            world.insert(entity, Dirty);
            true
        }
        GestureEvent::Tap { .. } => {
            if let Some(style) = world.get_mut::<widget::Style>(entity) {
                style.set_bg_color(Color::rgb(210, 168, 255));
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<widget::Style>(parent) {
        style.bg_color = Some(Color::rgb(30, 30, 46).into());
        style.layout = LayoutStyle {
            width: Dimension::px(320),
            height: Dimension::px(240),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let tap_box = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(63, 185, 80),
            position: Position::Absolute,
            left: 20,
            top: 20,
            width: 60,
            height: 60,
            border_radius: 8
        ) {}
    };
    world.insert(tap_box, TapCount(0));
    world.insert(
        tap_box,
        GestureHandler {
            on_gesture: tap_handler,
        },
    );

    let drag_box = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(88, 166, 255),
            position: Position::Absolute,
            left: 90,
            top: 90,
            width: 50,
            height: 50,
            border_radius: 25
        ) {}
    };
    world.insert(
        drag_box,
        GestureHandler {
            on_gesture: drag_handler,
        },
    );

    let lp_box = ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            bg_color: Color::rgb(210, 168, 255),
            position: Position::Absolute,
            left: 200,
            top: 20,
            width: 80,
            height: 80,
            border_radius: 12
        ) {}
    };
    world.insert(
        lp_box,
        GestureHandler {
            on_gesture: longpress_handler,
        },
    );
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(StdInstantClockPlugin)
        .add_plugin(FpsSummaryPlugin::default());
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    #[test]
    fn build_widgets_smoke() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| !c.0.is_empty()),
        );
    }
}
