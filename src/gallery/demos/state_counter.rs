extern crate alloc;

use crate::prelude::*;
use crate::ui::UiScope;

#[ui_scope]
pub fn build_widgets() {
    let count = Signal::new(0i32);
    let (dec, inc, label) = (count.clone(), count.clone(), count.clone());

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(16)
        ) {
            View (text: ${ alloc::format!("Count: {}", label.get()) }, height: 40)
            Row (padding: Padding::all(8)) {
                View (
                    bg_color: Color::rgb(220, 80, 80),
                    width: 60,
                    height: 40,
                    border_radius: 8,
                    text: "-"
                ) on Tap { dec.update(|n| *n -= 1); }
                View (
                    bg_color: Color::rgb(63, 185, 80),
                    width: 60,
                    height: 40,
                    border_radius: 8,
                    text: "+"
                ) on Tap { inc.update(|n| *n += 1); }
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
    let mut cx = UiScope::new(&mut app.world, parent);
    build_widgets(&mut cx);
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
        t.resolve(world).into_owned()
    }

    #[test]
    fn tap_increments_and_reactive_text_updates() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let label = world.get::<Children>(col).unwrap().0[0];
        let row = world.get::<Children>(col).unwrap().0[1];
        let inc = world.get::<Children>(row).unwrap().0[1];

        assert_eq!(label_text(&world, label), "Count: 0");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: inc,
        };
        GestureHandler::trigger(&mut world, inc, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "Count: 1");

        GestureHandler::trigger(&mut world, inc, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "Count: 2");
    }
}
