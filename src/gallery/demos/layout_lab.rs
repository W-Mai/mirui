mod cards;
mod chips;
mod shell;
mod style;

#[cfg(test)]
mod tests;

pub use shell::build_widgets;
#[cfg(feature = "std")]
pub use shell::setup_app;

pub const VIEWPORT: (u16, u16) = (1024, 720);

pub const DEMO_SIZE: crate::gallery::DemoSize =
    crate::gallery::DemoSize::range(320, 320, 1024, 720);
