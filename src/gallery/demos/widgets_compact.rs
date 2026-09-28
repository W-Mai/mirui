use super::compact_layout;

mod binding;
mod composition;
mod runtime;
mod style;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use runtime::{install, setup_app};

pub const VIEWPORT: (u16, u16) = (128, 128);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::fixed(128, 128);
