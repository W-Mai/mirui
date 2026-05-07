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
