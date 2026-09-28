#![allow(clippy::needless_update)]

extern crate alloc;

mod composition;
#[cfg(feature = "std")]
mod runtime;
mod view;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use view::{Shapes, shapes_anim_system, shapes_view};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 480);
