use super::*;
use crate::test_support::tracked_allocations;

#[test]
fn typed_reads_preserve_type_and_entity_identity_without_allocating() {
    let mut world = World::new();
    let first = world.spawn_empty();
    world.insert(first, 17u16);
    world.insert(first, 29u32);
    world.insert_resource(41u16);
    world.insert_resource(53u32);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(world.get::<u16>(first), Some(&17));
            assert_eq!(world.get::<u32>(first), Some(&29));
            assert_eq!(world.get::<u64>(first), None);
            assert!(world.has::<u16>(first));
            assert!(world.has::<u32>(first));
            assert!(!world.has::<u64>(first));
            assert!(world.has_type(first, TypeId::of::<u16>()));
            assert!(!world.has_type(first, TypeId::of::<u64>()));
            assert!(world.has_any_by_id(TypeId::of::<u16>()));
            assert!(world.has_any_by_id(TypeId::of::<u32>()));
            assert!(!world.has_any_by_id(TypeId::of::<u64>()));
            assert_eq!(world.resource::<u16>(), Some(&41));
            assert_eq!(world.resource::<u32>(), Some(&53));
            assert_eq!(world.resource::<u64>(), None);
        }),
        0
    );

    assert!(world.despawn(first));
    assert!(!world.has::<u16>(first));
    assert!(!world.has_type(first, TypeId::of::<u16>()));
    assert!(!world.has_any_by_id(TypeId::of::<u16>()));
    assert!(!world.has_any_by_id(TypeId::of::<u32>()));
    let second = world.spawn_empty();
    assert_eq!(first.id, second.id);
    assert_ne!(first.generation, second.generation);
    world.insert(second, 61u16);
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(world.get::<u16>(first), None);
            assert!(!world.has::<u16>(first));
            assert!(!world.has_type(first, TypeId::of::<u16>()));
            assert_eq!(world.get::<u16>(second), Some(&61));
            assert!(world.has::<u16>(second));
            assert!(!world.has::<u32>(second));
            assert!(!world.has_type(second, TypeId::of::<u32>()));
            assert!(world.has_type(second, TypeId::of::<u16>()));
            assert!(world.has_any_by_id(TypeId::of::<u16>()));
            assert!(!world.has_any_by_id(TypeId::of::<u32>()));
            assert_eq!(world.resource::<u16>(), Some(&41));
            assert_eq!(world.resource::<u32>(), Some(&53));
        }),
        0
    );
}

#[test]
fn component_revisions_preserve_repeated_writes_without_entity_tracking() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.watch_component_revision::<u32>();
    world.insert(entity, 1u32);
    let revision = world.component_revision::<u32>();
    world.insert(entity, 1u32);
    assert_ne!(world.component_revision::<u32>(), revision);
    assert!(world.component_changes.is_empty());
    let revision = world.component_revision::<u32>();
    world.remove::<u32>(entity);
    assert_ne!(world.component_revision::<u32>(), revision);
    let revision = world.component_revision::<u32>();
    world.remove::<u32>(entity);
    assert_eq!(world.component_revision::<u32>(), revision);
}

#[test]
fn filtered_revisions_track_writes_before_queue_merging() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u32);
    world.watch_component_type(TypeId::of::<u32>());
    world.watch_filtered_component_revision::<u32>(|kind| {
        matches!(kind, ComponentChangeKind::Component)
    });
    world.drain_component_changes(|_, _| {});
    let initial = world.filtered_component_revision::<u32>();
    world.get_mut_with_change::<u32>(entity, ComponentChangeKind::Layout);
    assert_eq!(world.filtered_component_revision::<u32>(), initial);
    world.get_mut::<u32>(entity);
    let written = world.filtered_component_revision::<u32>();
    assert_ne!(written, initial);
    assert_eq!(world.component_changes.len(), 1);
    world.drain_component_changes(|_, _| {});
    assert_eq!(world.filtered_component_revision::<u32>(), written);
    world.despawn(entity);
    assert_ne!(world.filtered_component_revision::<u32>(), written);
}

#[test]
fn membership_lookup_hints_validate_types_and_follow_current_membership() {
    let mut world = World::new();
    let kind = TypeId::of::<u32>();
    assert_eq!(
        world.component_membership_revision_by_id(kind, 0),
        (usize::MAX, 0)
    );
    world.watch_component_revision::<u16>();
    world.watch_component_revision::<u32>();
    let (slot, initial) = world.component_membership_revision_by_id(kind, 0);
    assert_eq!(slot, 1, "an unrelated hint must be relocated");
    let entity = world.spawn_empty();
    world.insert(entity, 1u32);
    let inserted = world.component_membership_revision::<u32>();
    assert_ne!(inserted, initial);
    world.watch_component_revision::<[u8; 1]>();
    world.watch_component_revision::<[u8; 2]>();
    world.watch_component_revision::<[u8; 3]>();
    world.watch_component_revision::<[u8; 4]>();
    world.watch_component_revision::<[u8; 5]>();
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(
                world.component_membership_revision_by_id(kind, slot),
                (slot, inserted)
            );
        }),
        0
    );
    world.insert(entity, 2u32);
    assert_eq!(
        world.component_membership_revision_by_id(kind, slot),
        (slot, inserted)
    );
    world.remove::<u32>(entity);
    assert_eq!(
        world.component_membership_revision_by_id(kind, slot),
        (slot, world.component_membership_revision::<u32>())
    );
    world.insert(entity, 3u32);
    world.despawn(entity);
    assert_eq!(
        world.component_membership_revision_by_id(kind, slot),
        (slot, world.component_membership_revision::<u32>())
    );
    let mut other = World::new();
    other.watch_component_revision::<u32>();
    other.watch_component_revision::<u16>();
    assert_eq!(
        other.component_membership_revision_by_id(kind, slot),
        (0, 0)
    );
}

#[test]
fn membership_revision_tracks_presence_not_value_writes() {
    let mut world = World::new();
    world.watch_component_revision::<u32>();
    let entity = world.spawn_empty();
    world.insert(entity, 1u32);
    let member = world.component_membership_revision::<u32>();
    let value = world.component_revision::<u32>();
    world.insert(entity, 2u32);
    *world.get_mut::<u32>(entity).unwrap() = 3;
    assert_eq!(world.component_membership_revision::<u32>(), member);
    assert_ne!(world.component_revision::<u32>(), value);
    world.remove::<u32>(entity);
    let removed = world.component_membership_revision::<u32>();
    assert_ne!(removed, member);
    world.remove::<u32>(entity);
    assert_eq!(world.component_membership_revision::<u32>(), removed);
    world.insert(entity, 4u32);
    let inserted = world.component_membership_revision::<u32>();
    assert_ne!(removed, inserted);
    world.despawn(entity);
    let retired = world.component_membership_revision::<u32>();
    assert_ne!(retired, inserted);
    world.despawn(entity);
    assert_eq!(world.component_membership_revision::<u32>(), retired);
    let replacement = world.spawn_empty();
    world.insert(replacement, 5u32);
    assert_ne!(world.component_membership_revision::<u32>(), retired);
}
#[test]
fn query_change_causes_coalesce_without_entity_records_or_allocations() {
    use QueryDependencyCause::{Component, Source};

    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 0u16);
    world.watch_query_dependency_type(TypeId::of::<u16>());
    let allocations = tracked_allocations(|| {
        for (changes, expected) in [
            (&[Component][..], Component),
            (&[Source][..], Source),
            (&[Component, Source][..], Source),
            (&[Source, Component][..], Source),
            (&[Source, Source][..], Source),
        ] {
            for cause in changes {
                match cause {
                    Component => *world.get_mut::<u16>(entity).unwrap() += 1,
                    Source => world.invalidate_query_dependency::<u16>(),
                }
            }
            let mut calls = 0;
            world.drain_query_dependency_changes(|_, kind, cause| {
                assert_eq!(kind, TypeId::of::<u16>());
                assert_eq!(cause, expected);
                calls += 1;
            });
            assert_eq!(calls, 1);
            assert!(world.component_changes.is_empty());
            world.drain_query_dependency_changes(|_, _, _| panic!("already delivered"));
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(core::mem::size_of::<Option<QueryDependencyCause>>(), 1);
}

#[test]
fn query_source_changes_during_drain_wait_for_the_next_snapshot() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.watch_query_dependency_type(TypeId::of::<u16>());
    world.watch_query_dependency_type(TypeId::of::<u32>());
    world.insert(entity, 0u16);
    world.insert(entity, 0u32);
    let mut calls = 0;
    world.drain_query_dependency_changes(|world, _, cause| {
        assert_eq!(cause, QueryDependencyCause::Component);
        world.invalidate_query_dependency::<u16>();
        world.invalidate_query_dependency::<u32>();
        calls += 1;
    });
    assert_eq!(calls, 2);
    calls = 0;
    world.drain_query_dependency_changes(|world, _, cause| {
        assert_eq!(cause, QueryDependencyCause::Source);
        *world.get_mut::<u16>(entity).unwrap() += 1;
        calls += 1;
    });
    assert_eq!(calls, 2);
    calls = 0;
    world.drain_query_dependency_changes(|_, kind, cause| {
        assert_eq!(kind, TypeId::of::<u16>());
        assert_eq!(cause, QueryDependencyCause::Component);
        calls += 1;
    });
    assert_eq!(calls, 1);
}

#[test]
fn query_source_invalidation_survives_interrupted_delivery() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 0u16);
    world.watch_query_dependency_type(TypeId::of::<u16>());
    world.invalidate_query_dependency::<u16>();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.drain_query_dependency_changes(|world, _, cause| {
            assert_eq!(cause, QueryDependencyCause::Source);
            *world.get_mut::<u16>(entity).unwrap() += 1;
            panic!("dependency callback failed");
        });
    }));
    assert!(failed.is_err());
    let mut calls = 0;
    world.drain_query_dependency_changes(|_, _, cause| {
        assert_eq!(cause, QueryDependencyCause::Source);
        calls += 1;
    });
    assert_eq!(calls, 1);
    world.drain_query_dependency_changes(|_, _, _| panic!("already delivered"));
}

#[test]
fn query_dependencies_share_registration_with_late_entity_tracking() {
    let mut world = World::new();
    let kind = TypeId::of::<u16>();
    world.watch_query_dependency_type(kind);
    world.watch_query_dependency_type(kind);
    assert!(world.storages.is_empty());
    assert_eq!(world.watched_component_types.len(), 1);
    assert!(!world.watched_component_types[0].entity_tracking);
    assert!(world.watched_component_types[0].query_tracking);
    world.drain_query_dependency_changes(|_, _, _| panic!("registration is not a write"));

    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    assert!(world.component_changes.is_empty());
    let mut notifications = 0;
    world.drain_query_dependency_changes(|_, changed, _| {
        assert_eq!(changed, kind);
        notifications += 1;
    });
    assert_eq!(notifications, 1);

    world.watch_component_type(kind);
    world.watch_component_type(kind);
    assert_eq!(world.watched_component_types.len(), 1);
    assert!(world.watched_component_types[0].entity_tracking);
    assert!(world.watched_component_types[0].query_tracking);
    assert_eq!(
        world.component_changes,
        [ComponentChange {
            entity,
            type_id: kind,
            kind: ComponentChangeKind::Component,
        }]
    );
}

#[test]
fn get_mut_records_only_successful_watched_component_access() {
    let mut world = World::new();
    let first = world.spawn_empty();
    let absent = world.spawn_empty();
    let kind = TypeId::of::<u16>();
    world.watch_component_type(kind);
    world.watch_query_dependency_type(kind);
    assert!(world.get_mut::<u16>(first).is_none());
    assert!(world.storages.is_empty());
    assert!(world.component_changes.is_empty());
    world.drain_query_dependency_changes(|_, _, _| panic!("absent storage is not a write"));

    world.insert(first, 1u16);
    let retired = world.spawn_empty();
    world.insert(retired, 3u16);
    assert!(world.despawn(retired));
    let replacement = world.spawn_empty();
    assert_eq!(replacement.id, retired.id);
    assert_ne!(replacement.generation, retired.generation);
    world.insert(replacement, 4u16);
    world.drain_component_changes(|_, _| {});
    world.drain_query_dependency_changes(|_, _, _| {});
    assert!(world.get_mut::<u16>(absent).is_none());
    assert!(world.get_mut::<u16>(retired).is_none());
    assert_eq!(world.get::<u16>(first), Some(&1));
    assert_eq!(world.get::<u16>(replacement), Some(&4));
    assert!(world.component_changes.is_empty());
    world
        .drain_query_dependency_changes(|_, _, _| panic!("reads and absent values are not writes"));

    let changes = world.component_changes.as_ptr();
    let capacity = world.component_changes.capacity();
    let mut notifications = 0;
    let allocations = tracked_allocations(|| {
        for value in 2..66 {
            *world.get_mut::<u16>(first).unwrap() = value;
            *world.get_mut::<u16>(first).unwrap() = value + 1;
            assert_eq!(
                world.component_changes,
                [ComponentChange {
                    entity: first,
                    type_id: kind,
                    kind: ComponentChangeKind::Component,
                }]
            );
            world.drain_component_changes(|world, change| {
                assert_eq!(world.get::<u16>(change.entity), Some(&(value + 1)));
            });
            world.drain_query_dependency_changes(|_, changed, _| {
                assert_eq!(changed, kind);
                notifications += 1;
            });
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(notifications, 64);
    assert_eq!(world.component_changes.as_ptr(), changes);
    assert_eq!(world.component_changes.capacity(), capacity);
    assert!(world.component_changes.is_empty());
}

#[test]
fn structural_query_dependencies_coalesce_without_extra_entity_records() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let query_only = TypeId::of::<u16>();
    let both = TypeId::of::<u32>();
    world.watch_query_dependency_type(query_only);
    world.watch_query_dependency_type(both);
    world.watch_component_type(both);
    world.watch_component_type(TypeId::of::<bool>());
    world.insert(entity, 1u16);
    world.insert(entity, 2u16);
    world.insert(entity, 3u32);
    world.insert(entity, true);
    let mut changed = alloc::vec::Vec::new();
    world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
    assert_eq!(changed, [query_only, both]);
    assert_eq!(world.component_changes.len(), 2);
    world.drain_component_changes(|_, _| {});

    assert_eq!(world.remove::<u16>(entity), Some(2));
    assert!(world.remove::<u16>(entity).is_none());
    changed.clear();
    world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
    assert_eq!(changed, [query_only]);
    assert!(world.component_changes.is_empty());

    world.insert(entity, 4u16);
    world.drain_query_dependency_changes(|_, _, _| {});
    assert!(world.despawn(entity));
    assert!(!world.despawn(entity));
    changed.clear();
    world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
    assert_eq!(changed, [query_only, both]);
    assert_eq!(world.component_changes.len(), 2);
    assert!(
        world
            .component_changes
            .iter()
            .all(|change| change.entity == entity)
    );
    assert!(
        world
            .component_changes
            .iter()
            .all(|change| change.type_id != query_only)
    );
}

#[test]
fn query_dependency_drain_freezes_all_types_before_callbacks() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let first = TypeId::of::<u16>();
    let second = TypeId::of::<u32>();
    let added = TypeId::of::<bool>();
    world.watch_query_dependency_type(first);
    world.watch_query_dependency_type(second);
    world.insert(entity, 1u16);
    world.insert(entity, 2u32);

    let mut changed = alloc::vec::Vec::new();
    world.drain_query_dependency_changes(|world, kind, _| {
        changed.push(kind);
        if kind == first {
            *world.get_mut::<u16>(entity).unwrap() += 1;
            *world.get_mut::<u32>(entity).unwrap() += 1;
            world.watch_query_dependency_type(added);
            world.insert(entity, true);
        }
    });
    assert_eq!(changed, [first, second]);
    changed.clear();
    world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
    assert_eq!(changed, [first, second, added]);
    world.drain_query_dependency_changes(|_, _, _| panic!("snapshot was already consumed"));
}

#[test]
fn query_dependency_drain_retains_unfinished_snapshot_after_unwind() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let first = TypeId::of::<u16>();
    let second = TypeId::of::<u32>();
    world.watch_query_dependency_type(first);
    world.watch_query_dependency_type(second);
    world.insert(entity, 1u16);
    world.insert(entity, 2u32);
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.drain_query_dependency_changes(|world, kind, _| {
            assert_eq!(kind, first);
            *world.get_mut::<u16>(entity).unwrap() += 1;
            panic!("dependency callback failed");
        });
    }));
    assert!(failed.is_err());
    let mut changed = alloc::vec::Vec::new();
    world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
    assert_eq!(changed, [first, second]);
    world.drain_query_dependency_changes(|_, _, _| panic!("snapshot was already consumed"));
}

#[test]
fn erased_stable_iteration_uses_current_storage_without_allocating() {
    let mut world = World::new();
    world.for_each_stable_type(TypeId::of::<u16>(), |_, _| panic!("absent storage"));
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    for entity in [first, second] {
        world.insert(entity, 1u16);
        world.insert(entity, false);
    }
    let allocations = tracked_allocations(|| {
        world.for_each_stable_type(TypeId::of::<u16>(), |world, entity| {
            *world.get_mut::<u16>(entity).unwrap() += 1;
            *world.get_mut::<bool>(entity).unwrap() = true;
        });
    });
    assert_eq!(allocations, 0);
    assert_eq!(world.get::<u16>(first), Some(&2));
    assert_eq!(world.get::<u16>(second), Some(&2));
    assert_eq!(world.get::<bool>(first), Some(&true));
    assert_eq!(world.get::<bool>(second), Some(&true));
}

#[test]
#[should_panic(expected = "inserted or removed the iterated component")]
fn erased_stable_iteration_rejects_driver_structure_changes() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    world.for_each_stable_type(TypeId::of::<u16>(), |world, entity| {
        world.remove::<u16>(entity);
    });
}

#[test]
fn prepared_query_dependency_writes_and_first_drain_do_not_allocate() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    world.watch_query_dependency_type(TypeId::of::<u16>());
    let registry = world.watched_component_types.as_ptr();
    let registry_capacity = world.watched_component_types.capacity();
    let entity_capacity = world.component_changes.capacity();
    let mut notifications = 0;
    let allocations = tracked_allocations(|| {
        for _ in 0..64 {
            *world.get_mut::<u16>(entity).unwrap() += 1;
        }
        world.drain_query_dependency_changes(|world, kind, _| {
            assert_eq!(kind, TypeId::of::<u16>());
            world.for_each_stable_type(kind, |world, entity| {
                assert_eq!(world.get::<u16>(entity), Some(&65));
            });
            notifications += 1;
        });
    });
    assert_eq!(allocations, 0);
    assert_eq!(notifications, 1);
    assert_eq!(world.watched_component_types.as_ptr(), registry);
    assert_eq!(world.watched_component_types.capacity(), registry_capacity);
    assert_eq!(world.component_changes.capacity(), entity_capacity);
    assert!(world.component_changes.is_empty());
}

#[test]
fn lifetime_expires_before_resource_destructors() {
    struct CheckLifetime {
        lifetime: Weak<()>,
        observed: Rc<core::cell::Cell<bool>>,
    }

    impl Drop for CheckLifetime {
        fn drop(&mut self) {
            self.observed.set(self.lifetime.upgrade().is_none());
        }
    }

    let mut world = World::new();
    let observed = Rc::new(core::cell::Cell::new(false));
    world.insert_resource(CheckLifetime {
        lifetime: world.lifetime(),
        observed: Rc::clone(&observed),
    });

    drop(world);
    assert!(observed.get());
}

#[test]
fn absent_component_mutation_does_not_create_storage() {
    let mut world = World::new();
    let entity = world.spawn_empty();

    assert!(world.get_mut::<u16>(entity).is_none());
    assert!(world.remove::<u16>(entity).is_none());
    assert!(world.storages.is_empty());

    world.insert(entity, 7u16);
    assert_eq!(world.get_mut::<u16>(entity), Some(&mut 7));
    assert_eq!(world.remove::<u16>(entity), Some(7));
    assert!(world.get_mut::<u16>(entity).is_none());
    assert!(world.remove::<u16>(entity).is_none());
    assert_eq!(world.storages.len(), 1);
}

#[test]
fn stable_iteration_mutates_values_and_other_component_types() {
    let mut world = World::new();
    let first = world.spawn_empty();
    let second = world.spawn_empty();
    world.insert(first, 1u16);
    world.insert(second, 2u16);

    world.for_each_stable::<u16>(|world, entity| {
        *world.get_mut::<u16>(entity).unwrap() += 10;
        world.insert(entity, true);
    });

    assert_eq!(world.get::<u16>(first), Some(&11));
    assert_eq!(world.get::<u16>(second), Some(&12));
    assert_eq!(world.get::<bool>(first), Some(&true));
    assert_eq!(world.get::<bool>(second), Some(&true));
}

#[test]
#[should_panic(expected = "inserted or removed the iterated component")]
fn stable_iteration_rejects_structural_changes_to_the_iterated_type() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);

    world.for_each_stable::<u16>(|world, entity| {
        world.remove::<u16>(entity);
    });
}

#[test]
fn watched_component_changes_dedupe_replacements_and_removal() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    world.watch_component_type(TypeId::of::<u16>());
    world.component_changes.reserve(2);
    let capacity = world.component_changes.capacity();

    world.insert(entity, 2u16);
    world.insert(entity, 3u16);
    world.insert(entity, true);
    assert_eq!(world.remove::<u16>(entity), Some(3));
    let mut changes = alloc::vec::Vec::new();
    world.drain_component_changes(|world, change| {
        assert!(world.get::<u16>(change.entity).is_none());
        changes.push(change);
    });
    assert_eq!(
        changes,
        [ComponentChange {
            entity,
            type_id: TypeId::of::<u16>(),
            kind: ComponentChangeKind::Component,
        }]
    );
    assert_eq!(world.component_changes.capacity(), capacity);
    assert!(world.component_changes.is_empty());
}

#[test]
fn despawn_records_only_watched_component_types() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    world.insert(entity, 2u32);
    world.watch_component_type(TypeId::of::<u16>());
    world.component_changes.reserve(1);
    assert!(world.despawn(entity));
    assert!(!world.despawn(entity));
    let mut count = 0;
    world.drain_component_changes(|world, change| {
        assert_eq!(change.entity, entity);
        assert_eq!(change.type_id, TypeId::of::<u16>());
        assert!(!world.is_alive(entity));
        count += 1;
    });
    assert_eq!(count, 1);
}

#[test]
fn temporarily_separated_resources_retain_slots_without_allocating() {
    let mut world = World::new();
    world.insert_resource(7u16);
    world.insert_resource(9u32);
    let slots = world.resources.len();
    let address = world.resource::<u16>().unwrap() as *const u16;
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..128 {
                world
                    .with_resource_box::<u16, _>(|outer, world| {
                        assert!(world.resource::<u16>().is_none());
                        assert!(world.resource_mut::<u16>().is_none());
                        assert_eq!(world.resources.len(), slots);
                        assert!(world.take_resource_box::<u16>().is_none());
                        world
                            .with_resource_box::<u32, _>(|inner, world| {
                                assert!(world.resource::<u32>().is_none());
                                assert_eq!(world.resources.len(), slots);
                                *inner += 1;
                            })
                            .unwrap();
                        *outer += 1;
                    })
                    .unwrap();
            }
        }),
        0
    );
    assert_eq!(world.resource::<u16>(), Some(&135));
    assert_eq!(world.resource::<u32>(), Some(&137));
    assert_eq!(world.resource::<u16>().unwrap() as *const u16, address);
    assert_eq!(world.remove_resource::<u16>(), Some(135));
    assert!(!world.resources.contains_key(&TypeId::of::<u16>()));
}

#[test]
fn separated_resource_hides_from_typed_access_and_restores_over_replacements() {
    let mut world = World::new();
    world.insert_resource(7u16);
    world
        .with_resource_box::<u16, _>(|value, world| {
            let cell = SystemWorldCell::new(world);
            // SAFETY: no resource references exist while the original value is separated.
            unsafe {
                assert!(cell.resource::<u16>().is_none());
                assert!(cell.resource_mut::<u16>().is_none());
            }
            world.insert_resource(99u16);
            assert_eq!(world.resource::<u16>(), Some(&99));
            *value = 8;
        })
        .unwrap();
    assert_eq!(world.resource::<u16>(), Some(&8));
}

#[test]
fn separated_resource_restores_its_allocation_after_its_slot_is_removed() {
    for unwind in [false, true] {
        let mut world = World::new();
        world.insert_resource(7u16);
        let address = world.resource::<u16>().unwrap() as *const u16;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.with_resource_box::<u16, _>(|value, world| {
                assert_eq!(world.remove_resource::<u16>(), None);
                assert!(!world.resources.contains_key(&TypeId::of::<u16>()));
                *value += 1;
                assert!(!unwind, "callback failed");
            });
        }));
        assert_eq!(outcome.is_err(), unwind);
        assert_eq!(world.resource::<u16>(), Some(&8));
        assert_eq!(world.resource::<u16>().unwrap() as *const u16, address);
    }
}

#[test]
fn separated_resource_is_restored_after_callback_unwinds() {
    let mut world = World::new();
    world.insert_resource(7u16);
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        world.with_resource_box::<u16, _>(|value, world| {
            assert!(world.resource::<u16>().is_none());
            *value += 1;
            panic!("callback failed");
        });
    }));
    assert!(failure.is_err());
    assert_eq!(world.resource::<u16>(), Some(&8));
}

#[test]
fn replacing_resource_reuses_its_storage() {
    let mut world = World::new();
    world.insert_resource(7u16);
    let original = world.resource::<u16>().unwrap() as *const u16;

    world.insert_resource(8u16);

    assert_eq!(world.resource::<u16>(), Some(&8));
    assert_eq!(world.resource::<u16>().unwrap() as *const u16, original);
}
