use super::attach_to_parent;
#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::UserState;
use crate::widget::dirty::Dirty;

pub struct ToggleErrored;

fn toggle_errored_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if !matches!(event, GestureEvent::Tap { .. }) {
        return false;
    }
    if matches!(world.get::<UserState>(entity), Some(UserState::Errored)) {
        world.remove::<UserState>(entity);
    } else {
        world.insert(entity, UserState::Errored);
    }
    world.insert(entity, Dirty);
    true
}

pub fn build_widgets(world: &mut World, parent: Entity) -> Entity {
    let surface_bg = Color::rgb(13, 17, 23);
    let hover_bg = Color::rgb(34, 74, 44);
    let errored_bg = Color::rgb(82, 38, 38);
    let disabled_bg = Color::rgb(34, 56, 86);
    let card_border = Color::rgb(48, 54, 61);
    let title_color = Color::rgb(201, 209, 217);

    let root = WidgetBuilder::new(world)
        .bg_color(surface_bg)
        .layout(LayoutStyle {
            direction: FlexDirection::Column,
            width: Dimension::px(720),
            height: Dimension::px(420),
            padding: Padding {
                top: 28.into(),
                left: 32.into(),
                right: 32.into(),
                bottom: 28.into(),
            },
            ..Default::default()
        })
        .id();

    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            direction: FlexDirection::Column,
            grow: 1.0
        ) {
            View (
                height: 36,
                text: "WidgetState: Hover / Press / Errored / Disabled",
                text_color: title_color
            ) {}
            View (height: 16) {}
            Row (
                direction: FlexDirection::Row,
                grow: 1.0
            ) {
                View (
                    bg_color: hover_bg,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Hover me / Press me",
                    text_color: title_color
                ) {}
                View (width: 16) {}
                View (
                    bg_color: errored_bg,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Tap to toggle Errored",
                    text_color: title_color
                ) [
                    ToggleErrored,
                    GestureHandler {
                        on_gesture: toggle_errored_handler,
                    },
                ] {}
                View (width: 16) {}
                View (
                    bg_color: disabled_bg,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Disabled (no events)",
                    text_color: title_color
                ) [
                    UserState::Disabled,
                ] {}
            }
        }
    };

    attach_to_parent(world, parent, root);
    root
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity) -> Entity
where
    B: Surface,
    F: RendererFactory<B>,
{
    build_widgets(&mut app.world, parent)
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
        let root = build_widgets(&mut world, parent);
        assert_ne!(root, parent);
        assert!(
            world
                .get::<Children>(parent)
                .is_some_and(|c| c.0.contains(&root)),
        );
    }
}
