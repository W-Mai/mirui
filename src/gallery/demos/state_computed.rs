extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::prelude::*;
use crate::state::{Computed, Signal};
#[cfg(feature = "std")]
use crate::surface::Surface;

pub fn build_widgets(world: &mut World, parent: Entity) {
    let n = Signal::new(2i32);
    let squared = {
        let n = n.clone();
        Computed::new(move || n.get() * n.get())
    };
    let (inc, label) = (n.clone(), squared.clone());

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
            View (text: ${ alloc::format!("n^2 = {}", label.get()) }, height: 40)
            View (
                bg_color: Color::rgb(63, 185, 80),
                width: 120,
                height: 40,
                border_radius: 8,
                text: "n + 1"
            ) on Tap {
                inc.update(|v| *v += 1);
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
    use crate::plugins::StdInstantClockPlugin;
    app.add_plugin(StdInstantClockPlugin);
    build_widgets(&mut app.world, parent);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::text::Text;
    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;
    use crate::state::flush_signal_dirty;
    use crate::widget::Children;
    use crate::widget::IdMap;
    use crate::widget::builder::WidgetBuilder;

    fn label_text(world: &World, label: Entity) -> alloc::string::String {
        let t = world.get::<Text>(label).expect("label has Text");
        alloc::string::String::from_utf8(t.0.clone()).unwrap()
    }

    #[test]
    fn computed_label_tracks_source() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let label = world.get::<Children>(col).unwrap().0[0];
        let btn = world.get::<Children>(col).unwrap().0[1];

        assert_eq!(label_text(&world, label), "n^2 = 4");

        let tap = GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: btn,
        };
        GestureHandler::trigger(&mut world, btn, &tap);
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, label), "n^2 = 9");
    }
}
