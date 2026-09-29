extern crate alloc;

mod composition;
mod render;
mod runtime;
mod state;
mod style;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use runtime::{build_sim_timeline, install};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(128, 128);

pub const VIEWPORT: (u16, u16) = (128, 128);
