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

    fn resolve_target(target: super::listeners::ListenerTarget) -> Option<Self::Target>;

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

    fn resolve_target(target: super::listeners::ListenerTarget) -> Option<Entity> {
        match target {
            super::listeners::ListenerTarget::Entity(entity) => Some(entity),
            super::listeners::ListenerTarget::App => None,
        }
    }

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

    fn resolve_target(target: super::listeners::ListenerTarget) -> Option<Entity> {
        match target {
            super::listeners::ListenerTarget::Entity(entity) => Some(entity),
            super::listeners::ListenerTarget::App => None,
        }
    }

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

    fn resolve_target(target: super::listeners::ListenerTarget) -> Option<()> {
        match target {
            super::listeners::ListenerTarget::App => Some(()),
            super::listeners::ListenerTarget::Entity(_) => None,
        }
    }

    fn is_live(_world: &World, (): ()) -> bool {
        true
    }
}

type Decision<E> = <<E as Event>::Route as EventRoute>::Decision;
type Scope<'world, E> = <<E as Event>::Route as EventRoute>::Scope<'world>;

/// Generated invocation capability. Construction cannot retain ECS parameter borrows.
#[doc(hidden)]
pub trait CallbackInvocation<P: SystemParamSet, T>: EventRoute {
    /// # Safety
    /// The complete parameter access plan must have been validated before invocation.
    unsafe fn invoke<R: EventResult<Self>>(
        world: &mut World,
        target: T,
        name: &'static str,
        body: impl for<'world> FnOnce(P::Item<'world>, &mut Self::Scope<'world>) -> R,
    ) -> Self::Decision
    where
        Self: Sized;
}

macro_rules! borrowed_invocation {
    ($($route:ty),+ $(,)?) => {$(
        impl<P: SystemParamSet, T> CallbackInvocation<P, T> for $route {
            unsafe fn invoke<R: EventResult<Self>>(
                world: &mut World,
                _target: T,
                name: &'static str,
                body: impl for<'world> FnOnce(P::Item<'world>, &mut ()) -> R,
            ) -> Self::Decision {
                // SAFETY: the caller validated the complete parameter access plan.
                body(unsafe { fetch_system_params::<P>(world, name) }, &mut ()).into_decision()
            }
        }
    )+};
}

borrowed_invocation!(
    super::contract::EntityBubble,
    super::contract::EntityNotification,
    super::contract::LayoutNotification,
    super::contract::Initialization,
    super::contract::InputFallback,
    super::contract::AppNotification,
);

struct ConstructionGuard<'world> {
    world: &'world mut World,
    parent: Entity,
    previous_children: usize,
    accepted: bool,
}

impl Drop for ConstructionGuard<'_> {
    fn drop(&mut self) {
        if self.accepted {
            return;
        }
        while let Some(entity) = self
            .world
            .get_mut::<crate::ui::Children>(self.parent)
            .and_then(|children| {
                if children.0.len() > self.previous_children {
                    children.0.pop()
                } else {
                    None
                }
            })
        {
            crate::ui::despawn_subtree(self.world, entity);
        }
    }
}

impl CallbackInvocation<(), Entity> for super::contract::WidgetConstruction {
    unsafe fn invoke<R: EventResult<Self>>(
        world: &mut World,
        target: Entity,
        _name: &'static str,
        body: impl for<'world> FnOnce((), &mut crate::ui::UiScope<'world>) -> R,
    ) -> Option<Entity> {
        use crate::ui::{Children, Parent};
        let previous_children = world
            .get::<Children>(target)
            .map_or(0, |children| children.0.len());
        let mut construction = ConstructionGuard {
            world,
            parent: target,
            previous_children,
            accepted: false,
        };
        let result =
            body((), &mut crate::ui::UiScope::new(construction.world, target)).into_decision();
        let valid = result.filter(|&entity| {
            construction.world.is_alive(target)
                && construction
                    .world
                    .get::<Parent>(entity)
                    .is_some_and(|parent| parent.0 == target)
                && construction
                    .world
                    .get::<Children>(target)
                    .is_some_and(|children| {
                        children.0.len() == previous_children + 1
                            && children.0.last() == Some(&entity)
                    })
        });
        if valid.is_none() {
            crate::warn!(target: "mirui::event::construction", "creation requires one new child root");
        }
        construction.accepted = valid.is_some();
        valid
    }
}

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
    E::Route: CallbackInvocation<(), C::Target>,
{
    fn into_callback(self, name: &'static str) -> EventCallback<E, C> {
        // SAFETY: no World parameters are fetched; generated bindings record
        // declared Model requirements, while ordinary captures remain opaque.
        unsafe { event_callback::<E, C, (), R>(name, &[], move |(), event, _, _| self(event)) }
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
        body: impl for<'event, 'world> Fn(&'event E, &mut Scope<'world, E>) -> R + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        R: EventResult<E::Route>,
        E::Route: CallbackInvocation<(), C::Target>,
    {
        // SAFETY: the macro records all declared Model bindings on the result.
        unsafe {
            event_callback::<E, C, (), R>(name, &[], move |(), event, _, scope| body(event, scope))
        }
    }

    /// Anchor a generated deferred payload closure to this selector.
    ///
    /// # Safety
    /// Model bindings declared through framework roles or generated factories must record their ownership requirements before installation.
    #[doc(hidden)]
    pub unsafe fn __bind_deferred<C, R>(
        self,
        name: &'static str,
        body: impl for<'event, 'commands, 'world> Fn(
            &'event E,
            &mut Scope<'world, E>,
            DeferredCommands<'commands>,
        ) -> R
        + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        R: EventResult<E::Route>,
        E::Route: CallbackInvocation<(), C::Target>,
    {
        // SAFETY: the macro records all declared Model bindings on the result.
        unsafe {
            deferred_event_callback::<E, C, (), R>(
                name,
                &[],
                move |(), event, _, scope, commands| body(event, scope, commands),
            )
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
        body: impl for<'world, 'event> Fn(
            P::Item<'world>,
            &'event E,
            C::Target,
            &mut Scope<'world, E>,
        ) -> R
        + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        P: SystemParamSet + 'static,
        E::Route: CallbackInvocation<P, C::Target>,
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
            &mut Scope<'world, E>,
            DeferredCommands<'commands>,
        ) -> R
        + 'static,
    ) -> EventCallback<E, C>
    where
        C: CallbackContext<E>,
        P: SystemParamSet + 'static,
        E::Route: CallbackInvocation<P, C::Target>,
        R: EventResult<E::Route>,
    {
        // SAFETY: delegated from the generated invocation plan.
        unsafe { deferred_event_callback::<E, C, P, R>(name, metadata, body) }
    }
}
type Run<E, C> = dyn Fn(&mut World, &E, <C as CallbackContext<E>>::Target) -> Option<Decision<E>>;
type DeferredRun<E, C> = dyn for<'commands> Fn(
    &mut World,
    &E,
    <C as CallbackContext<E>>::Target,
    DeferredCommands<'commands>,
) -> Option<Decision<E>>;

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
///
/// Declared bindings are retained for invocation-time validation. Expired Model bindings and collection iteration conflicts warn and skip the callback before parameter access. Binding checks allocate during callback construction and reuse their storage during execution; ordinary opaque captures are not inspected.
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

    pub fn require_binding<T: SharedValue + 'static>(mut self, value: &T) -> Self {
        self.owner = self.owner.merge(value.binding_owner());
        if self.owner.is_invalid() {
            return self;
        }
        let value = value.clone();
        let name = self.name;
        let ready = move |world: &World| {
            if let Err(error) = value.binding_owner().check(world.id()) {
                crate::warn!(target: "mirui::event", "callback {name} skipped: {error:?}");
                false
            } else if !value.callback_ready() {
                crate::warn!(target: "mirui::event", "callback {name} skipped: model is being iterated");
                false
            } else {
                true
            }
        };
        self.run = match self.run {
            CallbackRun::Plain(run) => {
                CallbackRun::Plain(Rc::new(move |world, payload, target| {
                    if ready(world) {
                        run(world, payload, target)
                    } else {
                        None
                    }
                }))
            }
            CallbackRun::Deferred { run, commands } => CallbackRun::Deferred {
                run: Rc::new(move |world, payload, target, commands| {
                    if ready(world) {
                        run(world, payload, target, commands)
                    } else {
                        None
                    }
                }),
                commands,
            },
        };
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
                if let Err(error) = batch.finish(world) {
                    if let Some(decision) = decision {
                        E::Route::discard(world, decision);
                    }
                    return Err(error);
                }
                decision
            }
        };
        Ok(decision)
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
    body: impl for<'world, 'event> Fn(P::Item<'world>, &'event E, C::Target, &mut Scope<'world, E>) -> R
    + 'static,
) -> EventCallback<E, C>
where
    E: Event,
    C: CallbackContext<E>,
    P: SystemParamSet + 'static,
    E::Route: CallbackInvocation<P, C::Target>,
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
            Some(unsafe {
                E::Route::invoke::<R>(world, target, name, |params, scope| {
                    body(params, payload, target, scope)
                })
            })
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
        &mut Scope<'world, E>,
        DeferredCommands<'commands>,
    ) -> R
    + 'static,
) -> EventCallback<E, C>
where
    E: Event,
    C: CallbackContext<E>,
    P: SystemParamSet + 'static,
    E::Route: CallbackInvocation<P, C::Target>,
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
                Some(unsafe {
                    E::Route::invoke::<R>(world, target, name, |params, scope| {
                        body(params, payload, target, scope, commands)
                    })
                })
            }),
        },
    }
}

#[cfg(test)]
mod tests;
