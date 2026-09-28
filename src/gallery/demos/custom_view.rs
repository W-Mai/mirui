mod composition;
#[cfg(feature = "std")]
mod runtime;
mod view;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use view::{Diamond, PALETTE, diamond_view};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 200);
