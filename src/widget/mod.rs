pub mod builder;
pub mod dirty;
pub mod render_system;

use alloc::vec::Vec;

use crate::layout::LayoutStyle;
use crate::types::Color;

/// Marker: this entity is a widget
pub struct Widget;

/// Visual style of a widget
#[derive(Clone, Debug, Default)]
pub struct Style {
    pub bg_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: u16,
    pub border_radius: u16,
    pub text_color: Option<Color>,
    pub layout: LayoutStyle,
}

/// Text content component
pub struct Text(pub alloc::vec::Vec<u8>);

/// Parent-children relationship
pub struct Children(pub Vec<crate::ecs::Entity>);

/// Who is my parent
pub struct Parent(pub crate::ecs::Entity);

/// Move a widget to a new absolute position, automatically tracking dirty state.
pub fn set_position(world: &mut crate::ecs::World, entity: crate::ecs::Entity, x: i32, y: i32) {
    use crate::types::Rect;
    use dirty::{Dirty, PrevRect};

    if let Some(style) = world.get::<Style>(entity) {
        let old_left = style.layout.left.unwrap_or(0);
        let old_top = style.layout.top.unwrap_or(0);
        let old_w = style.layout.width.unwrap_or(0);
        let old_h = style.layout.height.unwrap_or(0);
        if old_left != x || old_top != y {
            world.insert(
                entity,
                PrevRect(Rect {
                    x: old_left,
                    y: old_top,
                    w: old_w,
                    h: old_h,
                }),
            );
        }
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = Some(x);
        style.layout.top = Some(y);
    }
    world.insert(entity, Dirty);
}
