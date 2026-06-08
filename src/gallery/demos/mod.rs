use crate::ecs::{Entity, World};
use crate::widget::{Children, Parent};

pub mod hello;
pub mod on_handlers;
pub mod slider_value_changed;
pub mod tabbar_selection;
pub mod toggle;

pub(crate) fn attach_to_parent(world: &mut World, parent: Entity, child: Entity) {
    world.insert(child, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(child);
    }
}
