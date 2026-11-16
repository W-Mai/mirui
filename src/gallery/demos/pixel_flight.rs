//! A pixel flight scene with a wrapped height field and layered aircraft.

pub mod cockpit;
pub mod flame;
pub mod flight;
pub mod jet;
pub mod raster;
mod runtime;
pub mod world;
pub use runtime::{FlightModel, FlightModelHandle, setup_app};

pub const VIEWPORT: (u16, u16) = (1280, 720);
pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(480, 320);
