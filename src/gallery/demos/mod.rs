use crate::ecs::{Entity, World};
use crate::widget::{Children, Parent};

pub mod absolute;
pub mod hello;
pub mod lazy_list;
pub mod nested_scroll;
pub mod on_handlers;
pub mod scroll;
pub mod slider_value_changed;
pub mod tabbar_selection;
pub mod toggle;
pub mod walk;

pub(crate) fn attach_to_parent(world: &mut World, parent: Entity, child: Entity) {
    world.insert(child, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(child);
    }
}
