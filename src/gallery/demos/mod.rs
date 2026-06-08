use crate::ecs::{Entity, World};
use crate::widget::{Children, Parent};

pub mod absolute;
pub mod animation;
pub mod app_demo;
pub mod book_flip;
pub mod click;
pub mod components;
pub mod cover_flow;
pub mod custom_view;
pub mod disabled;
pub mod dsl;
pub mod effect;
pub mod enchants;
pub mod flip_card;
pub mod gesture;
pub mod hello;
pub mod hover_tour;
pub mod image;
pub mod image_flip;
pub mod input_feedback;
pub mod interactive_states;
pub mod lazy_list;
pub mod nested_scroll;
pub mod offscreen;
pub mod offscreen_modal;
pub mod on_handlers;
pub mod pinch_rotate;
pub mod rounded;
pub mod scroll;
pub mod slider_switch;
pub mod slider_value_changed;
pub mod spatial_anim;
pub mod tabbar;
pub mod tabbar_selection;
pub mod text;
pub mod text_input;
pub mod theme_swap;
pub mod three_body;
pub mod toggle;
pub mod transform;
pub mod walk;
pub mod widgets;

pub(crate) fn attach_to_parent(world: &mut World, parent: Entity, child: Entity) {
    world.insert(child, Parent(parent));
    if let Some(children) = world.get_mut::<Children>(parent) {
        children.0.push(child);
    }
}
