//! Typed callback execution with validated parameter access.

use alloc::rc::Rc;
use core::cell::RefCell;
use core::marker::PhantomData;

use crate::core::model::{BindingOwner, ModelOwnerError, SharedValue};
use crate::core::reactive::OwnerGuard;
use crate::ecs::commands::{CommandCapacity, CommandError, CommandQueue, DeferredCommands};
use crate::ecs::system::access::{AccessError, SystemAccess, check};
use crate::ecs::system::param::{SystemParamSet, fetch_system_params, reserve_system_params};
use crate::ecs::world::WorldId;
use crate::ecs::{Component, Entity, World};

use super::contract::{
    AppEventSource, AppRoute, EntityEventSource, EntityRoute, EntitySourceFor, Event, EventKey,
    EventResult, EventRoute,
};

mod sealed {
    pub trait Context<E> {}
}

/// Entity capability carried by a generated widget event adapter.
#[doc(hidden)]
pub struct EntityContext<W: Component>(PhantomData<fn() -> W>);

/// Entity capability whose component source is checked after attachment.
#[doc(hidden)]
pub struct NodeContext;

/// App capability carried by a generated App event adapter.
#[doc(hidden)]
pub struct AppContext;

/// Installation context for a concrete event payload.
#[doc(hidden)]
pub trait CallbackContext<E: Event>: sealed::Context<E> + 'static {
    type Target: Copy;

    fn is_live(world: &World, target: Self::Target) -> bool;
}

impl<E: Event, W: Component> sealed::Context<E> for EntityContext<W>
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<W>,
{
}

impl<E: Event, W: Component> CallbackContext<E> for EntityContext<W>
where
    E::Route: EntityRoute,
    E::Source: EntitySourceFor<W>,
{
    type Target = Entity;

    fn is_live(world: &World, target: Entity) -> bool {
        world.is_alive(target)
    }
}

impl<E: Event> sealed::Context<E> for NodeContext
where
    E::Route: EntityRoute,
    E::Source: EntityEventSource,
{
}

impl<E: Event> CallbackContext<E> for NodeContext
where
    E::Route: EntityRoute,
    E::Source: EntityEventSource,
{
    type Target = Entity;

    fn is_live(world: &World, target: Entity) -> bool {
        world.is_alive(target)
    }
}

impl<E: Event> sealed::Context<E> for AppContext
where
    E::Route: AppRoute,
    E::Source: AppEventSource,
{
}

impl<E: Event> CallbackContext<E> for AppContext
where
    E::Route: AppRoute,
    E::Source: AppEventSource,
{
    type Target = ();

    fn is_live(_world: &World, (): ()) -> bool {
        true
    }
}

type Decision<E> = <<E as Event>::Route as EventRoute>::Decision;

/// Typed callback values accepted by generated event bindings.
#[doc(hidden)]
pub trait CallbackValue<E: Event, C: CallbackContext<E>> {
    fn into_callback(self, name: &'static str) -> EventCallback<E, C>;
}

impl<E: Event, C: CallbackContext<E>> CallbackValue<E, C> for EventCallback<E, C> {
    fn into_callback(self, _name: &'static str) -> EventCallback<E, C> {
        self
    }
}

impl<E, C, F, R> CallbackValue<E, C> for F
where
    E: Event,
    C: CallbackContext<E>,
    F: for<'event> Fn(&'event E) -> R + 'static,
    R: EventResult<E::Route>,
{
    fn into_callback(self, name: &'static str) -> EventCallback<E, C> {
        // SAFETY: no World parameters are fetched; generated bindings record
        // declared Model requirements, while ordinary captures remain opaque.
        unsafe { event_callback::<E, C, (), R>(name, &[], move |(), event, _| self(event)) }
    }
}

impl<E: Event> EventKey<E> {
    /// Preserve a descriptor or bind an already typed payload callback.
    #[doc(hidden)]
    pub fn __bind_callback<C, V>(self, name: &'static str, value: V) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        V: CallbackValue<E, C>,
    {
        value.into_callback(name)
    }

    /// Anchor a generated payload closure to this selector's event type.
    ///
    /// # Safety
    /// Model bindings declared through framework roles or generated factories must record their ownership requirements before installation.
    #[doc(hidden)]
    pub unsafe fn __bind_plain<C, R>(
        self,
        name: &'static str,
        body: impl for<'event> Fn(&'event E) -> R + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        R: EventResult<E::Route>,
    {
        // SAFETY: the macro records all declared Model bindings on the result.
        unsafe { event_callback::<E, C, (), R>(name, &[], move |(), event, _| body(event)) }
    }

    /// Anchor a generated deferred payload closure to this selector.
    ///
    /// # Safety
    /// Model bindings declared through framework roles or generated factories must record their ownership requirements before installation.
    #[doc(hidden)]
    pub unsafe fn __bind_deferred<C, R>(
        self,
        name: &'static str,
        body: impl for<'event, 'commands> Fn(&'event E, DeferredCommands<'commands>) -> R + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        R: EventResult<E::Route>,
    {
        // SAFETY: the macro records all declared Model bindings on the result.
        unsafe {
            deferred_event_callback::<E, C, (), R>(name, &[], move |(), event, _, commands| {
                body(event, commands)
            })
        }
    }

    /// Build a generated closure with a complete typed invocation plan.
    ///
    /// # Safety
    /// The metadata must describe every fetched parameter. Declared Model bindings must record their ownership requirements before installation.
    #[doc(hidden)]
    pub unsafe fn __bind_params<C, P, R>(
        self,
        name: &'static str,
        metadata: &'static [SystemAccess],
        body: impl for<'world, 'event> Fn(P::Item<'world>, &'event E, C::Target) -> R + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        P: SystemParamSet + 'static,
        R: EventResult<E::Route>,
    {
        // SAFETY: delegated from the generated invocation plan.
        unsafe { event_callback::<E, C, P, R>(name, metadata, body) }
    }

    /// Build a generated closure with typed parameters and deferred commands.
    ///
    /// # Safety
    /// The metadata and declared Model requirements must cover the complete generated invocation plan.
    #[doc(hidden)]
    pub unsafe fn __bind_params_deferred<C, P, R>(
        self,
        name: &'static str,
        metadata: &'static [SystemAccess],
        body: impl for<'world, 'event, 'commands> Fn(
            P::Item<'world>,
            &'event E,
            C::Target,
            DeferredCommands<'commands>,
        ) -> R
        + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        P: SystemParamSet + 'static,
        R: EventResult<E::Route>,
    {
        // SAFETY: delegated from the generated invocation plan.
        unsafe { deferred_event_callback::<E, C, P, R>(name, metadata, body) }
    }
}
type Run<E, C> = dyn Fn(&mut World, &E, <C as CallbackContext<E>>::Target) -> Decision<E>;
type DeferredRun<E, C> = dyn for<'commands> Fn(
    &mut World,
    &E,
    <C as CallbackContext<E>>::Target,
    DeferredCommands<'commands>,
) -> Decision<E>;

enum CallbackRun<E: Event, C: CallbackContext<E>, D> {
    Plain(Rc<Run<E, C>>),
    Deferred {
        run: Rc<DeferredRun<E, C>>,
        commands: D,
    },
}

impl<E: Event, C: CallbackContext<E>, D: Clone> Clone for CallbackRun<E, C, D> {
    fn clone(&self) -> Self {
        match self {
            Self::Plain(run) => Self::Plain(Rc::clone(run)),
            Self::Deferred { run, commands } => Self::Deferred {
                run: Rc::clone(run),
                commands: commands.clone(),
            },
        }
    }
}

/// Uninstalled callback description emitted by an event authoring macro.
#[doc(hidden)]
pub struct EventCallback<E: Event, C: CallbackContext<E>> {
    name: &'static str,
    metadata: &'static [SystemAccess],
    owner: BindingOwner,
    reserve_params: unsafe fn(&mut World),
    run: CallbackRun<E, C, ()>,
}

/// A callback rejected before parameter preparation or body execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum PreparationError {
    Owner {
        callback: &'static str,
        error: ModelOwnerError,
    },
    Access {
        callback: &'static str,
        error: AccessError,
    },
    Command {
        callback: &'static str,
        error: CommandError,
    },
}

impl<E: Event, C: CallbackContext<E>> EventCallback<E, C> {
    pub(crate) fn uses_commands(&self) -> bool {
        matches!(self.run, CallbackRun::Deferred { .. })
    }

    pub fn require_binding<T: SharedValue>(mut self, value: &T) -> Self {
        self.owner = self.owner.merge(value.binding_owner());
        self
    }

    pub fn prepare(
        self,
        world: &mut World,
        capacity: CommandCapacity,
    ) -> Result<PreparedEvent<E, C>, PreparationError> {
        self.owner
            .check(world.id())
            .map_err(|error| PreparationError::Owner {
                callback: self.name,
                error,
            })?;
        check(self.metadata).map_err(|error| PreparationError::Access {
            callback: self.name,
            error,
        })?;
        let run = match self.run {
            CallbackRun::Plain(run) => CallbackRun::Plain(run),
            CallbackRun::Deferred { run, commands: () } => CallbackRun::Deferred {
                run,
                commands: Rc::new(RefCell::new(CommandQueue::bounded(capacity).map_err(
                    |error| PreparationError::Command {
                        callback: self.name,
                        error,
                    },
                )?)),
            },
        };
        let _owner = OwnerGuard::enter(world);
        // SAFETY: the immutable access plan was validated before this callback
        // parameter budget is prepared. No parameter references exist yet.
        unsafe { (self.reserve_params)(world) };
        Ok(PreparedEvent {
            owner: world.id(),
            run,
        })
    }
}

/// Validated callback with an immutable access plan and App identity.
#[doc(hidden)]
pub struct PreparedEvent<E: Event, C: CallbackContext<E>> {
    owner: WorldId,
    run: CallbackRun<E, C, Rc<RefCell<CommandQueue>>>,
}

impl<E: Event, C: CallbackContext<E>> Clone for PreparedEvent<E, C> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner,
            run: self.run.clone(),
        }
    }
}

impl<E: Event, C: CallbackContext<E>> PreparedEvent<E, C> {
    /// Return no decision for a destroyed or generation-stale entity.
    pub fn invoke(
        &self,
        world: &mut World,
        payload: &E,
        target: C::Target,
    ) -> Result<Option<Decision<E>>, CommandError> {
        assert_eq!(
            self.owner,
            world.id(),
            "event callback belongs to a different App"
        );
        if !C::is_live(world, target) {
            return Ok(None);
        }
        let _owner = OwnerGuard::enter(world);
        let decision = match &self.run {
            CallbackRun::Plain(run) => run(world, payload, target),
            CallbackRun::Deferred { run, commands } => {
                let mut queue = commands.try_borrow_mut().map_err(|_| CommandError::Busy)?;
                let mut batch = queue.begin();
                let decision = run(world, payload, target, batch.commands());
                batch.finish(world)?;
                decision
            }
        };
        Ok(Some(decision))
    }
}

/// Build a callback whose body only receives declared typed parameters.
///
/// # Safety
///
/// `metadata` must describe every component/resource access in `P`.
/// Model bindings declared through framework roles or generated factories must record their ownership requirements with `require_binding` before preparation.
/// The body must not obtain another World window through an external pointer.
#[doc(hidden)]
pub unsafe fn event_callback<E, C, P, R>(
    name: &'static str,
    metadata: &'static [SystemAccess],
    body: impl for<'world, 'event> Fn(P::Item<'world>, &'event E, C::Target) -> R + 'static,
) -> EventCallback<E, C>
where
    E: Event,
    C: CallbackContext<E>,
    P: SystemParamSet + 'static,
    R: EventResult<E::Route>,
{
    EventCallback {
        name,
        metadata,
        owner: BindingOwner::unbound(),
        reserve_params: reserve_system_params::<P>,
        run: CallbackRun::Plain(Rc::new(move |world, payload, target| {
            // SAFETY: prepare validated the complete immutable metadata before
            // this closure can run. The body cannot access World directly.
            let params = unsafe { fetch_system_params::<P>(world, name) };
            body(params, payload, target).into_decision()
        })),
    }
}

/// Build a typed callback requiring bounded command storage at installation.
///
/// # Safety
///
/// The same access and declared-Model requirements as [`event_callback`] apply.
#[doc(hidden)]
pub unsafe fn deferred_event_callback<E, C, P, R>(
    name: &'static str,
    metadata: &'static [SystemAccess],
    body: impl for<'world, 'event, 'commands> Fn(
        P::Item<'world>,
        &'event E,
        C::Target,
        DeferredCommands<'commands>,
    ) -> R
    + 'static,
) -> EventCallback<E, C>
where
    E: Event,
    C: CallbackContext<E>,
    P: SystemParamSet + 'static,
    R: EventResult<E::Route>,
{
    EventCallback {
        name,
        metadata,
        owner: BindingOwner::unbound(),
        reserve_params: reserve_system_params::<P>,
        run: CallbackRun::Deferred {
            commands: (),
            run: Rc::new(move |world, payload, target, commands| {
                // SAFETY: prepare validated the immutable complete access plan.
                // These references are released when the body returns, before the
                // executor commits any recorded structural changes.
                let params = unsafe { fetch_system_params::<P>(world, name) };
                body(params, payload, target, commands).into_decision()
            }),
        },
    }
}

#[cfg(test)]
mod tests;
