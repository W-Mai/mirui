mod composition;
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::{ACCENT, ThemeChoice, custom_theme, dark_with_accent, light_with_accent};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
