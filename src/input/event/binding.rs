use crate::ecs::{Entity, World};

use super::callback::{CallbackContext, EventCallback};
use super::contract::{
    EntityBubble, EntityEventSource, EntityNotification, EntityRoute, Event, EventKey, EventRoute,
    InputFallback,
};
use super::gesture::GesturePayload;
use super::listeners::{ListenerDecision, ListenerSelector};

mod sealed {
    pub trait Install<E> {}
}

/// Selector matching used by generated entity event bindings.
#[doc(hidden)]
pub trait EntityInstallRoute<E: Event>: EntityRoute + sealed::Install<E> {
    fn selector(key: EventKey<E>) -> ListenerSelector<E>;
}

impl<E: GesturePayload> sealed::Install<E> for EntityBubble {}

impl<E: GesturePayload> EntityInstallRoute<E> for EntityBubble {
    fn selector(key: EventKey<E>) -> ListenerSelector<E> {
        ListenerSelector::gesture(key)
    }
}

macro_rules! unit_routes {
    ($($route:ty),+ $(,)?) => {
        $(
            impl<E: Event<Route = $route, Options = ()>> sealed::Install<E> for $route {}

            impl<E: Event<Route = $route, Options = ()>> EntityInstallRoute<E> for $route {
                fn selector(key: EventKey<E>) -> ListenerSelector<E> {
                    ListenerSelector::new(key, |(), _| true)
                }
            }
        )+
    };
}

unit_routes!(EntityNotification, InputFallback);

/// One uninstalled binding stored by a widget builder.
#[doc(hidden)]
pub struct EventBinding<E: Event, C: CallbackContext<E>> {
    key: EventKey<E>,
    callback: EventCallback<E, C>,
}

impl<E: Event, C: CallbackContext<E>> EventBinding<E, C> {
    pub fn new(key: EventKey<E>, callback: EventCallback<E, C>) -> Self {
        Self { key, callback }
    }
}

/// Installation-order bindings retained without an additional container.
#[doc(hidden)]
pub trait EntityEventBindings {
    fn install(self, world: &mut World, entity: Entity);
}

impl EntityEventBindings for () {
    fn install(self, _world: &mut World, _entity: Entity) {}
}

macro_rules! binding_tuple {
    ($($item:ident),+) => {
        impl<$($item: EntityEventBindings),+> EntityEventBindings for ($($item,)+) {
            fn install(self, world: &mut World, entity: Entity) {
                #[allow(non_snake_case)]
                let ($($item,)+) = self;
                $($item.install(world, entity);)+
            }
        }
    };
}

binding_tuple!(A);
binding_tuple!(A, B);
binding_tuple!(A, B, C);
binding_tuple!(A, B, C, D);
binding_tuple!(A, B, C, D, E);
binding_tuple!(A, B, C, D, E, F);
binding_tuple!(A, B, C, D, E, F, G);
binding_tuple!(A, B, C, D, E, F, G, H);

impl<E, C> EntityEventBindings for EventBinding<E, C>
where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Route: EntityInstallRoute<E>,
    E::Source: EntityEventSource,
    <E::Route as EventRoute>::Decision: ListenerDecision,
{
    fn install(self, world: &mut World, entity: Entity) {
        install_entity_event(world, entity, self.key, self.callback);
    }
}

/// Install a generated binding after its entity and component source exist.
#[doc(hidden)]
pub fn install_entity_event<E, C>(
    world: &mut World,
    entity: Entity,
    key: EventKey<E>,
    callback: EventCallback<E, C>,
) where
    E: Event,
    C: CallbackContext<E, Target = Entity>,
    E::Route: EntityInstallRoute<E>,
    E::Source: EntityEventSource,
    <E::Route as EventRoute>::Decision: ListenerDecision,
{
    if let Err(error) = super::runtime::install_entity(world, entity, key, callback) {
        crate::warn!(target: "mirui::event::install", "entity event binding skipped: {:?}", error);
    }
}
