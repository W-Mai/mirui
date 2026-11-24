use super::*;

#[test]
fn equal_positions_keep_input_ownership_and_other_targets_are_not_cancelled() {
    let mut app = crate::app::App::headless(64, 64);
    let first = app.world.spawn_empty();
    let second = app.world.spawn_empty();
    app.world.insert(first, ScrollOffset::default());
    app.world.insert(second, ScrollOffset::default());
    app.world
        .resource_mut::<ScrollMotion>()
        .unwrap()
        .begin_pointer(first, 0);
    let drag = app.world.resource_mut::<ScrollDragState>().unwrap();
    drag.active = true;
    drag.resolved = true;
    drag.target = first;

    assert!(!set_position(
        &mut app.world,
        first,
        ScrollOffset::default()
    ));
    assert_eq!(
        app.world.resource::<ScrollMotion>().unwrap().target_entity,
        Some(first)
    );
    assert!(app.world.resource::<ScrollDragState>().unwrap().active);

    assert!(set_position(
        &mut app.world,
        second,
        ScrollOffset {
            x: 1.into(),
            y: 2.into()
        }
    ));
    assert_eq!(
        app.world.resource::<ScrollMotion>().unwrap().target_entity,
        Some(first)
    );
    assert!(app.world.resource::<ScrollDragState>().unwrap().active);

    assert!(set_position(
        &mut app.world,
        first,
        ScrollOffset {
            x: 3.into(),
            y: 4.into()
        }
    ));
    assert!(
        app.world
            .resource::<ScrollMotion>()
            .unwrap()
            .target_entity
            .is_none()
    );
    assert!(!app.world.resource::<ScrollDragState>().unwrap().active);
}
