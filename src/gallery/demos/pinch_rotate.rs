#[cfg(any(feature = "std", test))]
mod composition;
#[cfg(any(feature = "std", test))]
mod gesture;
#[cfg(feature = "std")]
mod runtime;
#[cfg(any(feature = "std", test))]
mod state;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEFAULT_VIEW: (u16, u16) = (480, 360);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 360);
