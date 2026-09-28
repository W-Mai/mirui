extern crate alloc;

mod composition;
mod model;
#[cfg(feature = "std")]
mod runtime;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use model::{CounterModel, CounterModelHandle, CounterModelObservedSnapshot};
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 240);
