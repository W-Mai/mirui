#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
// Lets `trace_span!` / `trace_fn` expand to `mirui::perf::enter(...)`
// uniformly — the crate has to be able to refer to itself by name
// for the absolute path to resolve in our own modules.
extern crate self as mirui;

pub mod anim;
pub mod app;
pub mod components;
pub mod draw;
pub mod ecs;
pub mod event;
pub mod feedback;
pub mod layout;
pub mod perf;
pub mod plugin;
pub mod plugins;
pub mod surface;
pub mod timer;
pub mod types;
pub mod widget;

pub use mirui_macros::{system, trace_fn, trace_span, ui};

/// `use mirui::prelude::*;` brings in the types and macros that nearly
/// every application file needs: `App`, the layout module, `Color` /
/// `Dimension` / `Fixed`, `Entity` / `World`, the widget builder, theme
/// tokens, and the `ui!` macro. Surface backends, individual widget
/// kinds, and plugins stay on their canonical paths so the prelude
/// doesn't pin a platform choice.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::ecs::{Entity, World};
    pub use crate::layout::*;
    pub use crate::types::{Color, Dimension, Fixed, Point, Rect};
    pub use crate::widget::builder::WidgetBuilder;
    pub use crate::widget::theme::{ColorToken, ThemedColor};
    pub use crate::widget::{Style, Widget};

    pub use mirui_macros::ui;
}
