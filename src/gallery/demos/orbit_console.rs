//! Orbit Console combines mirui's layout, interaction, animation, SDF text,
//! gradients, paths, and fixed-point rendering in one product-style interface.

extern crate alloc;

mod composition;
mod runtime;
mod state;
mod style;
mod visuals;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use runtime::console_animation_system;
#[cfg(feature = "std")]
pub use runtime::{setup, setup_app};
pub use state::{ConsoleMode, ConsoleState, DemoRunMode};
pub use visuals::{
    ActivityPlot, ConsoleBackdrop, OrbitInstrument, SignalMeter, activity_view, backdrop_view,
    orbit_view, signal_view,
};

pub const VIEWPORT: (u16, u16) = (1024, 640);

#[cfg(any(feature = "std", test))]
const FONT_BYTES: &[u8] = include_bytes!("assets/misans_ui.mirx");

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 640);
