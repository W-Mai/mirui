#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;
use crate::widget::UserState;
use crate::widget::dirty::Dirty;

pub struct ToggleErrored;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let hover_bg = Color::rgb(34, 74, 44);
    let errored_bg = Color::rgb(82, 38, 38);
    let disabled_bg = Color::rgb(34, 56, 86);
    let card_border = Color::rgb(48, 54, 61);
    let title_color = Color::rgb(201, 209, 217);

    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            padding: Padding {
                top: 28.into(),
                left: 32.into(),
                right: 32.into(),
                bottom: 28.into(),
            }
        ) {
            View (
                height: 36,
                text: "WidgetState: Hover / Press / Errored / Disabled",
                text_color: title_color
            )
            View (height: 16)
            Row (grow: 1.0) {
                View (
                    bg_color: hover_bg,
                    border_color: card_border,
                    border_width: 1,
                    grow: 1.0,
                    border_radius: 14,
                    padding: Padding::all(20),
                    text: "Hover me / Press me",
                    text_color: title_color
                )
                View (width: 16)
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
                ] on Tap {
                    if matches!(ctx.world.get::< UserState > (ctx.entity), Some(UserState::Errored)) {
                        ctx.world.remove::<UserState>(ctx.entity);
                    } else {
                        ctx.world.insert(ctx.entity, UserState::Errored);
                    }
                    ctx.world.insert(ctx.entity, Dirty);
                }
                View (width: 16)
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
                ]
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

    #[test]
    fn tap_toggles_errored_state() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);
        let column = world.get::<Children>(parent).unwrap().0[0];
        let row = world.get::<Children>(column).unwrap().0[2];
        let errored_card = world.get::<Children>(row).unwrap().0[2];

        assert!(world.get::<UserState>(errored_card).is_none());
        let h = world
            .get::<GestureHandler>(errored_card)
            .unwrap()
            .on_gesture;
        h(
            &mut world,
            errored_card,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: errored_card,
            },
        );
        assert!(matches!(
            world.get::<UserState>(errored_card),
            Some(UserState::Errored)
        ));
    }
}
