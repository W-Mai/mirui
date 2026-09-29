//! Interactive dual-channel signal scope.

mod controls;
mod render;
mod runtime;
mod signal;
mod state;

#[cfg(test)]
mod tests;

pub use runtime::setup_app;

pub const VIEWPORT: (u16, u16) = (800, 480);

const HOUSING_ART: &[u8] = include_bytes!("assets/product/signal-scope-housing.mirx");

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::range(320, 240, 800, 480);
