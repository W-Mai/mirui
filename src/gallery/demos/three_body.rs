extern crate alloc;

mod composition;
mod render;
#[cfg(feature = "std")]
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use render::field_view;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::Orbit;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
