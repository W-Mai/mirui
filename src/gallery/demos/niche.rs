mod card;
mod composition;
#[cfg(feature = "std")]
mod runtime;

#[cfg(test)]
mod tests;

pub use card::Card;
pub use composition::build_widgets;
#[cfg(feature = "std")]
pub use runtime::setup_app;

pub const DEFAULT_VIEW: (u16, u16) = (480, 320);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(480, 320);
