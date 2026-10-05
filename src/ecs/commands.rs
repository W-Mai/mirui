use alloc::vec::Vec;
use core::alloc::Layout;
use core::cmp;
use core::ptr::NonNull;

use super::payload::{PayloadArena, PayloadError, drop_stored, read_stored};
use super::{Component, Entity, World};

const MIN_PAYLOAD_CAPACITY: usize = 64;

/// Explicit logical limits for deferred command storage.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCapacity {
    pub records: usize,
    pub payload_bytes: usize,
    pub payload_alignment: usize,
}

impl Default for CommandCapacity {
    fn default() -> Self {
        Self {
            records: 0,
            payload_bytes: 0,
            payload_alignment: 1,
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    RecordCapacity { available: usize },
    PayloadCapacity { required: usize, available: usize },
    PayloadAlignment { required: usize, available: usize },
    SizeOverflow,
    InvalidLayout,
    AllocationFailed,
    Busy,
    Commit(CommandCommitError),
}

/// A deferred operation rejected after callback borrows have ended.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCommitError {
    MissingTarget {
        entity: Entity,
    },
    MissingSource {
        entity: Entity,
        source: &'static str,
    },
    MissingService {
        service: &'static str,
    },
    ForeignOwner {
        service: &'static str,
    },
    RecordCapacity {
        available: usize,
    },
    PayloadCapacity {
        required: usize,
        available: usize,
    },
    PayloadAlignment {
        required: usize,
        available: usize,
    },
    Busy,
}

impl From<PayloadError> for CommandError {
    fn from(error: PayloadError) -> Self {
        match error {
            PayloadError::SizeOverflow => Self::SizeOverflow,
            PayloadError::InvalidLayout => Self::InvalidLayout,
            PayloadError::AllocationFailed => Self::AllocationFailed,
            PayloadError::Capacity {
                required,
                available,
            } => Self::PayloadCapacity {
                required,
                available,
            },
            PayloadError::Alignment {
                required,
                available,
            } => Self::PayloadAlignment {
                required,
                available,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct CommandRecord {
    target: Option<Entity>,
    payload_offset: usize,
    apply: unsafe fn(&mut World, Option<Entity>, *mut u8) -> Result<(), CommandError>,
    drop_payload: Option<unsafe fn(*mut u8)>,
    consumed: bool,
}

#[derive(Default)]
pub(crate) struct CommandQueue {
    records: Vec<CommandRecord>,
    payload: PayloadArena,
    capacity: Option<CommandCapacity>,
    error: Option<CommandError>,
}

impl CommandQueue {
    pub(crate) fn bounded(capacity: CommandCapacity) -> Result<Self, CommandError> {
        Layout::array::<CommandRecord>(capacity.records).map_err(|_| CommandError::SizeOverflow)?;
        Layout::from_size_align(capacity.payload_bytes, capacity.payload_alignment)
            .map_err(|_| CommandError::InvalidLayout)?;
        let mut queue = Self::default();
        queue
            .payload
            .reserve(capacity.payload_bytes, capacity.payload_alignment)?;
        queue
            .records
            .try_reserve_exact(capacity.records)
            .map_err(|_| CommandError::AllocationFailed)?;
        queue.capacity = Some(capacity);
        Ok(queue)
    }

    pub(crate) fn begin(&mut self) -> CommandBatch<'_> {
        self.discard();
        CommandBatch {
            queue: self,
            finished: false,
        }
    }

    fn push_payload<T: 'static>(
        &mut self,
        target: Option<Entity>,
        value: T,
        apply: unsafe fn(&mut World, Option<Entity>, *mut u8) -> Result<(), CommandError>,
    ) {
        if let Err((error, _value)) = self.try_push_payload(target, value, apply) {
            self.error = Some(error);
        }
    }

    fn try_push_payload<T: 'static>(
        &mut self,
        target: Option<Entity>,
        value: T,
        apply: unsafe fn(&mut World, Option<Entity>, *mut u8) -> Result<(), CommandError>,
    ) -> Result<(), (CommandError, T)> {
        if let Err(error) = self.prepare_record() {
            return Err((error, value));
        }
        let offset = self.store_payload(value)?;
        self.records.push(CommandRecord {
            target,
            payload_offset: offset,
            apply,
            drop_payload: Some(drop_stored::<T>),
            consumed: false,
        });
        Ok(())
    }

    fn prepare_record(&mut self) -> Result<(), CommandError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        if let Some(capacity) = self.capacity {
            if self.records.len() >= capacity.records {
                return Err(CommandError::RecordCapacity {
                    available: capacity.records,
                });
            }
        } else {
            self.records
                .try_reserve(1)
                .map_err(|_| CommandError::AllocationFailed)?;
        }
        Ok(())
    }

    fn store_payload<T: 'static>(&mut self, value: T) -> Result<usize, (CommandError, T)> {
        let (required, alignment) = match self.payload.required::<T>() {
            Ok(required) => required,
            Err(error) => return Err((error.into(), value)),
        };
        if let Some(capacity) = self.capacity {
            if alignment > capacity.payload_alignment {
                return Err((
                    CommandError::PayloadAlignment {
                        required: alignment,
                        available: capacity.payload_alignment,
                    },
                    value,
                ));
            }
            if required > capacity.payload_bytes {
                return Err((
                    CommandError::PayloadCapacity {
                        required,
                        available: capacity.payload_bytes,
                    },
                    value,
                ));
            }
        } else if required > self.payload.capacity() || alignment > self.payload.alignment() {
            let current = self.payload.capacity();
            let doubled = current.checked_mul(2).unwrap_or(current);
            let capacity = cmp::max(cmp::max(required, doubled), MIN_PAYLOAD_CAPACITY)
                .checked_next_power_of_two()
                .unwrap_or(cmp::max(required, doubled));
            if let Err(error) = self.payload.reserve(capacity, alignment) {
                return Err((error.into(), value));
            }
        }
        self.payload
            .try_store(value)
            .map_err(|(error, value)| (error.into(), value))
    }

    fn push_empty(
        &mut self,
        target: Option<Entity>,
        apply: unsafe fn(&mut World, Option<Entity>, *mut u8) -> Result<(), CommandError>,
    ) {
        if let Err(error) = self.prepare_record() {
            self.error = Some(error);
            return;
        }
        self.records.push(CommandRecord {
            target,
            payload_offset: 0,
            apply,
            drop_payload: None,
            consumed: false,
        });
    }

    fn record_ptr(&mut self, record: CommandRecord) -> *mut u8 {
        if record.drop_payload.is_none() {
            return NonNull::<u8>::dangling().as_ptr();
        }
        self.payload.ptr(record.payload_offset)
    }

    fn apply(&mut self, world: &mut World) -> Result<(), CommandError> {
        for index in 0..self.records.len() {
            let record = self.records[index];
            self.records[index].consumed = true;
            let payload = self.record_ptr(record);
            // SAFETY: the record's apply function matches the stored payload
            // type and each record is consumed at most once.
            unsafe { (record.apply)(world, record.target, payload) }?;
        }
        self.records.clear();
        self.payload.clear();
        Ok(())
    }

    fn discard(&mut self) {
        for index in 0..self.records.len() {
            let record = self.records[index];
            if record.consumed {
                continue;
            }
            self.records[index].consumed = true;
            if let Some(drop_payload) = record.drop_payload {
                let payload = self.record_ptr(record);
                // SAFETY: unconsumed records still own exactly one payload.
                unsafe { drop_payload(payload) };
            }
        }
        self.records.clear();
        self.payload.clear();
        self.error = None;
    }
}

impl Drop for CommandQueue {
    fn drop(&mut self) {
        self.discard();
    }
}

pub(crate) struct CommandBatch<'queue> {
    queue: &'queue mut CommandQueue,
    finished: bool,
}

impl CommandBatch<'_> {
    pub(crate) fn commands(&mut self) -> DeferredCommands<'_> {
        DeferredCommands { queue: self.queue }
    }

    pub(crate) fn finish(mut self, world: &mut World) -> Result<(), CommandError> {
        if let Some(error) = self.queue.error {
            self.queue.discard();
            self.finished = true;
            return Err(error);
        }
        self.queue.apply(world)?;
        self.finished = true;
        Ok(())
    }
}

impl Drop for CommandBatch<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.queue.discard();
        }
    }
}

/// Deferred structural changes emitted by generated system wrappers.
///
/// This type is public only so proc-macro expansions can cross the crate
/// boundary. Author code receives the higher-level `commands!` facade.
#[doc(hidden)]
pub struct DeferredCommands<'queue> {
    queue: &'queue mut CommandQueue,
}

impl DeferredCommands<'_> {
    pub(crate) fn apply<T: 'static>(
        &mut self,
        entity: Entity,
        value: T,
        apply: fn(&mut World, Entity, T) -> Result<(), CommandError>,
    ) {
        self.queue
            .push_payload(Some(entity), (value, apply), apply_value::<T>);
    }

    pub(crate) fn apply_resource<T: 'static>(&mut self, value: T, apply: fn(&mut World, T)) {
        self.queue
            .push_payload(None, (value, apply), apply_resource_value::<T>);
    }

    #[doc(hidden)]
    pub fn insert<T: Component>(&mut self, entity: Entity, component: T) {
        self.queue
            .push_payload(Some(entity), component, apply_insert::<T>);
    }

    #[doc(hidden)]
    pub fn remove<T: Component>(&mut self, entity: Entity) {
        self.queue.push_empty(Some(entity), apply_remove::<T>);
    }

    #[doc(hidden)]
    pub fn despawn(&mut self, entity: Entity) {
        self.queue.push_empty(Some(entity), apply_despawn);
    }

    #[doc(hidden)]
    pub fn insert_resource<T: 'static>(&mut self, resource: T) {
        self.queue
            .push_payload(None, resource, apply_insert_resource::<T>);
    }

    #[doc(hidden)]
    pub fn remove_resource<T: 'static>(&mut self) {
        self.queue.push_empty(None, apply_remove_resource::<T>);
    }
}

unsafe fn apply_insert<T: Component>(
    world: &mut World,
    target: Option<Entity>,
    payload: *mut u8,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored T.
    let component = unsafe { read_stored::<T>(payload) };
    world.insert(target.expect("component command target"), component);
    Ok(())
}

unsafe fn apply_value<T: 'static>(
    world: &mut World,
    target: Option<Entity>,
    payload: *mut u8,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored value/function tuple.
    let (value, apply) = unsafe {
        read_stored::<(T, fn(&mut World, Entity, T) -> Result<(), CommandError>)>(payload)
    };
    apply(world, target.expect("deferred value target"), value)
}

unsafe fn apply_resource_value<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored value/function tuple.
    let (value, apply) = unsafe { read_stored::<(T, fn(&mut World, T))>(payload) };
    apply(world, value);
    Ok(())
}

unsafe fn apply_remove<T: Component>(
    world: &mut World,
    target: Option<Entity>,
    _payload: *mut u8,
) -> Result<(), CommandError> {
    let _ = world.remove::<T>(target.expect("component command target"));
    Ok(())
}

unsafe fn apply_despawn(
    world: &mut World,
    target: Option<Entity>,
    _payload: *mut u8,
) -> Result<(), CommandError> {
    let _ = world.despawn(target.expect("despawn command target"));
    Ok(())
}

unsafe fn apply_insert_resource<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored T.
    let resource = unsafe { read_stored::<T>(payload) };
    world.insert_resource(resource);
    Ok(())
}

unsafe fn apply_remove_resource<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    _payload: *mut u8,
) -> Result<(), CommandError> {
    let _ = world.remove_resource::<T>();
    Ok(())
}

#[cfg(test)]
mod tests {
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

    fn capacity<T>(records: usize) -> CommandCapacity {
        CommandCapacity {
            records,
            payload_bytes: core::mem::size_of::<T>() * records,
            payload_alignment: core::mem::align_of::<T>(),
        }
    }

    #[test]
    fn bounded_batch_records_and_commits_without_first_call_growth() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Marker(0));
        let mut queue = CommandQueue::bounded(capacity::<Marker>(2)).unwrap();
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
    fn bounded_storage_returns_rejected_ownership_without_mutating_records() {
        let drops = Rc::new(Cell::new(0));
        let mut queue = CommandQueue::bounded(capacity::<DropMarker>(1)).unwrap();
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
        let mut queue = CommandQueue::bounded(capacity::<DropMarker>(1)).unwrap();
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
        let mut queue = CommandQueue::bounded(CommandCapacity {
            records: 3,
            payload_bytes: 4,
            payload_alignment: 4,
        })
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
    fn empty_bounded_queue_rejects_commands_and_invalid_layouts_explicitly() {
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
            CommandQueue::bounded(CommandCapacity {
                records: usize::MAX,
                ..CommandCapacity::default()
            }),
            Err(CommandError::SizeOverflow)
        ));
        assert!(matches!(
            CommandQueue::bounded(CommandCapacity {
                payload_alignment: 3,
                ..CommandCapacity::default()
            }),
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
        let mut queue = CommandQueue::bounded(CommandCapacity {
            records: 2,
            ..CommandCapacity::default()
        })
        .unwrap();
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
}
