mod composition;
mod gesture;
#[cfg(feature = "std")]
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::PinchTarget;

pub const DEFAULT_VIEW: (u16, u16) = (480, 360);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 360);
