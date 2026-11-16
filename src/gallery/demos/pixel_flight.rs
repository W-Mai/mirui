//! A pixel flight scene with a wrapped height field and layered aircraft.

pub mod cockpit;
pub mod flame;
pub mod flight;
pub mod jet;
mod math;
pub mod raster;
mod runtime;
pub mod world;
pub use runtime::{FlightModel, FlightModelHandle, setup_app, setup_compact_app};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Desktop,
    Compact,
}

impl Profile {
    pub fn scene_size(self, width: u16, height: u16) -> (usize, usize, usize) {
        let (limit_w, limit_h, minimum_scale) = match self {
            Self::Desktop => (480, 300, 3),
            Self::Compact => (128, 128, 1),
        };
        let scale = minimum_scale
            .max(usize::from(width).div_ceil(limit_w))
            .max(usize::from(height).div_ceil(limit_h));
        (
            usize::from(width).div_ceil(scale).max(8),
            usize::from(height).div_ceil(scale).max(8),
            scale,
        )
    }
}

pub const VIEWPORT: (u16, u16) = (1280, 720);
pub const DEMO_SIZE: crate::gallery::DemoSize = crate::gallery::DemoSize::at_least(128, 128);
