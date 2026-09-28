extern crate alloc;

mod composition;
mod runtime;
mod storage;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(all(feature = "std", feature = "persistence"))]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(320, 240);
