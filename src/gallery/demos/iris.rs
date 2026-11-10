//! Iris is a lens controller. Nine aperture blades close around the red iris
//! of the mirui mark, and the same component tree reflows from a 128 x 160
//! panel to a round watch face, a phone and a desktop window.

extern crate alloc;

#[cfg(any(feature = "std", test))]
mod composition;
#[cfg(any(feature = "std", test))]
mod lens;
#[cfg(any(feature = "std", test))]
mod meters;
#[cfg(any(feature = "std", test))]
mod runtime;
#[cfg(any(feature = "std", test))]
mod state;
#[cfg(any(feature = "std", test))]
mod style;

#[cfg(test)]
mod tests;

#[cfg(any(feature = "std", test))]
pub use runtime::{setup, setup_app, shutter_bounds};
#[cfg(any(feature = "std", test))]
pub use state::{IrisLayout, IrisRun};
pub const DARK_ID: &str = "iris-dark";
pub const LIGHT_ID: &str = "iris-light";

pub const VIEWPORT: (u16, u16) = (1280, 800);

/// Length of the showcase loop that `IrisRun::Capture` repeats exactly.
pub const LOOP_MS: u32 = 8_000;

/// The Play face that `PlayFontPlugin` installs, checked for readout coverage.
#[cfg(test)]
const FONT_BYTES: &[u8] = crate::gallery::play::font::FONT_BYTES;

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(128, 128, 1440, 900);
