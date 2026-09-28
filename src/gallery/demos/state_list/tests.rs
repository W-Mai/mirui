use super::composition::build_widgets;
use super::state::ROW_H;
use crate::core::reactive::flush_signal_dirty;
use crate::input::event::GestureHandler;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::*;
use crate::ui::Children;
use crate::ui::IdMap;
use crate::ui::UiScope;

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
    let mut cx = UiScope::new(&mut world, parent);
    build_widgets(&mut cx);
    drop(cx);

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
        w.get::<crate::input::event::scroll::ScrollConfig>(list)
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
