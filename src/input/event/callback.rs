//! Typed callback execution with validated parameter access.

use alloc::rc::Rc;
use core::marker::PhantomData;

use crate::core::model::{ModelHandle, registered_owner};
use crate::core::reactive::OwnerGuard;
use crate::ecs::system::access::{SystemAccess, validate};
use crate::ecs::system::param::{SystemParamSet, fetch_system_params, reserve_system_params};
use crate::ecs::world::WorldId;
use crate::ecs::{Component, Entity, World};

use super::contract::{
    AppEventSource, AppRoute, EntityRoute, EntitySourceFor, Event, EventResult, EventRoute,
};

mod sealed {
    pub trait Context<E> {}
}

/// Entity capability carried by a generated widget event adapter.
#[doc(hidden)]
pub struct EntityContext<W: Component>(PhantomData<fn() -> W>);

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
type Run<E, C> = dyn Fn(&mut World, &E, <C as CallbackContext<E>>::Target) -> Decision<E>;

/// Uninstalled callback description emitted by an event authoring macro.
#[doc(hidden)]
pub struct EventCallback<E: Event, C: CallbackContext<E>> {
    name: &'static str,
    metadata: &'static [SystemAccess],
    owner: Option<WorldId>,
    reserve_params: unsafe fn(&mut World),
    run: Rc<Run<E, C>>,
}

impl<E: Event, C: CallbackContext<E>> EventCallback<E, C> {
    pub fn require_model<H: ModelHandle>(mut self, model: &H) -> Self {
        let owner = registered_owner(model);
        if let Some(previous) = self.owner {
            assert_eq!(previous, owner, "event models belong to different Apps");
        }
        self.owner = Some(owner);
        self
    }

    pub fn prepare(self, world: &mut World) -> PreparedEvent<E, C> {
        if let Some(owner) = self.owner {
            assert_eq!(
                owner,
                world.id(),
                "event callback belongs to a different App"
            );
        }
        validate("event", self.name, self.metadata);
        let _owner = OwnerGuard::enter(world);
        // SAFETY: the immutable access plan was validated before this callback
        // parameter budget is prepared. No parameter references exist yet.
        unsafe { (self.reserve_params)(world) };
        PreparedEvent {
            owner: world.id(),
            run: self.run,
        }
    }
}

/// Validated callback with an immutable access plan and App identity.
#[doc(hidden)]
pub struct PreparedEvent<E: Event, C: CallbackContext<E>> {
    owner: WorldId,
    run: Rc<Run<E, C>>,
}

impl<E: Event, C: CallbackContext<E>> Clone for PreparedEvent<E, C> {
    fn clone(&self) -> Self {
        Self {
            owner: self.owner,
            run: Rc::clone(&self.run),
        }
    }
}

impl<E: Event, C: CallbackContext<E>> PreparedEvent<E, C> {
    /// Return no decision for a destroyed or generation-stale entity.
    pub fn invoke(&self, world: &mut World, payload: &E, target: C::Target) -> Option<Decision<E>> {
        assert_eq!(
            self.owner,
            world.id(),
            "event callback belongs to a different App"
        );
        if !C::is_live(world, target) {
            return None;
        }
        let _owner = OwnerGuard::enter(world);
        Some((self.run)(world, payload, target))
    }
}

/// Build a callback whose body only receives declared typed parameters.
///
/// # Safety
///
/// `metadata` must describe every component/resource access in `P`. Every
/// captured Model must be declared with `require_model` before preparation.
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
        owner: None,
        reserve_params: reserve_system_params::<P>,
        run: Rc::new(move |world, payload, target| {
            // SAFETY: prepare validated the complete immutable metadata before
            // this closure can run. The body cannot access World directly.
            let params = unsafe { fetch_system_params::<P>(world, name) };
            body(params, payload, target).into_decision()
        }),
    }
}

#[cfg(test)]
mod tests;
