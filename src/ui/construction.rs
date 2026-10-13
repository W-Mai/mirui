use crate::ecs::{Entity, World};

use super::{Children, IdMap, NamedId, Parent, Style, Widget};

#[doc(hidden)]
pub fn component_type<C: crate::ecs::Component>(_: &C) -> core::marker::PhantomData<fn() -> C> {
    core::marker::PhantomData
}

#[doc(hidden)]
pub fn component_event<C, E>(
    _: core::marker::PhantomData<fn() -> C>,
    key: crate::input::event::contract::EventKey<E>,
) -> crate::input::event::contract::EventKey<E>
where
    C: crate::ecs::Component,
    E: crate::input::event::contract::Event,
    E::Source: crate::input::event::contract::EntitySourceFor<C>,
{
    key
}

#[doc(hidden)]
pub fn apply_component_field<C: crate::ecs::Component, V>(
    _: core::marker::PhantomData<fn() -> C>,
    entity: Entity,
    value: V,
    apply: impl FnOnce(&mut C, V) -> bool,
) {
    crate::core::reactive::with_world(|world| {
        if let Some(component) = world.get_mut::<C>(entity)
            && apply(component, value)
        {
            world.invalidate(entity);
        }
    });
}

pub(crate) fn spawn_widget(world: &mut World) -> Entity {
    let entity = world.spawn_empty();
    initialize_widget(world, entity);
    entity
}

fn initialize_widget(world: &mut World, entity: Entity) {
    world.insert(entity, Widget);
    world.insert(entity, Style::default());
    world.insert(entity, Children(alloc::vec::Vec::new()));
}

#[doc(hidden)]
pub struct ReservedWidget(Entity);

impl ReservedWidget {
    pub fn new(world: &mut World, name: &'static str) -> Self {
        let entity = world.spawn_empty();
        world
            .resource_mut::<IdMap>()
            .expect("widget construction requires an ID registry")
            .insert(name, entity);
        Self(entity)
    }

    pub fn build(self, world: &mut World) -> super::builder::WidgetBuilder<'_> {
        initialize_widget(world, self.0);
        super::builder::WidgetBuilder::from_entity(world, self.0)
    }
}

#[doc(hidden)]
pub fn finish_widget_static(world: &mut World, parent: Entity, entity: Entity) {
    world.insert(entity, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(entity);
    } else {
        world.insert(parent, Children(alloc::vec![entity]));
    }
    if let Some(name) = world.get::<NamedId>(entity).map(|name| name.0)
        && let Some(ids) = world.resource_mut::<IdMap>()
    {
        ids.insert(name, entity);
    }
    crate::ui::lifecycle::attach_handlers_for(world, entity);
}
