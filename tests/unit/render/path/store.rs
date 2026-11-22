use super::*;
use crate::core::reactive::flush_signal_dirty;
use crate::types::Point;
use crate::ui::dirty::Dirty;

const STATIC_COMMANDS: &[PathCmd] = &[
    PathCmd::MoveTo(Point::ZERO),
    PathCmd::LineTo(Point {
        x: crate::types::Fixed::from_int(10),
        y: crate::types::Fixed::ZERO,
    }),
];

#[test]
fn static_paths_remain_borrowed_and_immutable() {
    let mut store = PathStore::new(1).unwrap();
    let id = store.insert_static(STATIC_COMMANDS).unwrap();

    assert!(store.get(id).unwrap().is_borrowed());
    assert_eq!(store.revision(id), Ok(None));
    assert_eq!(
        store
            .edit(id, |path| {
                path.clear();
            })
            .unwrap_err(),
        PathStoreError::Immutable(id)
    );
}

#[test]
fn edit_advances_once_and_reuses_command_capacity() {
    let mut store = PathStore::new(1).unwrap();
    let mut path = Path::try_with_capacity(4).unwrap();
    path.move_to(Point::ZERO).line_to(Point::new(10, 0));
    let id = store.insert(path).unwrap();
    let capacity = store.get(id).unwrap().command_capacity();

    store
        .edit(id, |path| {
            path.set_command(0, PathCmd::MoveTo(Point::new(1, 2)))
                .unwrap();
            path.set_command(1, PathCmd::LineTo(Point::new(12, 2)))
                .unwrap();
        })
        .unwrap();

    assert_eq!(store.revision(id).unwrap().unwrap().get(), 1);
    assert_eq!(store.get(id).unwrap().command_capacity(), capacity);
}

#[test]
fn stale_ids_do_not_resolve_reused_slots() {
    let mut store = PathStore::new(1).unwrap();
    let old = store.insert(Path::new()).unwrap();
    store.remove(old).unwrap();
    let current = store.insert(Path::new()).unwrap();

    assert_ne!(old, current);
    assert_eq!(store.get(old).unwrap_err(), PathStoreError::Missing(old));
    assert!(store.get(current).is_ok());
}

#[test]
fn capacity_changes_preserve_slots_and_fail_without_mutation() {
    let mut store = PathStore::try_with_capacity(2).unwrap();
    let retired = store.insert_static(STATIC_COMMANDS).unwrap();
    assert_eq!(
        store.set_capacity_if_empty(usize::MAX),
        Err(PathStoreError::InUse { paths: 1 })
    );
    assert_eq!(store.capacity(), 2);
    assert_eq!(store.revision(retired), Ok(None));
    store.remove(retired).unwrap();
    assert_eq!(
        store.set_capacity_if_empty(usize::MAX),
        Err(PathStoreError::CapacityOverflow)
    );
    assert_eq!(store.capacity(), 2);
    let allocations = crate::test_support::tracked_allocations(|| {
        store.set_capacity_if_empty(0).unwrap();
        assert_eq!(
            store.insert_static(STATIC_COMMANDS),
            Err(PathStoreError::Full { capacity: 0 })
        );
        store.set_capacity_if_empty(1).unwrap();
        let current = store.insert_static(STATIC_COMMANDS).unwrap();
        assert_ne!(current, retired);
        assert_eq!(
            store.revision(retired),
            Err(PathStoreError::Missing(retired))
        );
        assert_eq!(store.capacity(), 1);
        assert_eq!(store.slots.capacity(), 2);
    });
    assert_eq!(allocations, 0);
}

#[test]
fn capacity_is_bounded_without_growth() {
    let mut store = PathStore::new(1).unwrap();
    store.insert(Path::new()).unwrap();

    assert_eq!(
        store.insert(Path::new()),
        Err(PathStoreError::Full { capacity: 1 })
    );
}

#[test]
fn mutable_path_edits_notify_only_while_subscribed() {
    let mut world = crate::ecs::World::new();
    let widget = world.spawn_empty();
    let mut store = PathStore::new(1).unwrap();
    let id = store.insert(Path::new()).unwrap();
    let subscription = store
        .subscribe(id, world.id(), widget, false)
        .unwrap()
        .unwrap();

    store
        .edit(id, |path| {
            path.move_to(Point::ZERO);
        })
        .unwrap();
    flush_signal_dirty(&mut world);
    assert!(world.remove::<Dirty>(widget).is_some());

    drop(subscription);
    store
        .edit(id, |path| {
            path.line_to(Point::ZERO);
        })
        .unwrap();
    flush_signal_dirty(&mut world);
    assert!(world.get::<Dirty>(widget).is_none());
}

#[test]
fn static_paths_need_no_subscription() {
    let mut world = crate::ecs::World::new();
    let widget = world.spawn_empty();
    let mut store = PathStore::new(1).unwrap();
    let id = store.insert_static(STATIC_COMMANDS).unwrap();

    assert!(
        store
            .subscribe(id, world.id(), widget, false)
            .unwrap()
            .is_none()
    );
}
#[test]
fn lazy_subscription_storage_has_the_same_size_as_an_optional_signal() {
    assert_eq!(
        core::mem::size_of::<OnceCell<Signal<PathRevision>>>(),
        core::mem::size_of::<Option<Signal<PathRevision>>>(),
    );
}
