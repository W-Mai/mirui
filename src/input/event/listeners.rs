//! Bounded listener installation and invocation snapshots.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::{RefCell, RefMut};
use core::marker::PhantomData;
use core::mem::size_of;
use core::ptr::NonNull;

use crate::ecs::commands::{CommandCapacity, CommandError};
use crate::ecs::world::WorldId;
use crate::ecs::{Component, Entity, World};

use super::BubbleControl;
use super::callback::{AppContext, CallbackContext, EntityContext, EventCallback, PreparedEvent};
use super::contract::{
    AppEventSource, AppRoute, AppSource, EntityRoute, EntitySourceFor, Event, EventKey, EventRoute,
    InputSource,
};
use super::focus::{FocusState, KeyHandler};
use super::gesture::GestureEvent;
use super::gesture::event::GesturePayload;
use super::input::{InputEvent, InputPayload};
use super::notification::{NotificationFlushError, NotificationQueue};
use crate::ui::view::{ViewInternalGesture, ViewRegistry};
use crate::ui::{Parent, UserState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerError {
    Busy,
    Capacity { available: usize },
    SnapshotCapacity { available: usize },
    AllocationFailed,
    IdentityExhausted,
    ForeignOwner,
    MissingSource { entity: Entity },
    CyclicRoute { entity: Entity },
    Command(CommandError),
}

impl From<CommandError> for ListenerError {
    fn from(error: CommandError) -> Self {
        Self::Command(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerTarget {
    App,
    Entity(Entity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListenerId {
    owner: WorldId,
    serial: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListenerCapacity {
    pub listeners: usize,
    pub snapshot_steps: usize,
}

/// A read-only event window; type erasure does not own or clone the payload.
pub(crate) struct BorrowedEvent<'event> {
    kind: TypeId,
    pointer: *const u8,
    lifetime: PhantomData<&'event ()>,
}

impl<'event> BorrowedEvent<'event> {
    fn new<E: Event>(event: &'event E) -> Self {
        Self {
            kind: TypeId::of::<E>(),
            pointer: core::ptr::from_ref(event).cast(),
            lifetime: PhantomData,
        }
    }

    /// # Safety
    /// `pointer` must retain a live payload of `kind` for `'event`, aligned
    /// for that payload unless it is zero-sized. ZST projections use an
    /// aligned dangling pointer instead of reading storage bytes.
    pub(crate) unsafe fn stored(kind: TypeId, pointer: *const u8) -> Self {
        Self {
            kind,
            pointer,
            lifetime: PhantomData,
        }
    }

    fn payload<E: Event>(&self) -> Option<&E> {
        if self.kind != TypeId::of::<E>() {
            return None;
        }
        let pointer = if size_of::<E>() == 0 {
            NonNull::<E>::dangling().as_ptr()
        } else {
            self.pointer.cast::<E>()
        };
        // SAFETY: constructors retain a live payload, the TypeId matches E,
        // and no owning or mutable reference is created by this projection.
        Some(unsafe { &*pointer })
    }
}

#[doc(hidden)]
pub trait ListenerDecision {
    fn control(self) -> BubbleControl;
}

impl ListenerDecision for () {
    fn control(self) -> BubbleControl {
        BubbleControl::Allow
    }
}

impl ListenerDecision for BubbleControl {
    fn control(self) -> BubbleControl {
        self
    }
}

trait ErasedListener {
    fn matches(&self, event: &BorrowedEvent<'_>, tap_count: Option<u8>) -> bool;
    fn invoke(
        &self,
        world: &mut World,
        event: &BorrowedEvent<'_>,
    ) -> Result<BubbleControl, CommandError>;
}

struct InstalledListener<E: Event, C: CallbackContext<E>> {
    callback: PreparedEvent<E, C>,
    target: C::Target,
    selector: ListenerSelector<E>,
}

pub struct ListenerSelector<E: Event> {
    key: EventKey<E>,
    matcher: OptionMatcher<E>,
}

enum OptionMatcher<E: Event> {
    Payload(fn(E::Options, &E) -> bool),
    Gesture(fn(E::Options, u8) -> bool),
}

impl<E: Event> ListenerSelector<E> {
    pub fn new(key: EventKey<E>, matches: fn(E::Options, &E) -> bool) -> Self {
        Self {
            key,
            matcher: OptionMatcher::Payload(matches),
        }
    }
}

impl<E: GesturePayload> ListenerSelector<E> {
    pub fn gesture(key: EventKey<E>) -> Self {
        Self {
            key,
            matcher: OptionMatcher::Gesture(E::matches_options),
        }
    }
}

impl<E: Event, C: CallbackContext<E>> ErasedListener for InstalledListener<E, C>
where
    <E::Route as EventRoute>::Decision: ListenerDecision,
{
    fn matches(&self, event: &BorrowedEvent<'_>, tap_count: Option<u8>) -> bool {
        event
            .payload::<E>()
            .is_some_and(|payload| match self.selector.matcher {
                OptionMatcher::Payload(matches) => matches(self.selector.key.options(), payload),
                OptionMatcher::Gesture(matches) => {
                    tap_count.is_some_and(|count| matches(self.selector.key.options(), count))
                }
            })
    }

    fn invoke(
        &self,
        world: &mut World,
        event: &BorrowedEvent<'_>,
    ) -> Result<BubbleControl, CommandError> {
        let payload = event
            .payload::<E>()
            .expect("listener snapshot preserves event identity");
        Ok(self
            .callback
            .invoke(world, payload, self.target)?
            .map_or(BubbleControl::Allow, ListenerDecision::control))
    }
}

struct ListenerEntry {
    id: ListenerId,
    kind: TypeId,
    target: ListenerTarget,
    callback: Rc<dyn ErasedListener>,
}

#[derive(Default)]
struct Entries {
    listeners: Vec<ListenerEntry>,
    limit: usize,
    next: u64,
}

enum DispatchStep {
    Internal {
        entity: Entity,
        callback: ViewInternalGesture,
    },
    Keyboard {
        entity: Entity,
        callback: fn(&mut World, Entity, &InputEvent) -> bool,
    },
    Listener(Rc<dyn ErasedListener>),
}

enum InternalEvent {
    None,
    Gesture(GestureEvent),
    Keyboard(InputEvent),
}

fn route_allowed(world: &World, mut entity: Entity) -> Result<bool, ListenerError> {
    let origin = entity;
    let mut remaining = world.live_entity_count();
    while world.is_alive(entity) {
        if remaining == 0 {
            return Err(ListenerError::CyclicRoute { entity });
        }
        remaining -= 1;
        if crate::ui::branch::is_effectively_hidden(world, entity)
            || matches!(world.get::<UserState>(entity), Some(UserState::Disabled))
        {
            return Ok(false);
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return Ok(true);
        };
        entity = parent.0;
    }
    Ok(world.is_alive(origin))
}

#[derive(Default)]
struct Scratch {
    steps: Vec<DispatchStep>,
    limit: usize,
}

/// One installation pool shared by typed input and notification listeners.
pub struct EventListeners {
    owner: WorldId,
    entries: RefCell<Entries>,
    scratch: RefCell<Scratch>,
}

impl EventListeners {
    pub fn new(world: &mut World) -> Self {
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
        Self {
            owner: world.id(),
            entries: RefCell::new(Entries::default()),
            scratch: RefCell::new(Scratch::default()),
        }
    }

    pub fn reserve(&self, capacity: ListenerCapacity) -> Result<(), ListenerError> {
        let mut scratch = self
            .scratch
            .try_borrow_mut()
            .map_err(|_| ListenerError::Busy)?;
        let mut entries = self
            .entries
            .try_borrow_mut()
            .map_err(|_| ListenerError::Busy)?;
        let requested = capacity.listeners.max(entries.limit);
        let steps = capacity.snapshot_steps.max(scratch.limit);
        let additional = requested.saturating_sub(entries.listeners.len());
        entries
            .listeners
            .try_reserve_exact(additional)
            .map_err(|_| ListenerError::AllocationFailed)?;
        scratch
            .steps
            .try_reserve_exact(steps)
            .map_err(|_| ListenerError::AllocationFailed)?;
        entries.limit = requested;
        scratch.limit = steps;
        Ok(())
    }

    fn check_installation(&self, world: &World) -> Result<(), ListenerError> {
        self.check_owner(world)?;
        self.prune_retired_entities(world)?;
        let entries = self.entries.try_borrow().map_err(|_| ListenerError::Busy)?;
        if entries.listeners.len() == entries.limit {
            return Err(ListenerError::Capacity {
                available: entries.limit,
            });
        }
        entries
            .next
            .checked_add(1)
            .ok_or(ListenerError::IdentityExhausted)?;
        Ok(())
    }

    fn prune_retired_entities(&self, world: &World) -> Result<(), ListenerError> {
        loop {
            let removed = {
                let mut entries = self
                    .entries
                    .try_borrow_mut()
                    .map_err(|_| ListenerError::Busy)?;
                let Some(index) = entries.listeners.iter().position(|entry| {
                    matches!(entry.target, ListenerTarget::Entity(entity) if !world.is_alive(entity))
                }) else {
                    return Ok(());
                };
                entries.listeners.remove(index)
            };
            drop(removed);
        }
    }

    fn check_owner(&self, world: &World) -> Result<(), ListenerError> {
        if self.owner != world.id() {
            return Err(ListenerError::ForeignOwner);
        }
        Ok(())
    }

    fn insert<E: Event>(
        &self,
        target: ListenerTarget,
        listener: impl ErasedListener + 'static,
    ) -> ListenerId {
        let mut entries = self.entries.borrow_mut();
        let serial = entries.next + 1;
        entries.next = serial;
        let id = ListenerId {
            owner: self.owner,
            serial,
        };
        entries.listeners.push(ListenerEntry {
            id,
            kind: TypeId::of::<E>(),
            target,
            callback: Rc::new(listener),
        });
        id
    }

    pub fn install_entity<E, W>(
        &self,
        world: &mut World,
        entity: Entity,
        selector: ListenerSelector<E>,
        callback: EventCallback<E, EntityContext<W>>,
        capacity: CommandCapacity,
    ) -> Result<ListenerId, ListenerError>
    where
        E: Event,
        W: Component,
        E::Route: EntityRoute,
        E::Source: EntitySourceFor<W>,
        <E::Route as EventRoute>::Decision: ListenerDecision,
    {
        self.check_installation(world)?;
        if !world.is_alive(entity) || !E::Source::present(world, entity) {
            return Err(ListenerError::MissingSource { entity });
        }
        let callback = callback.prepare(world, capacity)?;
        Ok(self.insert::<E>(
            ListenerTarget::Entity(entity),
            InstalledListener {
                callback,
                target: entity,
                selector,
            },
        ))
    }

    pub fn install_app<E>(
        &self,
        world: &mut World,
        selector: ListenerSelector<E>,
        callback: EventCallback<E, AppContext>,
        capacity: CommandCapacity,
    ) -> Result<ListenerId, ListenerError>
    where
        E: Event,
        E::Route: AppRoute,
        E::Source: AppEventSource,
        <E::Route as EventRoute>::Decision: ListenerDecision,
    {
        self.check_installation(world)?;
        let callback = callback.prepare(world, capacity)?;
        Ok(self.insert::<E>(
            ListenerTarget::App,
            InstalledListener {
                callback,
                target: (),
                selector,
            },
        ))
    }

    pub fn remove(&self, id: ListenerId) -> Result<bool, ListenerError> {
        if self.owner != id.owner {
            return Err(ListenerError::ForeignOwner);
        }
        let removed = {
            let mut entries = self
                .entries
                .try_borrow_mut()
                .map_err(|_| ListenerError::Busy)?;
            let Some(index) = entries.listeners.iter().position(|entry| entry.id == id) else {
                return Ok(false);
            };
            entries.listeners.remove(index)
        };
        drop(removed);
        Ok(true)
    }

    fn empty_snapshot<'pool, 'event>(
        &'pool self,
        world: &World,
        event: BorrowedEvent<'event>,
        internal: InternalEvent,
    ) -> Result<ListenerSnapshot<'pool, 'event>, ListenerError> {
        self.check_owner(world)?;
        let scratch = self
            .scratch
            .try_borrow_mut()
            .map_err(|_| ListenerError::Busy)?;
        debug_assert!(scratch.steps.is_empty());
        self.prune_retired_entities(world)?;
        Ok(ListenerSnapshot {
            owner: self.owner,
            scratch,
            event,
            internal,
        })
    }

    fn snapshot_borrowed<'pool, 'event>(
        &'pool self,
        world: &World,
        event: BorrowedEvent<'event>,
        target: ListenerTarget,
    ) -> Result<ListenerSnapshot<'pool, 'event>, ListenerError> {
        let mut snapshot = self.empty_snapshot(world, event, InternalEvent::None)?;
        snapshot.select_listeners(&self.entries, target, None)?;
        Ok(snapshot)
    }

    pub fn snapshot_gesture<'pool, 'event, E>(
        &'pool self,
        world: &World,
        event: &'event E,
    ) -> Result<ListenerSnapshot<'pool, 'event>, ListenerError>
    where
        E: GesturePayload + Copy + Into<GestureEvent>,
    {
        let gesture = (*event).into();
        let mut current = gesture.target();
        let mut snapshot = self.empty_snapshot(
            world,
            BorrowedEvent::new(event),
            InternalEvent::Gesture(gesture),
        )?;
        if !route_allowed(world, current)? {
            return Ok(snapshot);
        }
        let tap_count = super::multi_tap::current_count(world, current);
        while world.is_alive(current) {
            if let Some(registry) = world.resource::<ViewRegistry>() {
                for view in registry.iter() {
                    if let Some(callback) = view.internal_gesture()
                        && view
                            .component_filter()
                            .is_none_or(|kind| world.has_type(current, kind))
                    {
                        snapshot.push(DispatchStep::Internal {
                            entity: current,
                            callback,
                        })?;
                    }
                }
            }
            snapshot.select_listeners(
                &self.entries,
                ListenerTarget::Entity(current),
                Some(tap_count),
            )?;
            let Some(parent) = world.get::<Parent>(current) else {
                break;
            };
            current = parent.0;
        }
        Ok(snapshot)
    }

    pub fn snapshot_keyboard<'pool, 'event, E>(
        &'pool self,
        world: &World,
        event: &'event E,
    ) -> Result<ListenerSnapshot<'pool, 'event>, ListenerError>
    where
        E: InputPayload + Event<Source = InputSource> + Copy + Into<InputEvent>,
    {
        let mut snapshot = self.empty_snapshot(
            world,
            BorrowedEvent::new(event),
            InternalEvent::Keyboard((*event).into()),
        )?;
        let focused = world
            .resource::<FocusState>()
            .and_then(|state| state.focused);
        if let Some(mut current) = focused
            && route_allowed(world, current)?
        {
            while world.is_alive(current) {
                snapshot.select_listeners(&self.entries, ListenerTarget::Entity(current), None)?;
                if let Some(handler) = world.get::<KeyHandler>(current) {
                    snapshot.push(DispatchStep::Keyboard {
                        entity: current,
                        callback: handler.on_key,
                    })?;
                }
                let Some(parent) = world.get::<Parent>(current) else {
                    break;
                };
                current = parent.0;
            }
        }
        snapshot.select_listeners(&self.entries, ListenerTarget::App, None)?;
        Ok(snapshot)
    }

    pub fn snapshot<'pool, 'event, E: Event>(
        &'pool self,
        world: &World,
        event: &'event E,
        target: ListenerTarget,
    ) -> Result<ListenerSnapshot<'pool, 'event>, ListenerError> {
        self.snapshot_borrowed(world, BorrowedEvent::new(event), target)
    }

    pub fn dispatch_scroll<E>(
        &self,
        world: &mut World,
        root: Entity,
        event: &E,
        width: u16,
        height: u16,
    ) -> Result<BubbleControl, ListenerError>
    where
        E: InputPayload + Event<Source = AppSource> + Copy + Into<InputEvent>,
    {
        let mut snapshot =
            self.empty_snapshot(world, BorrowedEvent::new(event), InternalEvent::None)?;
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
        if super::scroll::scroll_system_with_target(
            world,
            root,
            &(*event).into(),
            width,
            height,
            None,
        ) {
            return Ok(BubbleControl::Prevent);
        }
        snapshot.select_listeners(&self.entries, ListenerTarget::App, None)?;
        snapshot.run(world)
    }

    pub fn flush_notifications(
        &self,
        world: &mut World,
        queue: &NotificationQueue,
        limit: usize,
    ) -> Result<usize, NotificationFlushError<ListenerError>> {
        self.check_owner(world)
            .map_err(NotificationFlushError::Listener)?;
        queue.flush(limit, |notification| {
            let target = notification
                .entity()
                .map_or(ListenerTarget::App, ListenerTarget::Entity);
            self.snapshot_borrowed(world, notification.borrowed_event(), target)?
                .run(world)?;
            Ok(())
        })
    }
}

/// A fixed installation-order selection, independent of later entry changes.
pub struct ListenerSnapshot<'pool, 'event> {
    owner: WorldId,
    scratch: RefMut<'pool, Scratch>,
    event: BorrowedEvent<'event>,
    internal: InternalEvent,
}

impl ListenerSnapshot<'_, '_> {
    fn push(&mut self, step: DispatchStep) -> Result<(), ListenerError> {
        if self.scratch.steps.len() == self.scratch.limit {
            return Err(ListenerError::SnapshotCapacity {
                available: self.scratch.limit,
            });
        }
        self.scratch.steps.push(step);
        Ok(())
    }

    fn select_listeners(
        &mut self,
        entries: &RefCell<Entries>,
        target: ListenerTarget,
        tap_count: Option<u8>,
    ) -> Result<(), ListenerError> {
        let entries = entries.try_borrow().map_err(|_| ListenerError::Busy)?;
        for entry in &entries.listeners {
            if entry.kind == self.event.kind
                && entry.target == target
                && entry.callback.matches(&self.event, tap_count)
            {
                self.push(DispatchStep::Listener(Rc::clone(&entry.callback)))?;
            }
        }
        Ok(())
    }

    pub fn run(self, world: &mut World) -> Result<BubbleControl, ListenerError> {
        if self.owner != world.id() {
            return Err(ListenerError::ForeignOwner);
        }
        let _owner_scope = crate::core::reactive::OwnerGuard::enter(world);
        for step in &self.scratch.steps {
            let result = match step {
                DispatchStep::Listener(callback) => callback.invoke(world, &self.event)?,
                DispatchStep::Internal { entity, callback } => {
                    if !world.is_alive(*entity) {
                        continue;
                    }
                    let InternalEvent::Gesture(gesture) = &self.internal else {
                        unreachable!("gesture steps retain their carrier");
                    };
                    if callback(world, *entity, gesture) {
                        BubbleControl::Prevent
                    } else {
                        BubbleControl::Allow
                    }
                }
                DispatchStep::Keyboard { entity, callback } => {
                    if !world.is_alive(*entity) {
                        continue;
                    }
                    let InternalEvent::Keyboard(input) = &self.internal else {
                        unreachable!("keyboard steps retain their carrier");
                    };
                    if callback(world, *entity, input) {
                        BubbleControl::Prevent
                    } else {
                        BubbleControl::Allow
                    }
                }
            };
            if result == BubbleControl::Prevent {
                return Ok(result);
            }
        }
        Ok(BubbleControl::Allow)
    }
}

impl Drop for ListenerSnapshot<'_, '_> {
    fn drop(&mut self) {
        self.scratch.steps.clear();
    }
}

#[cfg(test)]
mod tests;
