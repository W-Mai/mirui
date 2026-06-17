extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
use crate::state::{Computed, Signal};
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let name_filled = Signal::new(false);
    let agreed = Signal::new(false);
    let can_submit = {
        let (name_filled, agreed) = (name_filled.clone(), agreed.clone());
        Computed::new(move || name_filled.get() && agreed.get())
    };

    let toggle_name = name_filled.clone();
    let toggle_agree = agreed.clone();
    let name_bg = name_filled.clone();
    let agree_bg = agreed.clone();
    let submit_bg = can_submit.clone();

    fn on_off(on: bool) -> Color {
        if on {
            Color::rgb(63, 185, 80)
        } else {
            Color::rgb(80, 80, 96)
        }
    }

    //~focus-start
    ui! {
        :(
            parent: parent
            world: world
        :)

        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(16)
        ) {
            View (
                bg_color: ${ on_off(name_bg.get()) },
                width: 200,
                height: 36,
                border_radius: 6,
                text: "name (tap to fill)"
            ) on Tap {
                toggle_name.update(|v| *v = !*v);
            }
            View (
                bg_color: ${ on_off(agree_bg.get()) },
                width: 200,
                height: 36,
                border_radius: 6,
                text: "agree (tap to toggle)"
            ) on Tap {
                toggle_agree.update(|v| *v = !*v);
            }
            View (
                bg_color: ${ on_off(submit_bg.get()) },
                width: 200,
                height: 40,
                border_radius: 8,
                text: "Submit"
            )
        }
    };
    //~focus-end
}

#[cfg(feature = "std")]
pub fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;
    use crate::state::flush_signal_dirty;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::Style;
    use crate::ui::builder::WidgetBuilder;

    fn bg(world: &World, e: Entity) -> Option<crate::ui::theme::ThemedColor> {
        world.get::<Style>(e).and_then(|s| s.bg_color)
    }

    fn tap(world: &mut World, e: Entity) {
        GestureHandler::trigger(
            world,
            e,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
        );
        flush_signal_dirty(world);
    }

    #[test]
    fn submit_lights_up_only_when_both_set() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let name = world.get::<Children>(col).unwrap().0[0];
        let agree = world.get::<Children>(col).unwrap().0[1];
        let submit = world.get::<Children>(col).unwrap().0[2];

        let off = crate::ui::theme::ThemedColor::Raw(Color::rgb(80, 80, 96));
        let on = crate::ui::theme::ThemedColor::Raw(Color::rgb(63, 185, 80));

        assert_eq!(bg(&world, submit), Some(off), "starts disabled");
        tap(&mut world, name);
        assert_eq!(bg(&world, submit), Some(off), "name alone is not enough");
        tap(&mut world, agree);
        assert_eq!(bg(&world, submit), Some(on), "both set -> submit enabled");
    }
}
