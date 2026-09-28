extern crate alloc;

mod composition;
mod motion;
#[cfg(feature = "std")]
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use motion::bar_move_system;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::BarState;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
