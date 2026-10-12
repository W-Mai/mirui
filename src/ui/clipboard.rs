//! Component selection snapshots and platform clipboard writes.

use alloc::{boxed::Box, collections::VecDeque};
use core::any::Any;

use crate::ecs::{CommandCapacity, Entity, World, commands::DeferredCommands, world::WorldId};
use crate::input::event::{
    contract::{EntityNotification, EntityRequest, EntitySource, Event, RequestError},
    emission, request, runtime,
};

mod data;
pub use data::{ClipboardData, ClipboardFormat, ClipboardItem};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Unsupported,
    PermissionDenied,
    InvalidData,
    Capacity,
    Unavailable,
    Platform,
    Request(RequestError),
}
impl From<RequestError> for ClipboardError {
    fn from(error: RequestError) -> Self {
        Self::Request(error)
    }
}

trait Context: Any {
    fn as_any(&self) -> &dyn Any;
    fn same(&self, other: &dyn Context) -> bool;
}
impl<T: Eq + 'static> Context for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn same(&self, other: &dyn Context) -> bool {
        other
            .as_any()
            .downcast_ref::<T>()
            .is_some_and(|other| self == other)
    }
}

/// Selection availability and an owned, equality-comparable context snapshot.
pub struct SelectionState {
    selected: bool,
    context: Box<dyn Context>,
}
impl SelectionState {
    /// Capture source identity and revisions by value, not a live lookup closure.
    pub fn new(selected: bool, context: impl Eq + 'static) -> Self {
        Self {
            selected,
            context: Box::new(context),
        }
    }
    pub fn has_selection(&self) -> bool {
        self.selected
    }
}
impl PartialEq for SelectionState {
    fn eq(&self, other: &Self) -> bool {
        self.selected == other.selected && self.context.same(&*other.context)
    }
}
impl Eq for SelectionState {}

/// Inspect a component's selection without changing it.
pub struct InspectSelection;
/// Export an immutable snapshot of the selected content.
pub struct CopySelection;
macro_rules! request_event {
    ($event:ty, $result:ty) => {
        impl Event for $event {
            type Route = EntityRequest<Result<$result, ClipboardError>>;
            type Source = EntitySource;
            type Options = ();
        }
    };
}
request_event!(InspectSelection, SelectionState);
request_event!(CopySelection, ClipboardData);

/// Identity of one clipboard operation issued by an App.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardOperationId {
    owner: WorldId,
    serial: u64,
}

/// A completed copy operation. Empty selections complete without a platform write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardFinished {
    pub operation: ClipboardOperationId,
    pub result: Result<ClipboardOutcome, ClipboardError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardOutcome {
    Copied,
    EmptySelection,
}
impl Event for ClipboardFinished {
    type Route = EntityNotification;
    type Source = EntitySource;
    type Options = ();
}

/// A platform write owns the snapshot for the entire transfer.
pub struct ClipboardWrite {
    operation: ClipboardOperationId,
    data: ClipboardData,
}
impl ClipboardWrite {
    pub fn data(&self) -> &ClipboardData {
        &self.data
    }
    pub fn complete(self, result: Result<(), ClipboardError>) -> ClipboardResponse {
        ClipboardResponse {
            operation: self.operation,
            result,
        }
    }
}

/// A platform acknowledgment, matched to its issuing App and operation.
#[derive(Clone, Copy, Debug)]
pub struct ClipboardResponse {
    operation: ClipboardOperationId,
    result: Result<(), ClipboardError>,
}

pub enum ClipboardDispatch {
    Complete(ClipboardResponse),
    Pending,
}

#[doc(hidden)]
pub trait ClipboardSelectors {
    #[allow(non_upper_case_globals)]
    const InspectSelection: crate::input::event::contract::EventKey<InspectSelection> =
        InspectSelection::EVENT;
    #[allow(non_upper_case_globals)]
    const CopySelection: crate::input::event::contract::EventKey<CopySelection> =
        CopySelection::EVENT;
    #[allow(non_upper_case_globals)]
    const ClipboardFinished: crate::input::event::contract::EventKey<ClipboardFinished> =
        ClipboardFinished::EVENT;
}
impl<T: crate::ecs::Component> ClipboardSelectors for T {}

pub struct ClipboardCommands<'call, 'queue>(&'call mut DeferredCommands<'queue>);
impl<'queue> DeferredCommands<'queue> {
    pub fn clipboard(&mut self) -> ClipboardCommands<'_, 'queue> {
        ClipboardCommands(self)
    }
}
impl CommandCapacity {
    pub const fn clipboard(self, count: usize) -> Self {
        self.entity_value::<()>(count)
    }
}
impl ClipboardCommands<'_, '_> {
    pub fn copy(&mut self, target: Entity) {
        self.0.apply(target, (), |world, target, ()| {
            copy(world, target);
            Ok(())
        });
    }
}

struct Pending {
    target: Entity,
    write: ClipboardWrite,
}
struct Service {
    pending: VecDeque<Pending>,
    running: Option<(ClipboardOperationId, Entity)>,
    limit: usize,
    serial: u64,
}

fn operation(world: &mut World) -> Result<ClipboardOperationId, ClipboardError> {
    if world.resource::<Service>().is_none() {
        let limit = runtime::delivery_limits(world)
            .commands
            .map_or(0, |capacity| capacity.record_count());
        if limit == 0 {
            return Err(ClipboardError::Capacity);
        }
        let mut pending = VecDeque::new();
        pending
            .try_reserve_exact(limit)
            .map_err(|_| ClipboardError::Capacity)?;
        world.insert_resource(Service {
            pending,
            running: None,
            limit,
            serial: 0,
        });
    }
    let owner = world.id();
    let service = world.resource_mut::<Service>().unwrap();
    service.serial = service
        .serial
        .checked_add(1)
        .ok_or(ClipboardError::Unavailable)?;
    Ok(ClipboardOperationId {
        owner,
        serial: service.serial,
    })
}

fn finish(
    world: &mut World,
    target: Entity,
    operation: ClipboardOperationId,
    result: Result<ClipboardOutcome, ClipboardError>,
) {
    if let Err(error) = result {
        crate::warn!(target: "mirui::clipboard", "copy failed: {:?}", error);
    }
    if let Err(error) =
        emission::submit_if_listened(world, target, ClipboardFinished { operation, result })
    {
        crate::warn!(target: "mirui::clipboard", "clipboard notification skipped: {:?}", error);
    }
}

fn capture(world: &mut World, target: Entity) -> Result<Option<ClipboardData>, ClipboardError> {
    let inspect = request::resolve::<InspectSelection, _>(world, target)?;
    let before = inspect.invoke(world, &InspectSelection)??;
    if !before.has_selection() {
        return Ok(None);
    }
    let data = request::invoke(world, target, &CopySelection)??;
    let after = inspect.invoke(world, &InspectSelection)??;
    if before != after {
        return Err(ClipboardError::Unavailable);
    }
    data.validate()?;
    Ok(Some(data))
}

pub(crate) fn copy(world: &mut World, target: Entity) {
    let operation = match operation(world) {
        Ok(operation) => operation,
        Err(error) => {
            crate::warn!(target: "mirui::clipboard", "copy skipped: {:?}", error);
            return;
        }
    };
    let result = capture(world, target);
    match result {
        Ok(Some(data)) => {
            let service = world.resource_mut::<Service>().unwrap();
            if service.pending.len() + usize::from(service.running.is_some()) >= service.limit {
                finish(world, target, operation, Err(ClipboardError::Capacity));
            } else {
                service.pending.push_back(Pending {
                    target,
                    write: ClipboardWrite { operation, data },
                });
            }
        }
        Ok(None) => finish(
            world,
            target,
            operation,
            Ok(ClipboardOutcome::EmptySelection),
        ),
        Err(error) => finish(world, target, operation, Err(error)),
    }
}

pub(crate) fn take_write(world: &mut World) -> Option<ClipboardWrite> {
    let service = world.resource_mut::<Service>()?;
    if service.running.is_some() {
        return None;
    }
    let pending = service.pending.pop_front()?;
    service.running = Some((pending.write.operation, pending.target));
    Some(pending.write)
}

pub(crate) fn complete(world: &mut World, response: &ClipboardResponse) {
    if response.operation.owner != world.id() {
        return;
    }
    let Some(service) = world.resource_mut::<Service>() else {
        return;
    };
    let Some((operation, target)) = service.running else {
        return;
    };
    if operation != response.operation {
        return;
    }
    service.running = None;
    finish(
        world,
        target,
        operation,
        response.result.map(|()| ClipboardOutcome::Copied),
    );
}

#[cfg(test)]
mod tests;
