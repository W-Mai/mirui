extern crate alloc;

use super::PROJECTIVE_SPIN_PHASE;

mod animation;
mod composition;
#[cfg(feature = "std")]
mod runtime;
mod state;
#[cfg(test)]
mod tests;

pub use animation::flip_system;
pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::Page;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(640, 360);
