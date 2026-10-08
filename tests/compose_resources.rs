use mirui::prelude::*;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

struct Counter(u32);
struct Missing;
struct MoveOnly(u8);

#[compose]
fn resource_operations(seed: u32) -> (u32, bool, u32) {
    res!().insert(Counter(seed));
    let initial = res!(Counter).with(|counter| counter.0);
    let missing = res!(optional Missing).with(|value| value.is_none());
    res!(mut Counter).with(|counter| counter.0 += 1);
    let removed = res!().remove::<Counter>().expect("installed counter");
    let final_value = removed.0;
    res!().insert(removed);
    (initial, missing, final_value)
}

#[compose]
fn move_resource<T: 'static>(value: T) -> Option<T> {
    res!().insert(value);
    res!().remove::<T>()
}

#[compose]
fn update_existing_resource() -> u32 {
    res!(mut Counter).with(|counter| counter.0 += 1);
    let value = res!(Counter).with(|counter| counter.0);
    res!().insert(Counter(value));
    let optional = res!(optional Counter).with(|counter| counter.map(|counter| counter.0));
    optional.expect("installed counter")
}

#[compose]
fn helper_closure() -> u32 {
    let mut update = || res!(mut Counter).with(|counter| counter.0 += 2);
    update();
    res!(Counter).with(|counter| counter.0)
}

#[test]
fn resource_operations_use_the_installed_storage_immediately() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    assert_eq!(app.compose(root, resource_operations(7)), (7, true, 8));
    assert_eq!(app.world.resource::<Counter>().unwrap().0, 8);
    assert_eq!(app.world.resource::<Missing>().map(|_| ()), None);
}

#[test]
fn generic_resources_move_without_clone_requirements() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        let moved = move_resource(MoveOnly(19))(cx).expect("owned value");
        assert_eq!(moved.0, 19);
        assert!(move_resource::<Counter>(Counter(3))(cx).is_some());
    });
    assert!(app.world.resource::<MoveOnly>().is_none());
    assert!(app.world.resource::<Counter>().is_none());
}

#[test]
fn ordinary_helper_closures_borrow_the_same_composition_scope() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.add_resource(Counter(5));
    assert_eq!(app.compose(root, helper_closure()), 7);
    assert_eq!(app.world.resource::<Counter>().unwrap().0, 7);
}

#[test]
fn prepared_resource_access_and_replacement_do_not_allocate() {
    let mut app = App::headless(32, 32);
    let root = app.spawn_root().id();
    app.add_resource(Counter(0));
    app.compose(root, |_| {});
    for expected in 1..=2 {
        assert_eq!(
            tracked_allocations(|| {
                assert_eq!(app.compose(root, update_existing_resource()), expected);
            }),
            0,
        );
    }
}

#[test]
fn distinct_apps_keep_resource_values_independent() {
    let mut first = App::headless(32, 32);
    let mut second = App::headless(32, 32);
    let first_root = first.spawn_root().id();
    let second_root = second.spawn_root().id();
    assert_eq!(first.compose(first_root, resource_operations(2)).2, 3);
    assert_eq!(second.compose(second_root, resource_operations(9)).2, 10);
    assert_eq!(first.world.resource::<Counter>().unwrap().0, 3);
    assert_eq!(second.world.resource::<Counter>().unwrap().0, 10);
}
