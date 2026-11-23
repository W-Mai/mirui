extern crate alloc;

mod composition;
mod render;
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use render::field_view;
pub use runtime::install;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::Swarm;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
