#![allow(clippy::needless_update)]

mod composition;
mod render;
#[cfg(feature = "std")]
mod runtime;
mod scene;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use render::{ClipPath, clip_path_view};
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(320, 320);
