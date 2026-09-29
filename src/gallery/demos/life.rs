mod composition;
mod render;
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use composition::setup_app;
pub use render::life_render::view as life_view;
pub use runtime::life_step_system;
pub use state::LifeBoard;

pub(super) use runtime::dims_from_px;
#[cfg(feature = "std")]
pub(super) use runtime::install_runtime;
pub(super) use state::seeded_board;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(640, 640);
