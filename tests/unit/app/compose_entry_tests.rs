use super::*;
use crate::core::reactive::current_world_id;

#[test]
fn returns_entity_and_owned_results_after_preparing_dirty_storage() {
    let mut app = App::headless(8, 8);
    let root = app.spawn_root().id();
    let payload = alloc::vec![2_u8, 3, 5];
    let output = app.compose(root, |cx| {
        assert_eq!(current_world_id(), Some(cx.world_mut().id()));
        (cx.world_mut().spawn_empty(), payload)
    });

    assert!(app.world.is_alive(output.0));
    assert_eq!(output.1, [2, 3, 5]);
    assert_eq!(current_world_id(), None);
    let capacity = app
        .world
        .storage::<crate::ui::dirty::Dirty>()
        .unwrap()
        .reserved_entity_capacity();
    assert!(capacity.0 >= app.world.allocated_entity_slots() as usize);
    assert!(capacity.1 >= app.world.live_entity_count());
}

#[test]
fn restores_enclosing_owner_after_return_and_unwind() {
    let mut outer = App::headless(8, 8);
    let outer_root = outer.spawn_root().id();
    let mut inner = App::headless(8, 8);
    let inner_root = inner.spawn_root().id();
    let outer_id = outer.world.id();
    let inner_id = inner.world.id();

    outer.compose(outer_root, |_cx| {
        assert_eq!(current_world_id(), Some(outer_id));
        assert_eq!(
            inner.compose(inner_root, |_cx| current_world_id()),
            Some(inner_id)
        );
        assert_eq!(current_world_id(), Some(outer_id));
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.compose(inner_root, |_cx| panic!("composition unwind"));
        }));
        assert!(failure.is_err());
        assert_eq!(current_world_id(), Some(outer_id));
    });
    assert_eq!(current_world_id(), None);
}

#[test]
fn repeated_invocation_adds_no_wrapper_allocations() {
    let mut app = App::headless(8, 8);
    let root = app.spawn_root().id();
    app.compose(root, |_cx| ());
    let allocations = crate::test_support::tracked_allocations(|| {
        for value in 0..64 {
            assert_eq!(app.compose(root, |_cx| value), value);
        }
    });
    assert_eq!(allocations, 0);
}
