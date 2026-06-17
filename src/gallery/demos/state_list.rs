extern crate alloc;

#[cfg(feature = "std")]
use crate::app::{App, RendererFactory};
use crate::ecs::{Entity, World};
use crate::event::scroll::{ScrollAxis, ScrollConfig, ScrollOffset};
use crate::prelude::*;
use crate::state::Signal;
#[cfg(feature = "std")]
use crate::surface::Surface;

const ROW_H: i32 = 28;

#[derive(Clone)]
struct Fruit {
    name: &'static str,
    color: ColorToken,
}

const PALETTE: [Fruit; 6] = [
    Fruit {
        name: "Apple",
        color: ColorToken::Error,
    },
    Fruit {
        name: "Lime",
        color: ColorToken::Success,
    },
    Fruit {
        name: "Plum",
        color: ColorToken::Primary,
    },
    Fruit {
        name: "Mango",
        color: ColorToken::Secondary,
    },
    Fruit {
        name: "Berry",
        color: ColorToken::Tertiary,
    },
    Fruit {
        name: "Pear",
        color: ColorToken::SurfaceVariant,
    },
];

pub fn build_widgets(world: &mut World, parent: Entity) {
    let fruits = Signal::new(alloc::vec![
        PALETTE[0].clone(),
        PALETTE[1].clone(),
        PALETTE[2].clone(),
    ]);
    let (dec, inc, label) = (fruits.clone(), fruits.clone(), fruits.clone());
    let rows = fruits.clone();
    let content = fruits.clone();

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
            padding: Padding::all(12)
        ) {
            View (text: ${ alloc::format!("{} fruits", label.with(| f | f.len())) }, height: 32)
            Row (height: 52, padding: Padding::all(8)) {
                View (
                    bg_color: ColorToken::Error,
                    text_color: ColorToken::OnPrimary,
                    width: 48,
                    height: 36,
                    border_radius: 8,
                    text: "-"
                ) on Tap {
                    dec.update(|f| {
                        f.pop();
                    });
                }
                View (
                    bg_color: ColorToken::Primary,
                    text_color: ColorToken::OnPrimary,
                    width: 48,
                    height: 36,
                    border_radius: 8,
                    text: "+"
                ) on Tap {
                    inc.update(|f| {
                        let next = PALETTE[f.len() % PALETTE.len()].clone();
                        f.push(next);
                    });
                }
            }
            Column (
                id: "state_list_scroll",
                bg_color: ColorToken::Surface,
                grow: 1.0,
                width: 180,
                border_radius: 8,
                padding: Padding::all(4)
            ) [
                ScrollOffset {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                },
                ScrollConfig {
                    direction: ScrollAxis::Vertical,
                    elastic: true,
                    content_height: Fixed::from_int(3 * ROW_H),
                    content_width: Fixed::ZERO,
                },
            ] {
                walk ${ rows.get() } with fruit {
                    View (
                        bg_color: fruit.color,
                        text_color: ColorToken::OnPrimary,
                        width: 168,
                        height: 28,
                        border_radius: 6,
                        text: fruit.name
                    )
                }
            }
        }
    };
    //~focus-end

    let scroll =
        World::find_by_id(world, "state_list_scroll").expect("scroll container id registered");
    crate::state::with_world_scope(world, || {
        crate::state::effect_with_widget(scroll, move || {
            let n = content.with(|f| f.len()) as i32;
            crate::state::with_world(|w| {
                if let Some(cfg) = w.get_mut::<ScrollConfig>(scroll) {
                    cfg.content_height = Fixed::from_int(n * ROW_H);
                }
            });
        });
    });
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
    use crate::ui::builder::WidgetBuilder;

    fn row_count(world: &World, list: Entity) -> usize {
        world.get::<Children>(list).map(|c| c.0.len()).unwrap_or(0)
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
    fn list_grows_and_shrinks_by_signal() {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        let parent = WidgetBuilder::new(&mut world).id();
        build_widgets(&mut world, parent);

        let col = world.get::<Children>(parent).unwrap().0[0];
        let list = world.get::<Children>(col).unwrap().0[2];
        let row_ctrl = world.get::<Children>(col).unwrap().0[1];
        let dec = world.get::<Children>(row_ctrl).unwrap().0[0];
        let inc = world.get::<Children>(row_ctrl).unwrap().0[1];

        assert_eq!(
            world.get::<Children>(col).unwrap().0.len(),
            3,
            "column has label, button-row, list"
        );
        assert_eq!(
            world.get::<Children>(row_ctrl).unwrap().0.len(),
            2,
            "button row has - and + buttons"
        );

        assert_eq!(row_count(&world, list), 3, "starts with 3 rows");

        let content_h = |w: &World| {
            w.get::<crate::event::scroll::ScrollConfig>(list)
                .unwrap()
                .content_height
        };
        assert_eq!(
            content_h(&world),
            Fixed::from_int(3 * ROW_H),
            "scroll content height tracks 3 rows"
        );

        let first_before = world.get::<Children>(list).unwrap().0[0];
        tap(&mut world, inc);
        assert_eq!(row_count(&world, list), 4, "tap + appends one row");
        let first_after = world.get::<Children>(list).unwrap().0[0];
        assert_eq!(first_before, first_after, "surviving row keeps its entity");
        assert_eq!(
            content_h(&world),
            Fixed::from_int(4 * ROW_H),
            "scroll content height grows with rows"
        );

        tap(&mut world, dec);
        tap(&mut world, dec);
        assert_eq!(row_count(&world, list), 2, "two taps - drop two tail rows");
        assert_eq!(
            world.get::<Children>(list).unwrap().0[0],
            first_before,
            "row 0 survives shrink"
        );
        assert_eq!(
            content_h(&world),
            Fixed::from_int(2 * ROW_H),
            "scroll content height shrinks with rows"
        );
    }
}
