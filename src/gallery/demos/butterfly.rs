#![allow(clippy::needless_update)]
#![allow(clippy::too_many_arguments)]

extern crate alloc;

mod animation;
mod composition;
mod render;
#[cfg(feature = "std")]
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use animation::butterfly_anim_system;
pub use composition::build_widgets;
pub use render::butterfly_view;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::Butterfly;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 480);
