//! Component selection snapshots and acknowledged platform clipboard operations.

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
/// Delete the selected content after an acknowledged clipboard write.
pub struct CutSelection;
/// Import clipboard data into the receiving component's current selection.
#[derive(Clone, Debug)]
pub struct PasteSelection {
    data: ClipboardData,
}
impl PasteSelection {
    pub fn data(&self) -> &ClipboardData {
        &self.data
    }
}
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
request_event!(CutSelection, ());
request_event!(PasteSelection, ());

/// Identity of one clipboard operation issued by an App.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardOperationId {
    owner: WorldId,
    serial: u64,
}

/// A completed clipboard operation. Empty selections complete without a platform write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardFinished {
    pub operation: ClipboardOperationId,
    pub result: Result<ClipboardOutcome, ClipboardError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardOutcome {
    Copied,
    Cut,
    Pasted,
    CopiedWithoutCut,
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
            result: result.map(|()| Response::Written),
        }
    }
}

/// A platform acknowledgment, matched to its issuing App and operation.
#[derive(Clone, Debug)]
pub struct ClipboardResponse {
    operation: ClipboardOperationId,
    result: Result<Response, ClipboardError>,
}
#[derive(Clone, Debug)]
enum Response {
    Written,
    Read(PasteSelection),
}

/// An owned request for a platform clipboard snapshot.
pub struct ClipboardRead {
    operation: ClipboardOperationId,
}
impl ClipboardRead {
    pub fn complete(self, result: Result<ClipboardData, ClipboardError>) -> ClipboardResponse {
        ClipboardResponse {
            operation: self.operation,
            result: result.map(|data| Response::Read(PasteSelection { data })),
        }
    }
}

pub(crate) enum ClipboardRequest {
    Write(ClipboardWrite),
    Read(ClipboardRead),
}
impl ClipboardRequest {
    fn operation(&self) -> ClipboardOperationId {
        match self {
            Self::Write(request) => request.operation,
            Self::Read(request) => request.operation,
        }
    }
}

pub enum ClipboardDispatch {
    Complete(ClipboardResponse),
    Pending,
}

/// A host clipboard action without a physical key, such as a native menu command.
#[derive(Clone, Debug)]
pub struct ClipboardInput(ClipboardIntent);

#[derive(Clone, Debug)]
enum ClipboardIntent {
    Copy,
    Cut,
    Paste(Result<PasteSelection, ClipboardError>),
}

impl ClipboardInput {
    pub fn copy() -> Self {
        Self(ClipboardIntent::Copy)
    }

    pub fn cut() -> Self {
        Self(ClipboardIntent::Cut)
    }

    /// Own the complete native paste payload before the platform event expires.
    pub fn paste(data: Result<ClipboardData, ClipboardError>) -> Self {
        Self(ClipboardIntent::Paste(
            data.map(|data| PasteSelection { data }),
        ))
    }

    pub(crate) fn deliver(&self, world: &mut World, target: Entity) {
        match &self.0 {
            ClipboardIntent::Copy => copy(world, target),
            ClipboardIntent::Cut => cut(world, target),
            ClipboardIntent::Paste(data) => {
                let operation = match operation(world) {
                    Ok(operation) => operation,
                    Err(error) => {
                        crate::warn!(target: "mirui::clipboard", "clipboard operation skipped: {:?}", error);
                        return;
                    }
                };
                let result = data.as_ref().map_err(|error| *error).and_then(|event| {
                    event.data.validate()?;
                    capture_paste(world, target)?.apply(world, event)
                });
                finish(
                    world,
                    target,
                    operation,
                    result.map(|()| ClipboardOutcome::Pasted),
                );
            }
        }
    }
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
    const CutSelection: crate::input::event::contract::EventKey<CutSelection> = CutSelection::EVENT;
    #[allow(non_upper_case_globals)]
    const PasteSelection: crate::input::event::contract::EventKey<PasteSelection> =
        PasteSelection::EVENT;
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
    pub fn cut(&mut self, target: Entity) {
        self.0.apply(target, (), |world, target, ()| {
            cut(world, target);
            Ok(())
        });
    }
    pub fn paste(&mut self, target: Entity) {
        self.0.apply(target, (), |world, target, ()| {
            paste(world, target);
            Ok(())
        });
    }
}

struct SelectionGuard<E: Event> {
    state: SelectionState,
    inspect: request::ResolvedRequest<InspectSelection>,
    request: request::ResolvedRequest<E>,
}
impl<E: Event<Route = EntityRequest<Result<(), ClipboardError>>, Source = EntitySource>>
    SelectionGuard<E>
{
    fn apply(self, world: &mut World, event: &E) -> Result<(), ClipboardError> {
        self.request.validate(world)?;
        if self.inspect.invoke(world, &InspectSelection)?? != self.state {
            return Err(ClipboardError::Unavailable);
        }
        self.request.invoke(world, event)??;
        Ok(())
    }
}

enum Completion {
    Copy,
    Cut(SelectionGuard<CutSelection>),
    Paste(SelectionGuard<PasteSelection>),
}
struct Pending {
    target: Entity,
    request: ClipboardRequest,
    completion: Completion,
}
struct Running {
    operation: ClipboardOperationId,
    target: Entity,
    completion: Completion,
}
struct Service {
    pending: VecDeque<Pending>,
    running: Option<Running>,
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
        crate::warn!(target: "mirui::clipboard", "clipboard operation failed: {:?}", error);
    }
    if let Err(error) =
        emission::submit_if_listened(world, target, ClipboardFinished { operation, result })
    {
        crate::warn!(target: "mirui::clipboard", "clipboard notification skipped: {:?}", error);
    }
}

fn capture(
    world: &mut World,
    target: Entity,
    cut: bool,
) -> Result<Option<(ClipboardData, Completion)>, ClipboardError> {
    let inspect = request::resolve::<InspectSelection, _>(world, target)?;
    let before = inspect.invoke(world, &InspectSelection)??;
    if !before.has_selection() {
        return Ok(None);
    }
    let cut = cut
        .then(|| request::resolve::<CutSelection, _>(world, target))
        .transpose()?;
    let data = request::invoke(world, target, &CopySelection)??;
    let after = inspect.invoke(world, &InspectSelection)??;
    if before != after {
        return Err(ClipboardError::Unavailable);
    }
    data.validate()?;
    let completion = if let Some(cut) = cut {
        cut.validate(world)?;
        Completion::Cut(SelectionGuard {
            state: before,
            inspect,
            request: cut,
        })
    } else {
        Completion::Copy
    };
    Ok(Some((data, completion)))
}

pub(crate) fn copy(world: &mut World, target: Entity) {
    write_selection(world, target, false);
}

pub(crate) fn cut(world: &mut World, target: Entity) {
    write_selection(world, target, true);
}

fn write_selection(world: &mut World, target: Entity, cut: bool) {
    let operation = match operation(world) {
        Ok(operation) => operation,
        Err(error) => {
            crate::warn!(target: "mirui::clipboard", "clipboard operation skipped: {:?}", error);
            return;
        }
    };
    let result = capture(world, target, cut);
    match result {
        Ok(Some((data, completion))) => {
            enqueue(
                world,
                Pending {
                    target,
                    request: ClipboardRequest::Write(ClipboardWrite { operation, data }),
                    completion,
                },
            );
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

fn enqueue(world: &mut World, pending: Pending) {
    let service = world.resource_mut::<Service>().unwrap();
    if service.pending.len() + usize::from(service.running.is_some()) >= service.limit {
        finish(
            world,
            pending.target,
            pending.request.operation(),
            Err(ClipboardError::Capacity),
        );
    } else {
        service.pending.push_back(pending);
    }
}

pub(crate) fn paste(world: &mut World, target: Entity) {
    let operation = match operation(world) {
        Ok(operation) => operation,
        Err(error) => {
            crate::warn!(target: "mirui::clipboard", "clipboard operation skipped: {:?}", error);
            return;
        }
    };
    match capture_paste(world, target) {
        Ok(guard) => enqueue(
            world,
            Pending {
                target,
                request: ClipboardRequest::Read(ClipboardRead { operation }),
                completion: Completion::Paste(guard),
            },
        ),
        Err(error) => finish(world, target, operation, Err(error)),
    }
}

fn capture_paste(
    world: &mut World,
    target: Entity,
) -> Result<SelectionGuard<PasteSelection>, ClipboardError> {
    let request = request::resolve(world, target)?;
    let inspect = request::resolve(world, target)?;
    let state = inspect.invoke(world, &InspectSelection)??;
    request.validate(world)?;
    Ok(SelectionGuard {
        state,
        inspect,
        request,
    })
}

pub(crate) fn take_request(world: &mut World) -> Option<ClipboardRequest> {
    let service = world.resource_mut::<Service>()?;
    if service.running.is_some() {
        return None;
    }
    let pending = service.pending.pop_front()?;
    service.running = Some(Running {
        operation: pending.request.operation(),
        target: pending.target,
        completion: pending.completion,
    });
    Some(pending.request)
}

pub(crate) fn complete(world: &mut World, response: &ClipboardResponse) {
    if response.operation.owner != world.id() {
        return;
    }
    let Some(service) = world.resource_mut::<Service>() else {
        return;
    };
    let Some(running) = &service.running else {
        return;
    };
    if running.operation != response.operation {
        return;
    }
    let Running {
        operation,
        target,
        completion,
    } = service.running.take().unwrap();
    let result = match &response.result {
        Err(error) => Err(*error),
        Ok(Response::Written) => match completion {
            Completion::Copy => Ok(ClipboardOutcome::Copied),
            Completion::Cut(cut) => Ok(match cut.apply(world, &CutSelection) {
                Ok(()) => ClipboardOutcome::Cut,
                Err(error) => {
                    crate::warn!(target: "mirui::clipboard", "clipboard written, cut skipped: {:?}", error);
                    ClipboardOutcome::CopiedWithoutCut
                }
            }),
            Completion::Paste(_) => Err(ClipboardError::InvalidData),
        },
        Ok(Response::Read(event)) => match completion {
            Completion::Paste(paste) => event
                .data
                .validate()
                .and_then(|()| paste.apply(world, event))
                .map(|()| ClipboardOutcome::Pasted),
            _ => Err(ClipboardError::InvalidData),
        },
    };
    finish(world, target, operation, result);
}

#[cfg(test)]
pub(crate) mod tests;
