extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
#[cfg(feature = "std")]
use crate::plugins::{FpsSummaryPlugin, StdInstantClockPlugin};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget;
use crate::widget::dirty::Dirty;

pub struct TapCount(pub u32);

const TAP_COLORS: [Color; 5] = [
    Color::rgb(63, 185, 80),
    Color::rgb(248, 81, 73),
    Color::rgb(210, 168, 255),
    Color::rgb(88, 166, 255),
    Color::rgb(255, 200, 50),
];

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        View (grow: 1.0) {
            View (
                bg_color: Color::rgb(63, 185, 80),
                position: Position::Absolute,
                left: 20,
                top: 20,
                width: 60,
                height: 60,
                border_radius: 8
            ) [
                TapCount(0),
            ] on Tap {
                let count = ctx
                    .world
                    .get_mut::<TapCount>(ctx.entity)
                    .map(|c| {
                        c.0 += 1;
                        c.0
                    })
                    .unwrap_or(0);
                let color = TAP_COLORS[(count as usize) % TAP_COLORS.len()];
                if let Some(style) = ctx.world.get_mut::<widget::Style>(ctx.entity) {
                    style.set_bg_color(color);
                }
                ctx.world.insert(ctx.entity, Dirty);
            }
            View (
                bg_color: Color::rgb(88, 166, 255),
                position: Position::Absolute,
                left: 90,
                top: 90,
                width: 50,
                height: 50,
                border_radius: 25
            ) on DragMove {
                widget::set_position(
                    ctx.world,
                    ctx.entity,
                    Fixed::from_int(90) + *dx,
                    Fixed::from_int(90) + *dy,
                );
            } on DragEnd {
                widget::set_position(
                    ctx.world,
                    ctx.entity,
                    Fixed::from_int(90),
                    Fixed::from_int(90),
                );
            }
            View (
                bg_color: Color::rgb(210, 168, 255),
                position: Position::Absolute,
                left: 200,
                top: 20,
                width: 80,
                height: 80,
                border_radius: 12
            ) on LongPress {
                if let Some(style) = ctx.world.get_mut::<widget::Style>(ctx.entity) {
                    style.set_bg_color(Color::rgb(255, 50, 50));
                }
                ctx.world.insert(ctx.entity, Dirty);
            } on Tap {
                if let Some(style) = ctx.world.get_mut::<widget::Style>(ctx.entity) {
                    style.set_bg_color(Color::rgb(210, 168, 255));
                }
                ctx.world.insert(ctx.entity, Dirty);
            }
        }
    };
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

    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;

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

    fn fire(world: &mut World, entity: Entity, event: &GestureEvent) {
        if let Some(f) = world.get::<GestureHandler>(entity).map(|g| g.on_gesture) {
            f(world, entity, event);
        }
    }

    #[test]
    fn tap_box_counts() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let root = world.get::<Children>(parent).unwrap().0[0];
        let tap_box = world.get::<Children>(root).unwrap().0[0];

        assert_eq!(world.get::<TapCount>(tap_box).map(|c| c.0), Some(0));
        fire(
            &mut world,
            tap_box,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: tap_box,
            },
        );
        assert_eq!(world.get::<TapCount>(tap_box).map(|c| c.0), Some(1));
    }

    #[test]
    fn drag_box_moves_then_resets() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let root = world.get::<Children>(parent).unwrap().0[0];
        let drag_box = world.get::<Children>(root).unwrap().0[1];

        fire(
            &mut world,
            drag_box,
            &GestureEvent::DragMove {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                dx: Fixed::from_int(30),
                dy: Fixed::from_int(40),
                target: drag_box,
            },
        );
        let style = world.get::<widget::Style>(drag_box).unwrap();
        assert_eq!(
            style.layout.left.resolve_or(Fixed::ZERO, Fixed::ZERO),
            Fixed::from_int(120)
        );
        assert_eq!(
            style.layout.top.resolve_or(Fixed::ZERO, Fixed::ZERO),
            Fixed::from_int(130)
        );
    }
}
