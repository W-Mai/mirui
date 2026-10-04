use alloc::alloc::handle_alloc_error;
use alloc::vec::Vec;
use core::alloc::Layout;
use core::cmp;
use core::ptr::NonNull;

use super::payload::{PayloadArena, PayloadError, drop_stored, read_stored};
use super::{Component, Entity, World};

const MIN_PAYLOAD_CAPACITY: usize = 64;

#[derive(Clone, Copy)]
struct CommandRecord {
    target: Option<Entity>,
    payload_offset: usize,
    apply: unsafe fn(&mut World, Option<Entity>, *mut u8),
    drop_payload: Option<unsafe fn(*mut u8)>,
    consumed: bool,
}

#[derive(Default)]
pub(crate) struct CommandQueue {
    records: Vec<CommandRecord>,
    payload: PayloadArena,
}

impl CommandQueue {
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
        apply: unsafe fn(&mut World, Option<Entity>, *mut u8),
    ) {
        self.records
            .try_reserve(1)
            .expect("deferred command record capacity exhausted");
        let offset = self.store_payload(value);
        self.records.push(CommandRecord {
            target,
            payload_offset: offset,
            apply,
            drop_payload: Some(drop_stored::<T>),
            consumed: false,
        });
    }

    fn store_payload<T: 'static>(&mut self, value: T) -> usize {
        let (required, alignment) = self
            .payload
            .required::<T>()
            .expect("deferred command payload size overflow");
        if required > self.payload.capacity() || alignment > self.payload.alignment() {
            let current = self.payload.capacity();
            let doubled = current.checked_mul(2).unwrap_or(current);
            let capacity = cmp::max(cmp::max(required, doubled), MIN_PAYLOAD_CAPACITY)
                .checked_next_power_of_two()
                .unwrap_or(cmp::max(required, doubled));
            match self.payload.reserve(capacity, alignment) {
                Ok(()) => {}
                Err(PayloadError::AllocationFailed) => {
                    let layout =
                        Layout::from_size_align(capacity, alignment.max(self.payload.alignment()))
                            .expect("invalid deferred command payload layout");
                    handle_alloc_error(layout);
                }
                Err(error) => panic!("deferred command payload reservation failed: {error:?}"),
            }
        }
        match self.payload.try_store(value) {
            Ok(offset) => offset,
            Err((error, _value)) => panic!("deferred command payload storage failed: {error:?}"),
        }
    }

    fn push_empty(
        &mut self,
        target: Option<Entity>,
        apply: unsafe fn(&mut World, Option<Entity>, *mut u8),
    ) {
        self.records
            .try_reserve(1)
            .expect("deferred command record capacity exhausted");
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

    fn apply(&mut self, world: &mut World) {
        for index in 0..self.records.len() {
            let record = self.records[index];
            self.records[index].consumed = true;
            let payload = self.record_ptr(record);
            // SAFETY: the record's apply function matches the stored payload
            // type and each record is consumed at most once.
            unsafe { (record.apply)(world, record.target, payload) };
        }
        self.records.clear();
        self.payload.clear();
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

    pub(crate) fn finish(mut self, world: &mut World) {
        self.queue.apply(world);
        self.finished = true;
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
        apply: fn(&mut World, Entity, T),
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

unsafe fn apply_insert<T: Component>(world: &mut World, target: Option<Entity>, payload: *mut u8) {
    // SAFETY: this apply function is paired with a stored T.
    let component = unsafe { read_stored::<T>(payload) };
    world.insert(target.expect("component command target"), component);
}

unsafe fn apply_value<T: 'static>(world: &mut World, target: Option<Entity>, payload: *mut u8) {
    // SAFETY: this apply function is paired with a stored value/function tuple.
    let (value, apply) = unsafe { read_stored::<(T, fn(&mut World, Entity, T))>(payload) };
    apply(world, target.expect("deferred value target"), value);
}

unsafe fn apply_resource_value<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
) {
    // SAFETY: this apply function is paired with a stored value/function tuple.
    let (value, apply) = unsafe { read_stored::<(T, fn(&mut World, T))>(payload) };
    apply(world, value);
}

unsafe fn apply_remove<T: Component>(world: &mut World, target: Option<Entity>, _payload: *mut u8) {
    let _ = world.remove::<T>(target.expect("component command target"));
}

unsafe fn apply_despawn(world: &mut World, target: Option<Entity>, _payload: *mut u8) {
    let _ = world.despawn(target.expect("despawn command target"));
}

unsafe fn apply_insert_resource<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
) {
    // SAFETY: this apply function is paired with a stored T.
    let resource = unsafe { read_stored::<T>(payload) };
    world.insert_resource(resource);
}

unsafe fn apply_remove_resource<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    _payload: *mut u8,
) {
    let _ = world.remove_resource::<T>();
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
        batch.finish(&mut world);

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
            batch.finish(&mut world);
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
        batch.finish(&mut world);

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
        batch.finish(&mut world);

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
        unsafe fn panic_after_read(_world: &mut World, _target: Option<Entity>, payload: *mut u8) {
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
            batch.finish(&mut world);
        }));

        assert!(result.is_err());
        assert_eq!(drops.get(), 2);

        let entity = world.spawn_empty();
        world.insert(entity, Marker(0));
        let allocations = tracked_allocations(|| {
            let mut batch = queue.begin();
            batch.commands().insert(entity, Marker(9));
            batch.finish(&mut world);
        });
        assert_eq!(allocations, 0);
        assert_eq!(world.get::<Marker>(entity), Some(&Marker(9)));
    }
}
