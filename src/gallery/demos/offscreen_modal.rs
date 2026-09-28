extern crate alloc;

mod animation;
mod composition;
mod runtime;
mod state;
mod style;

#[cfg(test)]
mod tests;

pub use animation::{fps_readout_system, modal_slide_system, mode_toggle_system};
pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::{DEFAULT_VIEW, FpsReadout, ModalAnim, ModeToggle};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 360);
