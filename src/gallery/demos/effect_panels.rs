extern crate alloc;

mod animation;
mod composition;
mod runtime;
mod style;

#[cfg(test)]
mod tests;

pub use animation::{BlurPan, ColorFlash, GlowPulse, ShadowOffset, animate_color_flash};
pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEFAULT_VIEW: (u16, u16) = (360, 560);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(360, 560);
