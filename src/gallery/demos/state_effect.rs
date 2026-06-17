extern crate alloc;

use crate::core::reactive::{Effect, Signal};
use crate::prelude::*;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let value = Signal::new(0i32);
    // an effect with a side effect: count how many times `value` changed
    let changes = Signal::new(0i32);
    {
        let (value, changes) = (value.clone(), changes.clone());
        core::mem::forget(Effect::new(move || {
            let _ = value.get();
            changes.update(|c| *c += 1);
        }));
    }
    let (bump, vlabel, clabel) = (value.clone(), value.clone(), changes.clone());

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
            View (text: ${ alloc::format!("value = {}", vlabel.get()) }, height: 32)
            View (text: ${ alloc::format!("effect ran {} times", clabel.get()) }, height: 32)
            View (
                bg_color: Color::rgb(88, 166, 255),
                width: 120,
                height: 40,
                border_radius: 8,
                text: "bump"
            ) on Tap {
                bump.update(|v| *v += 1);
            }
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
    use crate::app::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::widgets::text::Text;

    fn label_text(world: &World, label: Entity) -> alloc::string::String {
        let t = world.get::<Text>(label).expect("label has Text");
        alloc::string::String::from_utf8(t.0.clone()).unwrap()
    }

    #[test]
    fn effect_counts_each_source_change() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let runs_label = world.get::<Children>(col).unwrap().0[1];
        let btn = world.get::<Children>(col).unwrap().0[2];

        // effect ran once at creation
        assert_eq!(label_text(&world, runs_label), "effect ran 1 times");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: btn,
        };
        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, runs_label), "effect ran 2 times");
    }
}
