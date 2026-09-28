#![allow(clippy::needless_update)]

mod composition;
mod render;
#[cfg(feature = "std")]
mod runtime;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use render::{FillRules, fill_rules_view};
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(400, 240);
