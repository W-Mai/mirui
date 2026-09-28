extern crate alloc;

mod composition;
mod motion;
mod runtime;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use motion::{AnimateColor, AnimateX};
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 180);
