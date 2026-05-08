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

/// Computed screen rect after layout (logical pixels)
pub struct ComputedRect(pub crate::types::Rect);

/// Move a widget to a new absolute position, automatically tracking dirty state.
pub fn set_position(world: &mut crate::ecs::World, entity: crate::ecs::Entity, x: i32, y: i32) {
    use crate::types::{Dimension, Fixed, Rect};
    use dirty::{Dirty, PrevRect};

    if let Some(style) = world.get::<Style>(entity) {
        let old_left = style
            .layout
            .left
            .resolve(Fixed::ZERO)
            .unwrap_or(Fixed::ZERO)
            .to_int();
        let old_top = style
            .layout
            .top
            .resolve(Fixed::ZERO)
            .unwrap_or(Fixed::ZERO)
            .to_int();
        let old_w = style
            .layout
            .width
            .resolve(Fixed::ZERO)
            .unwrap_or(Fixed::ZERO)
            .to_int() as u16;
        let old_h = style
            .layout
            .height
            .resolve(Fixed::ZERO)
            .unwrap_or(Fixed::ZERO)
            .to_int() as u16;
        if old_left != x || old_top != y {
            world.insert(
                entity,
                PrevRect(Rect {
                    x: Fixed::from_int(old_left),
                    y: Fixed::from_int(old_top),
                    w: Fixed::from_int(old_w as i32),
                    h: Fixed::from_int(old_h as i32),
                }),
            );
        }
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = Dimension::Px(Fixed::from_int(x));
        style.layout.top = Dimension::Px(Fixed::from_int(y));
    }
    world.insert(entity, Dirty);
}
