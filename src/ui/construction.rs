use crate::ecs::{Entity, World};

use super::{Children, IdMap, NamedId, Parent, Style, Widget};

pub(crate) fn spawn_widget(world: &mut World) -> Entity {
    let entity = world.spawn_empty();
    world.insert(entity, Widget);
    world.insert(entity, Style::default());
    world.insert(entity, Children(alloc::vec::Vec::new()));
    entity
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
    crate::input::event::widget_input::attach_handlers_for(world, entity);
}
