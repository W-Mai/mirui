extern crate alloc;

mod catalog;
mod composition;
#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use composition::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
