use super::composition::build_widgets;
use crate::core::reactive::flush_signal_dirty;
use crate::ecs::World;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::scroll::{ScrollBounds, ScrollConfig};
use crate::prelude::*;
use crate::types::Viewport;
use crate::ui::render_system::try_update_layout;
use crate::ui::{Children, ComputedRect};

fn row_count(world: &World, list: Entity) -> usize {
    world.get::<Children>(list).map(|c| c.0.len()).unwrap_or(0)
}

fn tap(world: &mut World, e: Entity) {
    {
        let decision = crate::__macro_support::dispatch_entity_gesture(
            world,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
            0,
        )
        .unwrap();
        crate::__macro_support::flush_entity_events(world).unwrap();
        assert_eq!(decision, crate::input::event::BubbleControl::Prevent);
    };
    flush_signal_dirty(world);
}

#[test]
fn list_grows_and_shrinks_by_signal() {
    let mut app = App::headless(320, 320);
    app.with_default_widgets();
    let parent = app.spawn_root().id();
    app.compose(parent, build_widgets());
    let mut world = app.world;

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

    let first_before = world.get::<Children>(list).unwrap().0[0];
    tap(&mut world, inc);
    assert_eq!(row_count(&world, list), 4, "tap + appends one row");
    let first_after = world.get::<Children>(list).unwrap().0[0];
    assert_eq!(first_before, first_after, "surviving row keeps its entity");
    for _ in 0..8 {
        tap(&mut world, inc);
    }
    try_update_layout(&mut world, parent, &Viewport::new(320, 320, Fixed::ONE)).unwrap();
    let bounds = world.get::<ScrollBounds>(list).unwrap();
    let last = *world.get::<Children>(list).unwrap().0.last().unwrap();
    let last_rect = world.get::<ComputedRect>(last).unwrap().0;
    let list_rect = world.get::<ComputedRect>(list).unwrap().0;
    assert!(bounds.max_y > Fixed::ZERO, "overflowing rows can scroll");
    assert_eq!(
        bounds.content_height,
        last_rect.y + last_rect.h - list_rect.y,
        "scroll bounds include row spacing and leading padding"
    );
    assert_eq!(
        world.get::<ScrollConfig>(list).unwrap().content_height,
        Fixed::ZERO
    );
    assert!(world.get::<Style>(list).unwrap().clip_children);

    for _ in 0..10 {
        tap(&mut world, dec);
    }
    assert_eq!(row_count(&world, list), 2, "remove tail rows");
    assert_eq!(
        world.get::<Children>(list).unwrap().0[0],
        first_before,
        "row 0 survives shrink"
    );
    try_update_layout(&mut world, parent, &Viewport::new(320, 320, Fixed::ONE)).unwrap();
    assert_eq!(world.get::<ScrollBounds>(list).unwrap().max_y, Fixed::ZERO);
    tap(&mut world, dec);
    tap(&mut world, dec);
    try_update_layout(&mut world, parent, &Viewport::new(320, 320, Fixed::ONE)).unwrap();
    assert_eq!(row_count(&world, list), 0);
    assert_eq!(world.get::<ScrollBounds>(list).unwrap().max_y, Fixed::ZERO);
}
