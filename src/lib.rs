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

pub use mirui_macros::{system, trace_fn, trace_span};

/// Common imports for application code. `use mirui::prelude::*;` covers
/// `App`, the layout types, the most-reached widget kinds, theme tokens,
/// and the standard plugins.
pub mod prelude {
    pub use crate::app::App;
    pub use crate::ecs::{System, SystemSlot, World};
    pub use crate::layout::*;
    #[cfg(feature = "std")]
    pub use crate::plugins::StdInstantClockPlugin;
    pub use crate::plugins::{
        FpsSummaryPlugin, InputFeedbackPlugin, PerfReportPlugin, SystemPerfSnapshot, SystemStat,
    };
    pub use crate::types::{Color, Dimension, Fixed, Point, Rect};
    pub use crate::widget::builder::WidgetBuilder;
    pub use crate::widget::theme::{ColorToken, Theme, ThemedColor};
    pub use crate::widget::view::View;
    pub use crate::widget::{Style, Widget, WidgetRoot};

    pub use crate::components::button::Button;
    pub use crate::components::checkbox::Checkbox;
    pub use crate::components::image::Image;
    pub use crate::components::lazy_list::LazyList;
    pub use crate::components::progress_bar::ProgressBar;
    pub use crate::components::slider::Slider;
    pub use crate::components::switch::Switch;
    pub use crate::components::tab_pages::TabContent;
    pub use crate::components::tabbar::TabBar;
    pub use crate::components::text::Text;
    pub use crate::components::text_input::TextInput;
}
