mod composition;
#[cfg(feature = "std")]
mod runtime;
mod view;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use view::diamond_render::view as diamond_view;
pub use view::{Diamond, PALETTE};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 200);
