extern crate alloc;

mod composition;
mod motion;
mod runtime;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use motion::{GaussRadius, GlassX};
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEFAULT_VIEW: (u16, u16) = (128, 128);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(128, 128);
