extern crate alloc;

use crate::prelude::*;
use crate::ui::widgets::{Button, Text};

use alloc::vec::Vec;

#[derive(Clone)]
struct Card {
    id: u32,
    name: &'static str,
    color: ColorToken,
}

fn deck() -> Vec<Card> {
    alloc::vec![
        Card {
            id: 0,
            name: "Red",
            color: ColorToken::Error
        },
        Card {
            id: 1,
            name: "Green",
            color: ColorToken::Success
        },
        Card {
            id: 2,
            name: "Blue",
            color: ColorToken::Primary
        },
        Card {
            id: 3,
            name: "Gold",
            color: ColorToken::Secondary
        },
    ]
}

#[compose]
pub fn build_widgets() {
    let cards = Signal::new(deck());
    let rotate = cards.clone();
    let rows = cards.clone();

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 12
        ) {
            Button (
                text_color: ColorToken::OnPrimary,
                width: Dimension::percent(100),
                max_width: 260,
                height: 42,
                border_radius: 11,
                normal_color: ColorToken::Primary,
                pressed_color: ColorToken::Secondary
            ) [
                Text::label("Rotate keyed rows"),
            ] on Tap {
                rotate
                    .update(|c| {
                        if !c.is_empty() {
                            let head = c.remove(0);
                            c.push(head);
                        }
                    });
            }
            Column (
                id: "state_keyed_list",
                bg_color: ColorToken::SurfaceVariant,
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 260,
                max_height: 160,
                border_radius: 12,
                padding: Padding::all(6),
                row_gap: 6
            ) {
                walk ${ rows.get() } with card by card.id {
                    View (
                        bg_color: card.color,
                        text_color: ColorToken::OnPrimary,
                        width: Dimension::percent(100),
                        height: 30,
                        border_radius: 8
                    ) [
                        Text::label(card.name),
                    ]
                }
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
    app.compose(parent, build_widgets);
}

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 320);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::flush_signal_dirty;
    use crate::input::event::GestureHandler;
    use crate::input::event::gesture::GestureEvent;
    use crate::ui::Children;
    use crate::ui::IdMap;
    use crate::ui::UiScope;

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
    fn rotate_keeps_widget_identity_per_key() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        let mut cx = UiScope::new(&mut world, parent);
        build_widgets(&mut cx);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let rotate_btn = world.get::<Children>(col).unwrap().0[0];
        let list = world.get::<Children>(col).unwrap().0[1];

        let before = world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(before.len(), 4, "four cards");
        let row_red = before[0];
        let row_green = before[1];

        // rotate: [Red,Green,Blue,Gold] -> [Green,Blue,Gold,Red]
        tap(&mut world, rotate_btn);

        let after = world.get::<Children>(list).unwrap().0.clone();
        assert_eq!(after.len(), 4, "still four cards");
        // Green moved to front but is the SAME entity (keyed identity)
        assert_eq!(
            after[0], row_green,
            "Green row keeps its entity after rotate"
        );
        // Red moved to the tail, also the same entity
        assert_eq!(after[3], row_red, "Red row keeps its entity, now at tail");
        assert!(
            before.iter().all(|e| after.contains(e)),
            "rotate reuses all rows, builds none"
        );
    }
}
