extern crate alloc;

mod composition;
#[cfg(feature = "std")]
mod runtime;
mod state;
#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 320);
