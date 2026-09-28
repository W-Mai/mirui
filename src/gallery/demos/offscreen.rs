extern crate alloc;

mod composition;
#[cfg(feature = "std")]
mod runtime;
mod state;
mod systems;
#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::{DEFAULT_VIEW, ForceDirty, FpsReadout, ModeToggle, PanelTarget};
pub use systems::{force_dirty_system, fps_readout_system, mode_toggle_system};

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 360);
