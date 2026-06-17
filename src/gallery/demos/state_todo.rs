extern crate alloc;

use alloc::vec::Vec;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::core::reactive::{Computed, Signal};
use crate::ecs::{Entity, World};
use crate::prelude::*;
#[cfg(feature = "std")]
use crate::surface::Surface;

const ITEMS: &[&str] = &["buy milk", "write docs", "ship release"];

fn row_color(done: bool) -> Color {
    if done {
        Color::rgb(63, 185, 80)
    } else {
        Color::rgb(80, 80, 96)
    }
}

pub fn build_widgets(world: &mut World, parent: Entity) {
    let dones: Vec<Signal<bool>> = ITEMS.iter().map(|_| Signal::new(false)).collect();
    let remaining = {
        let dones = dones.clone();
        Computed::new(move || dones.iter().filter(|d| !d.get()).count() as i32)
    };
    let summary = remaining.clone();

    let root = WidgetBuilder::new(world)
        .layout(crate::ui::layout::LayoutStyle {
            direction: FlexDirection::Column,
            align: AlignItems::Center,
            padding: Padding::all(12),
            grow: Fixed::ONE,
            ..Default::default()
        })
        .id();
    world.insert(root, crate::ui::Parent(parent));
    if let Some(c) = world.get_mut::<crate::ui::Children>(parent) {
        c.0.push(root);
    }

    let _ = ui! {
        :(
            parent: root
            world: world
        :)

        summary_label (
            text: ${ alloc::format!("{} remaining", summary.get()) },
            height: 32
        )
    };

    for (i, label) in ITEMS.iter().enumerate() {
        let toggle = dones[i].clone();
        let bg = dones[i].clone();
        let text = *label;
        ui! {
            :(
                parent: root
                world: world
            :)

            row (
                bg_color: ${ row_color(bg.get()) },
                width: 220,
                height: 32,
                border_radius: 6,
                text: text
            ) on Tap {
                toggle.update(|d| *d = !*d);
            }
        };
    }
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
    use crate::core::reactive::flush_signal_dirty;
    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::widgets::text::Text;

    fn label_text(world: &World, label: Entity) -> alloc::string::String {
        let t = world.get::<Text>(label).expect("label has Text");
        alloc::string::String::from_utf8(t.0.clone()).unwrap()
    }

    #[test]
    fn toggling_a_row_updates_remaining_count() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let root = world.get::<Children>(parent).unwrap().0[0];
        let kids = world.get::<Children>(root).unwrap().0.clone();
        let summary = kids[0];
        let first_row = kids[1];

        assert_eq!(label_text(&world, summary), "3 remaining");

        GestureHandler::trigger(
            &mut world,
            first_row,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: first_row,
            },
        );
        flush_signal_dirty(&mut world);
        assert_eq!(label_text(&world, summary), "2 remaining");
    }
}
