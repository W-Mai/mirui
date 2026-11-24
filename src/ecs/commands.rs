use alloc::vec::Vec;
use core::alloc::Layout;
use core::any::Any;
use core::cmp;
use core::ptr::NonNull;

use super::payload::{PayloadArena, PayloadError, PayloadRequirements, drop_stored, read_stored};
use super::{Component, Entity, World};

const MIN_PAYLOAD_CAPACITY: usize = 64;

pub(crate) type EntityPayload<T> = (T, fn(&mut World, Entity, T) -> Result<(), CommandError>);
pub(crate) type ResourcePayload<T> = (T, fn(&mut World, T));

/// A typed command submitted by a framework extension.
///
/// The callback's borrows have ended when this runs. Validate the operation before mutating the World if a rejection must preserve the original state.
pub trait DeferredOperation: 'static {
    fn apply(self, world: &mut World) -> Result<(), CommandCommitError>;
}

trait EntityOperation: 'static {
    fn apply(
        self,
        world: &mut World,
        entity: Entity,
        event: Option<&dyn Any>,
    ) -> Result<(), CommandError>;
}

impl<T: 'static> EntityOperation for EntityPayload<T> {
    fn apply(
        self,
        world: &mut World,
        entity: Entity,
        _event: Option<&dyn Any>,
    ) -> Result<(), CommandError> {
        (self.1)(world, entity, self.0)
    }
}

struct EventOperation<E>(fn(&mut World, Entity, &E) -> Result<(), CommandError>);

impl<E: 'static> EntityOperation for EventOperation<E> {
    fn apply(
        self,
        world: &mut World,
        entity: Entity,
        event: Option<&dyn Any>,
    ) -> Result<(), CommandError> {
        let event = event
            .and_then(|event| event.downcast_ref::<E>())
            .ok_or(CommandError::EventContext)?;
        (self.0)(world, entity, event)
    }
}

pub(crate) type ApplyCommand =
    unsafe fn(&mut World, Option<Entity>, *mut u8, Option<&dyn Any>) -> Result<(), CommandError>;

/// Storage requirements for deferred operations in one callback.
///
/// Counts describe a combination of operations, not individual runtime quotas. Their real payload layouts determine the byte and alignment requirements for every ordering of that combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandCapacity {
    records: usize,
    payload: PayloadRequirements,
    error: Option<CommandError>,
}

impl CommandCapacity {
    pub(crate) const fn record_count(self) -> usize {
        self.records
    }
    /// Declare no deferred operations.
    pub const fn new() -> Self {
        Self {
            records: 0,
            payload: PayloadRequirements::new(),
            error: None,
        }
    }

    /// Reserve component insertions or replacements.
    pub const fn insert<T: Component>(self, count: usize) -> Self {
        self.payload::<T>(count)
    }

    /// Reserve component removals.
    pub const fn remove<T: Component>(self, count: usize) -> Self {
        self.empty(count)
    }

    /// Reserve entity removals.
    pub const fn despawn(self, count: usize) -> Self {
        self.empty(count)
    }

    /// Reserve resource insertions or replacements.
    pub const fn insert_resource<T: 'static>(self, count: usize) -> Self {
        self.payload::<T>(count)
    }

    /// Reserve resource removals.
    pub const fn remove_resource<T: 'static>(self, count: usize) -> Self {
        self.empty(count)
    }

    /// Reserve operations of the given extension command type.
    pub const fn enqueue<O: DeferredOperation>(self, count: usize) -> Self {
        self.payload::<O>(count)
    }

    pub(crate) const fn entity_value<T: 'static>(self, count: usize) -> Self {
        self.payload::<EntityPayload<T>>(count)
    }

    pub(crate) const fn resource_value<T: 'static>(self, count: usize) -> Self {
        self.payload::<ResourcePayload<T>>(count)
    }

    const fn payload<T>(mut self, count: usize) -> Self {
        if self.error.is_some() {
            return self;
        }
        let Some(records) = self.records.checked_add(count) else {
            self.error = Some(CommandError::SizeOverflow);
            return self;
        };
        let Some(payload) = self.payload.include::<T>(count) else {
            self.error = Some(CommandError::SizeOverflow);
            return self;
        };
        if payload.limits().is_none() {
            self.error = Some(CommandError::SizeOverflow);
            return self;
        }
        self.records = records;
        self.payload = payload;
        self
    }

    const fn empty(mut self, count: usize) -> Self {
        if self.error.is_none() {
            match self.records.checked_add(count) {
                Some(records) => self.records = records,
                None => self.error = Some(CommandError::SizeOverflow),
            }
        }
        self
    }

    const fn limits(self) -> Result<CommandLimits, CommandError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let Some((payload_bytes, payload_alignment)) = self.payload.limits() else {
            return Err(CommandError::SizeOverflow);
        };
        Ok(CommandLimits {
            records: self.records,
            payload_bytes,
            payload_alignment,
        })
    }

    pub(crate) const fn validate(self) -> Result<(), CommandError> {
        match self.validated_limits() {
            Ok(_) => Ok(()),
            Err(error) => Err(error),
        }
    }

    const fn validated_limits(self) -> Result<CommandLimits, CommandError> {
        let limits = match self.limits() {
            Ok(limits) => limits,
            Err(error) => return Err(error),
        };
        if Layout::array::<CommandRecord>(limits.records).is_err() {
            return Err(CommandError::SizeOverflow);
        }
        if Layout::from_size_align(limits.payload_bytes, limits.payload_alignment).is_err() {
            return Err(CommandError::InvalidLayout);
        }
        Ok(limits)
    }
}

impl Default for CommandCapacity {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CommandLimits {
    records: usize,
    payload_bytes: usize,
    payload_alignment: usize,
}

/// Failure to record or submit deferred commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandError {
    RecordCapacity { available: usize },
    PayloadCapacity { required: usize, available: usize },
    PayloadAlignment { required: usize, available: usize },
    SizeOverflow,
    InvalidLayout,
    AllocationFailed,
    Busy,
    EventContext,
    Commit(CommandCommitError),
}

/// A deferred operation rejected after callback borrows have ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandCommitError {
    OperationRejected {
        operation: &'static str,
    },
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
    apply: ApplyCommand,
    drop_payload: Option<unsafe fn(*mut u8)>,
    consumed: bool,
}

#[derive(Default)]
pub(crate) struct CommandQueue {
    records: Vec<CommandRecord>,
    payload: PayloadArena,
    capacity: Option<CommandLimits>,
    error: Option<CommandError>,
}

impl CommandQueue {
    pub(crate) fn bounded(capacity: CommandCapacity) -> Result<Self, CommandError> {
        let capacity = capacity.validated_limits()?;
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

    fn push_payload<T: 'static>(&mut self, target: Option<Entity>, value: T, apply: ApplyCommand) {
        if let Err((error, _value)) = self.try_push_payload(target, value, apply) {
            self.error = Some(error);
        }
    }

    fn try_push_payload<T: 'static>(
        &mut self,
        target: Option<Entity>,
        value: T,
        apply: ApplyCommand,
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

    fn push_empty(&mut self, target: Option<Entity>, apply: ApplyCommand) {
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

    fn apply(&mut self, world: &mut World, event: Option<&dyn Any>) -> Result<(), CommandError> {
        for index in 0..self.records.len() {
            let record = self.records[index];
            self.records[index].consumed = true;
            let payload = self.record_ptr(record);
            // SAFETY: the record's apply function matches the stored payload
            // type and each record is consumed at most once.
            unsafe { (record.apply)(world, record.target, payload, event) }?;
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

    pub(crate) fn finish(self, world: &mut World) -> Result<(), CommandError> {
        self.finish_in(world, None)
    }

    pub(crate) fn finish_with_event<E: 'static>(
        self,
        world: &mut World,
        event: &E,
    ) -> Result<(), CommandError> {
        self.finish_in(world, Some(event))
    }

    fn finish_in(mut self, world: &mut World, event: Option<&dyn Any>) -> Result<(), CommandError> {
        if let Some(error) = self.queue.error {
            self.queue.discard();
            self.finished = true;
            return Err(error);
        }
        self.queue.apply(world, event)?;
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
    /// Record a typed extension command for execution in the current batch.
    pub fn enqueue<O: DeferredOperation>(&mut self, operation: O) {
        self.queue
            .push_payload(None, operation, apply_operation::<O>);
    }

    pub(crate) fn apply<T: 'static>(
        &mut self,
        entity: Entity,
        value: T,
        apply: fn(&mut World, Entity, T) -> Result<(), CommandError>,
    ) {
        let payload: EntityPayload<T> = (value, apply);
        self.queue
            .push_payload(Some(entity), payload, apply_value::<EntityPayload<T>>);
    }

    pub(crate) fn apply_event<E: 'static>(
        &mut self,
        entity: Entity,
        apply: fn(&mut World, Entity, &E) -> Result<(), CommandError>,
    ) {
        self.queue.push_payload(
            Some(entity),
            EventOperation(apply),
            apply_value::<EventOperation<E>>,
        );
    }

    pub(crate) fn apply_resource<T: 'static>(&mut self, value: T, apply: fn(&mut World, T)) {
        let payload: ResourcePayload<T> = (value, apply);
        self.queue
            .push_payload(None, payload, apply_resource_value::<T>);
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

    pub(crate) fn apply_empty(&mut self, entity: Entity, apply: ApplyCommand) {
        self.queue.push_empty(Some(entity), apply);
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

unsafe fn apply_operation<O: DeferredOperation>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
    _event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored O.
    unsafe { read_stored::<O>(payload) }
        .apply(world)
        .map_err(CommandError::Commit)
}

unsafe fn apply_insert<T: Component>(
    world: &mut World,
    target: Option<Entity>,
    payload: *mut u8,
    _event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored T.
    let component = unsafe { read_stored::<T>(payload) };
    world.insert(target.expect("component command target"), component);
    Ok(())
}

unsafe fn apply_value<T: EntityOperation>(
    world: &mut World,
    target: Option<Entity>,
    payload: *mut u8,
    event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored operation of type T.
    let operation = unsafe { read_stored::<T>(payload) };
    operation.apply(world, target.expect("deferred value target"), event)
}

unsafe fn apply_resource_value<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
    _event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    // SAFETY: this apply function is paired with a stored value/function tuple.
    let (value, apply) = unsafe { read_stored::<ResourcePayload<T>>(payload) };
    apply(world, value);
    Ok(())
}

unsafe fn apply_remove<T: Component>(
    world: &mut World,
    target: Option<Entity>,
    _payload: *mut u8,
    _event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    let _ = world.remove::<T>(target.expect("component command target"));
    Ok(())
}

unsafe fn apply_insert_resource<T: 'static>(
    world: &mut World,
    _target: Option<Entity>,
    payload: *mut u8,
    _event: Option<&dyn Any>,
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
    _event: Option<&dyn Any>,
) -> Result<(), CommandError> {
    let _ = world.remove_resource::<T>();
    Ok(())
}

#[cfg(test)]
#[path = "command_event_tests.rs"]
mod event_tests;

#[cfg(test)]
#[path = "../../tests/unit/ecs/commands/tests.rs"]
mod tests;
