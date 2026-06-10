extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::text::Text;
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::UserState;
use crate::widget::dirty::Dirty;
use alloc::format;

pub struct ClickCount(pub u32);

pub fn build_widgets(world: &mut World, parent: Entity) {
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (grow: 1.0, padding: Padding::all(24)) {
            Row (grow: 1.0) {
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    grow: 1.0,
                    border_radius: 8,
                    padding: Padding::all(8),
                    text: "Tap me to bump count"
                ) [
                    ClickCount(0),
                ] on Tap {
                    let next = ctx
                        .world
                        .get_mut::<ClickCount>(ctx.entity)
                        .map(|c| {
                            c.0 += 1;
                            c.0
                        });
                    if let Some(next) = next {
                        let buf = format!("Clicked: {next}");
                        if let Some(t) = ctx.world.get_mut::<Text>(ctx.entity) {
                            t.0 = buf.into_bytes();
                        }
                        ctx.world.insert(ctx.entity, Dirty);
                    }
                }
                View (
                    bg_color: Color::rgb(248, 81, 73),
                    grow: 1.0,
                    border_radius: 8,
                    padding: Padding::all(8),
                    text: "Disabled target",
                    id: "target_card"
                ) [
                    ClickCount(0),
                ] on Tap {
                    let next = ctx
                        .world
                        .get_mut::<ClickCount>(ctx.entity)
                        .map(|c| {
                            c.0 += 1;
                            c.0
                        });
                    if let Some(next) = next {
                        let buf = format!("Clicked: {next}");
                        if let Some(t) = ctx.world.get_mut::<Text>(ctx.entity) {
                            t.0 = buf.into_bytes();
                        }
                        ctx.world.insert(ctx.entity, Dirty);
                    }
                }
            }
            View (
                height: 40,
                bg_color: Color::rgb(88, 166, 255),
                border_radius: 8,
                padding: Padding::all(8),
                text: "Toggle Disabled on right card"
            ) on Tap {
                if let Some(target) = ctx.world.find_by_id("target_card") {
                    if matches!(ctx.world.get::< UserState > (target), Some(UserState::Disabled)) {
                        ctx.world.remove::<UserState>(target);
                    } else {
                        ctx.world.insert(target, UserState::Disabled);
                    }
                    ctx.world.insert(target, Dirty);
                }
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

    fn tap(world: &mut World, entity: Entity) {
        let h = world.get::<GestureHandler>(entity).map(|g| g.on_gesture);
        if let Some(f) = h {
            f(
                world,
                entity,
                &GestureEvent::Tap {
                    x: crate::types::Fixed::ZERO,
                    y: crate::types::Fixed::ZERO,
                    target: entity,
                },
            );
        }
    }

    #[test]
    fn tap_card_bumps_count_and_text() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let card = world.find_by_id("target_card").expect("target_card id");
        assert_eq!(world.get::<ClickCount>(card).map(|c| c.0), Some(0));
        tap(&mut world, card);
        assert_eq!(world.get::<ClickCount>(card).map(|c| c.0), Some(1));
        assert_eq!(
            world.get::<Text>(card).map(|t| t.0.clone()),
            Some(b"Clicked: 1".to_vec())
        );
    }

    #[test]
    fn toggle_button_disables_target_card() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let card = world.find_by_id("target_card").expect("target_card id");
        let column = world.get::<Children>(parent).unwrap().0[0];
        let toggle = world.get::<Children>(column).unwrap().0[1];

        assert!(world.get::<UserState>(card).is_none());
        tap(&mut world, toggle);
        assert!(matches!(
            world.get::<UserState>(card),
            Some(UserState::Disabled)
        ));
        tap(&mut world, toggle);
        assert!(world.get::<UserState>(card).is_none());
    }
}
