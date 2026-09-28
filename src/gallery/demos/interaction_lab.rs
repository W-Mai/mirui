extern crate alloc;

mod composition;
mod runtime;
mod state;
mod style;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const VIEWPORT: (u16, u16) = (1024, 720);

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 720);
