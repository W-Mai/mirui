extern crate alloc;

mod composition;
mod motion;
mod render;
#[cfg(feature = "std")]
mod runtime;
mod state;
mod wings;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use motion::{DriftX, DriftY, Flap};
pub use render::butterfly_view;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::Butterfly;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 480);
