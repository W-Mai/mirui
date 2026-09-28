#![allow(clippy::needless_update)]

extern crate alloc;

mod composition;
mod render;
#[cfg(feature = "std")]
mod runtime;
mod scene;
mod state;
#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use render::{vector_mandala_anim_system, vector_mandala_view};
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::VectorMandala;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(512, 512);
