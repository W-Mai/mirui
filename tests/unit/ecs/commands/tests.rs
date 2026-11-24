use alloc::rc::Rc;
use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::*;
use crate::test_support::tracked_allocations;

#[derive(Debug, PartialEq, Eq)]
struct Marker(u32);

impl Component for Marker {}

struct Resource(u32);

struct ReplaceMarker {
    entity: Entity,
    value: u32,
}

impl DeferredOperation for ReplaceMarker {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError> {
        let marker =
            world
                .get_mut::<Marker>(self.entity)
                .ok_or(CommandCommitError::MissingTarget {
                    entity: self.entity,
                })?;
        marker.0 = self.value;
        Ok(())
    }
}

#[test]
fn bounded_extension_command_shares_order_capacity_and_commit_errors() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let removed = world.spawn_empty();
    world.despawn(removed);
    world.insert(entity, Marker(0));
    let capacity = CommandCapacity::new()
        .insert::<Marker>(2)
        .enqueue::<ReplaceMarker>(1);
    let mut queue = CommandQueue::bounded(capacity).unwrap();
    let allocations = tracked_allocations(|| {
        let mut batch = queue.begin();
        batch.commands().insert(entity, Marker(1));
        batch.commands().enqueue(ReplaceMarker { entity, value: 2 });
        assert_eq!(world.get::<Marker>(entity), Some(&Marker(0)));
        batch.finish(&mut world).unwrap();
        assert_eq!(world.get::<Marker>(entity), Some(&Marker(2)));

        let mut batch = queue.begin();
        batch.commands().insert(entity, Marker(3));
        batch.commands().enqueue(ReplaceMarker {
            entity: removed,
            value: 4,
        });
        batch.commands().insert(entity, Marker(5));
        assert_eq!(
            batch.finish(&mut world),
            Err(CommandError::Commit(CommandCommitError::MissingTarget {
                entity: removed
            }))
        );
        assert_eq!(world.get::<Marker>(entity), Some(&Marker(3)));
    });
    assert_eq!(allocations, 0);
}

struct DropMarker(Rc<Cell<u32>>);

impl Component for DropMarker {}

impl Drop for DropMarker {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[repr(align(64))]
struct OverAligned([u8; 160]);

impl Component for OverAligned {}

#[derive(Debug, PartialEq, Eq)]
struct Small([u8; 3]);

impl Component for Small {}

#[derive(Debug, PartialEq, Eq)]
#[repr(align(8))]
struct Medium(u64);

impl Component for Medium {}

#[repr(align(64))]
struct UnitNotification;

impl crate::input::event::contract::Event for UnitNotification {
    type Route = crate::input::event::contract::EntityNotification;
    type Source = crate::input::event::contract::WidgetSource<Marker>;
    type Options = ();
}

static ZST_DROPS: AtomicUsize = AtomicUsize::new(0);

struct DropZst;

impl Component for DropZst {}

impl Drop for DropZst {
    fn drop(&mut self) {
        ZST_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn command_batch_applies_in_source_order_after_the_body() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Marker(0));
    let mut queue = CommandQueue::default();

    let mut batch = queue.begin();
    {
        let mut commands = batch.commands();
        commands.insert(entity, Marker(1));
        commands.insert(entity, Marker(2));
        commands.apply_resource(Resource(7), |world, resource| {
            world.insert_resource(resource);
        });
    }
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(0)));
    assert!(world.resource::<Resource>().is_none());
    batch.finish(&mut world).unwrap();

    assert_eq!(world.get::<Marker>(entity), Some(&Marker(2)));
    assert_eq!(world.resource::<Resource>().map(|value| value.0), Some(7));
}

#[test]
fn unfinished_batch_drops_owned_payload_once() {
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::default();
    {
        let mut batch = queue.begin();
        batch.commands().insert(
            Entity {
                id: 0,
                generation: 0,
            },
            DropMarker(drops.clone()),
        );
    }
    assert_eq!(drops.get(), 1);
}

#[test]
fn warmed_command_batch_does_not_allocate() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Marker(0));
    world.insert_resource(Resource(0));
    let mut queue = CommandQueue::default();

    for value in [1, 2] {
        let mut batch = queue.begin();
        let allocations = tracked_allocations(|| {
            let mut commands = batch.commands();
            commands.insert(entity, Marker(value));
            commands.apply_resource(Resource(value), |world, resource| {
                world.insert_resource(resource);
            });
        });
        batch.finish(&mut world).unwrap();
        if value == 2 {
            assert_eq!(allocations, 0);
        }
    }
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(2)));
    assert_eq!(world.resource::<Resource>().map(|value| value.0), Some(2));
}

#[test]
fn removal_and_despawn_commands_are_deferred() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Marker(1));
    world.insert_resource(Resource(1));
    let mut queue = CommandQueue::default();

    let mut batch = queue.begin();
    {
        let mut commands = batch.commands();
        commands.remove::<Marker>(entity);
        commands.remove_resource::<Resource>();
        commands.despawn(entity);
    }
    assert!(world.is_alive(entity));
    assert!(world.get::<Marker>(entity).is_some());
    batch.finish(&mut world).unwrap();

    assert!(!world.is_alive(entity));
    assert!(world.resource::<Resource>().is_none());
}

#[test]
fn arena_realigns_without_corrupting_earlier_payloads() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let mut queue = CommandQueue::default();

    let mut batch = queue.begin();
    {
        let mut commands = batch.commands();
        commands.insert(entity, Marker(41));
        commands.insert(entity, OverAligned([7; 160]));
    }
    batch.finish(&mut world).unwrap();

    assert_eq!(world.get::<Marker>(entity), Some(&Marker(41)));
    assert_eq!(world.get::<OverAligned>(entity).unwrap().0, [7; 160]);
}

#[test]
fn unwinding_callback_discards_zero_sized_payload_once() {
    ZST_DROPS.store(0, Ordering::Relaxed);
    let mut queue = CommandQueue::default();

    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut batch = queue.begin();
        batch.commands().insert(
            Entity {
                id: 0,
                generation: 0,
            },
            DropZst,
        );
        panic!("callback failed");
    }));

    assert!(result.is_err());
    assert_eq!(ZST_DROPS.load(Ordering::Relaxed), 1);
}

#[test]
fn apply_unwind_drops_remaining_payloads_once() {
    unsafe fn panic_after_read(
        _world: &mut World,
        _target: Option<Entity>,
        payload: *mut u8,
        _event: Option<&dyn Any>,
    ) -> Result<(), CommandError> {
        // SAFETY: this function is paired with a stored DropMarker.
        let value = unsafe { read_stored::<DropMarker>(payload) };
        drop(value);
        panic!("apply failed");
    }

    let drops = Rc::new(Cell::new(0));
    let mut world = World::new();
    let mut queue = CommandQueue::default();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let batch = queue.begin();
        batch
            .queue
            .push_payload(None, DropMarker(drops.clone()), panic_after_read);
        batch.queue.push_payload(
            Some(Entity {
                id: 0,
                generation: 0,
            }),
            DropMarker(drops.clone()),
            apply_insert::<DropMarker>,
        );
        batch.finish(&mut world).unwrap();
    }));

    assert!(result.is_err());
    assert_eq!(drops.get(), 2);

    let entity = world.spawn_empty();
    world.insert(entity, Marker(0));
    let allocations = tracked_allocations(|| {
        let mut batch = queue.begin();
        batch.commands().insert(entity, Marker(9));
        batch.finish(&mut world).unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(9)));
}

#[test]
fn bounded_batch_records_and_commits_without_first_call_growth() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Marker(0));
    let mut queue = CommandQueue::bounded(CommandCapacity::new().insert::<Marker>(2)).unwrap();
    let records = queue.records.capacity();
    let bytes = queue.payload.capacity();
    assert_eq!(
        tracked_allocations(|| {
            for value in 1..=32 {
                let mut batch = queue.begin();
                batch.commands().insert(entity, Marker(value));
                batch.commands().insert(entity, Marker(value + 1));
                batch.finish(&mut world).unwrap();
            }
        }),
        0
    );
    assert_eq!(queue.records.capacity(), records);
    assert_eq!(queue.payload.capacity(), bytes);
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(33)));
}

#[test]
fn default_ui_budget_covers_editor_requests_and_transform_batches() {
    use crate::ui::widgets::{TextEditing, TextInput};

    let capacity = crate::input::event::EventCapacity::default()
        .resolve()
        .unwrap()
        .commands
        .unwrap();
    let limits = capacity.limits().unwrap();
    for requirements in [
        CommandCapacity::new()
            .insert::<TextEditing>(1)
            .insert::<TextInput>(1)
            .text_input(1),
        CommandCapacity::new()
            .set_theme(1)
            .set::<crate::ui::property::prop::Transform>(3),
    ] {
        let required = requirements.limits().unwrap();
        assert!(limits.records >= required.records);
        assert!(limits.payload_bytes >= required.payload_bytes);
        assert!(limits.payload_alignment >= required.payload_alignment);
    }
    let mut world = World::new();
    let entity = world.spawn(TextEditing::default());
    world.insert(entity, TextInput::default());
    let mut queue = CommandQueue::bounded(capacity).unwrap();
    for reverse in [false, true] {
        assert_eq!(
            tracked_allocations(|| {
                let mut batch = queue.begin();
                if reverse {
                    batch.commands().insert(entity, TextInput::default());
                    batch.commands().insert(entity, TextEditing::default());
                } else {
                    batch.commands().insert(entity, TextEditing::default());
                    batch.commands().insert(entity, TextInput::default());
                }
                batch.finish(&mut world).unwrap();
            }),
            0
        );
    }
}

#[test]
fn semantic_requirements_are_const_and_build_without_allocation() {
    const CAPACITY: CommandCapacity = CommandCapacity::new()
        .insert::<Marker>(1)
        .remove::<Marker>(2)
        .despawn(1)
        .insert_resource::<Resource>(1)
        .remove_resource::<Resource>(1)
        .set::<crate::ui::property::prop::Width>(1)
        .emit::<UnitNotification>(1)
        .set_theme(1);
    assert_eq!(CAPACITY.limits().unwrap().records, 9);
    assert_eq!(
        tracked_allocations(|| {
            let capacity = CommandCapacity::new()
                .insert::<Marker>(1)
                .remove::<Marker>(2)
                .despawn(1)
                .insert_resource::<Resource>(1)
                .remove_resource::<Resource>(1)
                .set::<crate::ui::property::prop::Width>(1)
                .emit::<UnitNotification>(1)
                .set_theme(1);
            assert_eq!(capacity, CAPACITY);
        }),
        0
    );
}

#[test]
fn mixed_requirements_accept_every_operation_order_without_first_or_repeated_growth() {
    const CAPACITY: CommandCapacity = CommandCapacity::new()
        .insert::<Small>(1)
        .insert::<Medium>(1)
        .insert::<OverAligned>(1);
    const REVERSED: CommandCapacity = CommandCapacity::new()
        .insert::<OverAligned>(1)
        .insert::<Medium>(1)
        .insert::<Small>(1);
    assert_eq!(CAPACITY.limits(), REVERSED.limits());
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Small([0; 3]));
    world.insert(entity, Medium(0));
    world.insert(entity, OverAligned([0; 160]));
    let mut queue = CommandQueue::bounded(CAPACITY).unwrap();
    let records = queue.records.capacity();
    let bytes = queue.payload.capacity();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        assert_eq!(
            tracked_allocations(|| {
                for _ in 0..2 {
                    let mut batch = queue.begin();
                    for item in order {
                        match item {
                            0 => batch.commands().insert(entity, Small([3; 3])),
                            1 => batch.commands().insert(entity, Medium(8)),
                            2 => batch.commands().insert(entity, OverAligned([64; 160])),
                            _ => unreachable!("three operation kinds"),
                        }
                    }
                    batch.finish(&mut world).unwrap();
                }
            }),
            0
        );
    }
    assert_eq!(queue.records.capacity(), records);
    assert_eq!(queue.payload.capacity(), bytes);
    assert_eq!(world.get::<Small>(entity), Some(&Small([3; 3])));
    assert_eq!(world.get::<Medium>(entity), Some(&Medium(8)));
    assert_eq!(world.get::<OverAligned>(entity).unwrap().0, [64; 160]);
}

#[test]
fn unit_emission_reserves_the_real_function_tuple_instead_of_an_empty_payload() {
    const CAPACITY: CommandCapacity = CommandCapacity::new().emit::<UnitNotification>(2);
    let limits = CAPACITY.limits().unwrap();
    assert_eq!(limits.records, 2);
    assert_eq!(
        limits.payload_bytes,
        core::mem::size_of::<EntityPayload<UnitNotification>>() * 2
    );
    assert_eq!(
        limits.payload_alignment,
        core::mem::align_of::<EntityPayload<UnitNotification>>()
    );
    assert!(limits.payload_bytes > 0);
    assert_eq!(limits.payload_alignment, 64);
}

#[test]
fn overflowing_requirements_preserve_the_first_error_without_allocating() {
    const CAPACITY: CommandCapacity = CommandCapacity::new().insert_resource::<u16>(usize::MAX);
    assert_eq!(CAPACITY.limits(), Err(CommandError::SizeOverflow));
    assert_eq!(
        tracked_allocations(|| {
            assert_eq!(CAPACITY.despawn(1).set_theme(usize::MAX), CAPACITY);
            assert_eq!(
                CommandCapacity::new()
                    .insert_resource::<u8>(usize::MAX - 192)
                    .insert::<OverAligned>(1)
                    .limits(),
                Err(CommandError::SizeOverflow)
            );
            assert!(matches!(
                CommandQueue::bounded(CAPACITY),
                Err(CommandError::SizeOverflow)
            ));
        }),
        0
    );
}

#[test]
fn bounded_storage_returns_rejected_ownership_without_mutating_records() {
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(CommandCapacity::new().insert::<DropMarker>(1)).unwrap();
    let entity = Entity {
        id: 0,
        generation: 0,
    };
    assert!(
        queue
            .try_push_payload(
                Some(entity),
                DropMarker(drops.clone()),
                apply_insert::<DropMarker>
            )
            .is_ok()
    );
    let result = queue.try_push_payload(
        Some(entity),
        DropMarker(drops.clone()),
        apply_insert::<DropMarker>,
    );
    let (error, rejected) = result.err().unwrap();
    assert_eq!(error, CommandError::RecordCapacity { available: 1 });
    assert_eq!(queue.records.len(), 1);
    assert_eq!(drops.get(), 0);
    drop(rejected);
    queue.discard();
    assert_eq!(drops.get(), 2);
}

#[test]
fn capacity_failure_discards_the_batch_and_reuses_storage() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, Marker(0));
    let drops = Rc::new(Cell::new(0));
    let mut queue = CommandQueue::bounded(CommandCapacity::new().insert::<DropMarker>(1)).unwrap();
    let allocations = tracked_allocations(|| {
        let mut batch = queue.begin();
        batch.commands().insert(entity, DropMarker(drops.clone()));
        batch.commands().insert(entity, DropMarker(drops.clone()));
        batch.commands().insert(entity, Marker(8));
        assert_eq!(
            batch.finish(&mut world),
            Err(CommandError::RecordCapacity { available: 1 })
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(drops.get(), 2);
    assert!(!world.has::<DropMarker>(entity));
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(0)));
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = queue.begin();
            batch.commands().insert(entity, Marker(5));
            batch.finish(&mut world).unwrap();
        }),
        0
    );
    assert_eq!(world.get::<Marker>(entity), Some(&Marker(5)));
}

#[test]
fn logical_payload_and_alignment_limits_do_not_use_allocator_slack() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    let mut queue = CommandQueue::bounded(
        CommandCapacity::new()
            .insert::<Marker>(1)
            .remove::<Marker>(2),
    )
    .unwrap();
    // Deliberately enlarge physical storage without raising logical limits.
    queue.payload.reserve(256, 64).unwrap();
    let mut batch = queue.begin();
    batch.commands().insert(entity, Marker(7));
    batch.commands().insert(entity, Marker(9));
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::PayloadCapacity {
            required: 8,
            available: 4
        })
    );
    assert!(!world.has::<Marker>(entity));
    let mut batch = queue.begin();
    batch.commands().insert(entity, OverAligned([0; 160]));
    assert_eq!(
        batch.finish(&mut world),
        Err(CommandError::PayloadAlignment {
            required: 64,
            available: 4
        })
    );
    assert!(!world.has::<OverAligned>(entity));
}

#[test]
fn empty_bounded_queue_rejects_commands_and_invalid_requirements_explicitly() {
    let mut queue = CommandQueue::bounded(CommandCapacity::default()).unwrap();
    let mut world = World::new();
    let allocations = tracked_allocations(|| {
        let mut batch = queue.begin();
        batch.commands().remove_resource::<Resource>();
        assert_eq!(
            batch.finish(&mut world),
            Err(CommandError::RecordCapacity { available: 0 })
        );
    });
    assert_eq!(allocations, 0);
    assert_eq!(queue.records.capacity(), 0);
    assert_eq!(queue.payload.capacity(), 0);
    assert!(matches!(
        CommandQueue::bounded(CommandCapacity::new().despawn(usize::MAX).despawn(1)),
        Err(CommandError::SizeOverflow)
    ));
    assert!(matches!(
        CommandQueue::bounded(
            CommandCapacity::new().insert::<OverAligned>(
                isize::MAX as usize / core::mem::size_of::<OverAligned>() + 1
            )
        ),
        Err(CommandError::InvalidLayout)
    ));
}

#[test]
fn over_aligned_zero_sized_commands_need_records_but_no_payload_bytes() {
    static DROPS: AtomicUsize = AtomicUsize::new(0);
    #[repr(align(128))]
    struct Empty;
    impl Component for Empty {}
    impl Drop for Empty {
        fn drop(&mut self) {
            DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
    let mut queue = CommandQueue::bounded(CommandCapacity::new().insert::<Empty>(2)).unwrap();
    let mut world = World::new();
    let entity = world.spawn_empty();
    assert_eq!(
        tracked_allocations(|| {
            let mut batch = queue.begin();
            for _ in 0..3 {
                batch.commands().insert(entity, Empty);
            }
            assert_eq!(
                batch.finish(&mut world),
                Err(CommandError::RecordCapacity { available: 2 })
            );
        }),
        0
    );
    assert_eq!(DROPS.load(Ordering::Relaxed), 3);
    assert_eq!(queue.payload.capacity(), 0);
    assert!(!world.has::<Empty>(entity));
}
