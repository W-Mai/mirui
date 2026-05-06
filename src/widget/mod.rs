pub mod builder;
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
    pub layout: LayoutStyle,
}

/// Parent-children relationship
pub struct Children(pub Vec<crate::ecs::Entity>);

/// Who is my parent
pub struct Parent(pub crate::ecs::Entity);
