#[cfg(any(feature = "std", test))]
mod composition;
#[cfg(any(feature = "std", test))]
mod runtime;
#[cfg(any(feature = "std", test))]
mod state;
#[cfg(any(feature = "std", test))]
mod style;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const VIEWPORT: (u16, u16) = (1024, 720);

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 720);
