use alloc::rc::Rc;
use core::any::{Any, TypeId};
use core::cell::Cell;

use crate::core::reactive::retained::InputLease;
use crate::ecs::{Component, Entity, World};
use crate::ui::view::ViewRegistry;

use super::callback::{CallbackContext, EventCallback, PreparedEvent};
use super::contract::{EntityEventSource, EntityRequest, Event, EventRoute, RequestError};
use super::runtime::{RuntimeInstallError, freeze_shared_callbacks, prepare_shared_callback};
use super::view_input::InputState;

type Decision<E> = <<E as Event>::Route as EventRoute>::Decision;

trait Invoke<E: Event> {
    fn invoke(
        &self,
        world: &mut World,
        target: Entity,
        payload: &E,
    ) -> Result<Decision<E>, RequestError>;
}

struct Installed<E: Event, C: CallbackContext<E>> {
    callback: PreparedEvent<E, C>,
    active: Cell<bool>,
}

struct Active<'a>(&'a Cell<bool>);

impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl<E, C> Invoke<E> for Installed<E, C>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Source: EntityEventSource,
{
    fn invoke(
        &self,
        world: &mut World,
        target: Entity,
        payload: &E,
    ) -> Result<Decision<E>, RequestError> {
        if !world.is_alive(target)
            || !E::Source::present(world, target)
            || !crate::core::reactive::retained::enabled(world, target)
        {
            return Err(RequestError::Unavailable);
        }
        if self.active.replace(true) {
            return Err(RequestError::Busy);
        }
        let _active = Active(&self.active);
        let lease = InputLease::capture(world, target);
        let result = self
            .callback
            .invoke(world, payload, target)
            .map_err(RequestError::Command)?;
        if !world.is_alive(target) || lease.is_some_and(|lease| !lease.valid(world)) {
            return Err(RequestError::Unavailable);
        }
        result.ok_or(RequestError::Unavailable)
    }
}

pub(crate) struct Provider<E: Event>(Rc<dyn Invoke<E>>);
impl<E: Event> Component for Provider<E> {}
impl<E: Event> Clone for Provider<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub(crate) struct RequestHook {
    pub(crate) kind: TypeId,
    callback: Rc<dyn Any>,
}

impl RequestHook {
    pub(crate) fn prepare<E, C>(
        world: &mut World,
        callback: EventCallback<E, C>,
    ) -> Result<Self, RuntimeInstallError>
    where
        E: Event,
        C: CallbackContext<E, Target = Entity>,
        E::Source: EntityEventSource,
    {
        Ok(Self {
            kind: TypeId::of::<E>(),
            callback: Rc::new(prepare(world, callback)?),
        })
    }
}

fn prepare<E, C>(
    world: &mut World,
    callback: EventCallback<E, C>,
) -> Result<Provider<E>, RuntimeInstallError>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Source: EntityEventSource,
{
    Ok(Provider(Rc::new(Installed {
        callback: prepare_shared_callback(world, callback)?,
        active: Cell::new(false),
    })))
}

pub(crate) fn install<E, C>(
    world: &mut World,
    target: Entity,
    callback: EventCallback<E, C>,
) -> Result<(), RuntimeInstallError>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Source: EntityEventSource,
{
    if !world.is_alive(target) || !E::Source::present(world, target) {
        return Err(super::listeners::ListenerError::MissingSource { entity: target }.into());
    }
    if world.has::<Provider<E>>(target) {
        return Err(RuntimeInstallError::DuplicateRequest {
            event: core::any::type_name::<E>(),
        });
    }
    let provider = prepare(world, callback)?;
    world.insert(target, provider);
    freeze_shared_callbacks(world);
    Ok(())
}

pub(crate) fn invoke<E, D>(
    world: &mut World,
    target: Entity,
    payload: &E,
) -> Result<D, RequestError>
where
    D: 'static,
    E: Event<Route = EntityRequest<D>>,
    E::Source: EntityEventSource,
{
    if !super::listeners::InputBranch::scan(world, target)
        .map_err(|_| RequestError::Unavailable)?
        .allowed()
    {
        return Err(RequestError::Unavailable);
    }
    if let Some(provider) = world.get::<Provider<E>>(target).cloned() {
        let result = provider.0.invoke(world, target, payload)?;
        if world
            .get::<Provider<E>>(target)
            .is_none_or(|current| !Rc::ptr_eq(&provider.0, &current.0))
        {
            return Err(RequestError::Unavailable);
        }
        return Ok(result);
    }
    let mut selected = None;
    if let Some(registry) = world.resource::<ViewRegistry>() {
        for view in registry.iter() {
            let Some(inputs) = view.inputs() else {
                continue;
            };
            let Some(guard) =
                crate::ui::lifecycle::view_input_guard(world, target, view.registration_id())
            else {
                continue;
            };
            let state = inputs.try_borrow().map_err(|_| RequestError::Busy)?;
            let InputState::Ready(inputs) = &*state else {
                continue;
            };
            for hook in &inputs.requests {
                if hook.kind != TypeId::of::<E>() {
                    continue;
                }
                if selected.is_some() {
                    return Err(RequestError::Ambiguous);
                }
                let provider = hook
                    .callback
                    .downcast_ref::<Provider<E>>()
                    .expect("request hook preserves payload identity")
                    .clone();
                selected = Some((provider, guard));
                break;
            }
        }
    }
    let (provider, guard) = selected.ok_or(RequestError::Unsupported)?;
    let result = provider.0.invoke(world, target, payload)?;
    if !guard.valid(world, target) {
        return Err(RequestError::Unavailable);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
