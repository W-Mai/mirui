extern crate alloc;

mod composition;
mod layout;
mod runtime;
mod state;

#[cfg(test)]
mod tests;

pub use composition::build_widgets;
pub use layout::layout_system;
#[cfg(feature = "std")]
pub use runtime::setup_app;
pub use state::{Carousel, CarouselCard, CoverFlowBounds};

pub const DEFAULT_VIEW: (u16, u16) = (640, 360);

pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_most(640, 360);
