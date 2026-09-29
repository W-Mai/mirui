//! Orbit Console combines mirui's layout, interaction, animation, SDF text,
//! gradients, paths, and fixed-point rendering in one product-style interface.

extern crate alloc;

#[cfg(any(feature = "std", test))]
mod composition;
#[cfg(any(feature = "std", test))]
mod runtime;
#[cfg(any(feature = "std", test))]
mod state;
#[cfg(any(feature = "std", test))]
mod style;
#[cfg(any(feature = "std", test))]
mod visuals;

#[cfg(test)]
mod tests;

#[cfg(feature = "std")]
pub use runtime::{setup, setup_app};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleMode {
    Orbit,
    Flow,
    Pulse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DemoRunMode {
    Live,
    Capture,
}

pub const VIEWPORT: (u16, u16) = (1024, 640);

#[cfg(any(feature = "std", test))]
const FONT_BYTES: &[u8] = include_bytes!("assets/misans_ui.mirx");

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 640);
