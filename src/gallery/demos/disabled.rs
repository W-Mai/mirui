extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::components::text::Text;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::Style;
use crate::widget::UserState;
use crate::widget::dirty::Dirty;
use alloc::format;

pub struct ToggleTarget(pub Entity);

pub struct ClickCount(pub u32);

fn toggle_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let target = world.get::<ToggleTarget>(entity).map(|t| t.0);
    let Some(target) = target else { return false };
    if matches!(world.get::<UserState>(target), Some(UserState::Disabled)) {
        world.remove::<UserState>(target);
    } else {
        world.insert(target, UserState::Disabled);
    }
    world.insert(target, Dirty);
    true
}

fn count_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    let next = {
        let Some(c) = world.get_mut::<ClickCount>(entity) else {
            return false;
        };
        c.0 += 1;
        c.0
    };
    let buf = format!("Clicked: {next}");
    if let Some(t) = world.get_mut::<Text>(entity) {
        t.0 = buf.into_bytes();
    }
    world.insert(entity, Dirty);
    true
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    if let Some(style) = world.get_mut::<Style>(parent) {
        style.bg_color = Some(Color::rgb(20, 20, 30).into());
        style.layout = LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(480),
            height: Dimension::px(320),
            padding: Padding::all(24),
            grow: Fixed::ONE,
            ..Default::default()
        };
    }

    let row_e = ui! {
        :(
            parent: parent
            world: world
        :)

        Row (
            direction: FlexDirection::Row,
            grow: 1.0
        ) {
            View (
                bg_color: Color::rgb(63, 185, 80),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Tap me to bump count"
            ) [
                ClickCount(0),
                GestureHandler {
                    on_gesture: count_handler,
                },
            ] {}
            View (
                bg_color: Color::rgb(248, 81, 73),
                grow: 1.0,
                border_radius: 8,
                padding: Padding::all(8),
                text: "Disabled target"
            ) [
                ClickCount(0),
                GestureHandler {
                    on_gesture: count_handler,
                },
            ] {}
        }
    };
    let target = world
        .get::<crate::widget::Children>(row_e)
        .and_then(|c| c.0.get(1).copied())
        .expect("target child of row");

    ui! {
        :(
            parent: parent
            world: world
        :)

        View (
            height: 40,
            bg_color: Color::rgb(88, 166, 255),
            border_radius: 8,
            padding: Padding::all(8),
            text: "Toggle Disabled on right card"
        ) [
            ToggleTarget(target),
            GestureHandler {
                on_gesture: toggle_handler,
            },
        ] {}
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
