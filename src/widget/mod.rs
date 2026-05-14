pub mod builder;
pub mod dirty;
pub mod render_system;

use alloc::vec::Vec;

use crate::layout::LayoutStyle;
use crate::types::{Color, Fixed};

/// Marker: this entity is a widget
pub struct Widget;

/// Visual style of a widget
#[derive(Clone, Debug, Default)]
pub struct Style {
    pub bg_color: Option<Color>,
    pub border_color: Option<Color>,
    pub border_width: Fixed,
    pub border_radius: Fixed,
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
pub fn set_position(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    x: impl Into<crate::types::Fixed>,
    y: impl Into<crate::types::Fixed>,
) {
    use crate::types::{Dimension, Fixed, Rect};
    use dirty::{Dirty, PrevRect};

    let x = x.into();
    let y = y.into();

    if let Some(style) = world.get::<Style>(entity) {
        let l = &style.layout;
        let old_rect = Rect {
            x: l.left.resolve_or(Fixed::ZERO, Fixed::ZERO),
            y: l.top.resolve_or(Fixed::ZERO, Fixed::ZERO),
            w: l.width.resolve_or(Fixed::ZERO, Fixed::ZERO),
            h: l.height.resolve_or(Fixed::ZERO, Fixed::ZERO),
        };
        let new_rect = Rect {
            x,
            y,
            w: old_rect.w,
            h: old_rect.h,
        };
        if old_rect.to_px() != new_rect.to_px() {
            let (px, py, pw, ph) = old_rect.to_px();
            let axis_old = Rect::new(px, py, pw, ph);
            let merged = match world.get::<PrevRect>(entity) {
                Some(p) => p.0.union(&axis_old),
                None => axis_old,
            };
            world.insert(entity, PrevRect(merged));
        }
    }
    if let Some(style) = world.get_mut::<Style>(entity) {
        style.layout.left = Dimension::Px(x);
        style.layout.top = Dimension::Px(y);
    }
    world.insert(entity, Dirty);
}
