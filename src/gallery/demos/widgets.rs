extern crate alloc;

mod binding;
mod composition;
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use binding::slider_to_progress_system;
pub use composition::build_widgets;
pub use runtime::build_sim_timeline;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::{
    ACCENT, Cycle, ThemeCycleIndex, custom_theme, dark_with_accent, light_with_accent,
};

pub const DEFAULT_VIEW: (u16, u16) = (512, 512);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(512, 512);
