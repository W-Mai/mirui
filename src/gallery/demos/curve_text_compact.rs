mod composition;
mod geometry;
mod runtime;
mod state;
mod style;
#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use runtime::install;
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(128, 128);
pub const VIEWPORT: (u16, u16) = (128, 128);
