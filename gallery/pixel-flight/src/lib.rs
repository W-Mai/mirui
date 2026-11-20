//! A pixel flight scene with a wrapped height field and layered aircraft.

#![no_std]

extern crate alloc;
use core::num::NonZeroU16;
use mirui::types::{Fixed, Point, Rect};
use mirui_sprite_stack::{FrameSize, Resolution};
#[cfg(test)]
#[macro_use]
extern crate std;

#[cfg(test)]
#[path = "../../../tests/support/tracking_allocator.rs"]
mod test_support;

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
        let frame = self.frame_size([width, height]);
        let [width, height] = frame.pixels().map(usize::from);
        (
            width,
            height,
            usize::from(frame.integer_scale().unwrap().get()),
        )
    }

    fn resolution(self) -> Resolution {
        let (limit_w, limit_h, minimum_scale) = match self {
            Self::Desktop => (480, 300, 3),
            Self::Compact => (128, 128, 1),
        };
        Resolution::Integer {
            limit: [limit_w, limit_h]
                .map(|side| NonZeroU16::new(side).expect("positive frame limit")),
            minimum: [8, 8],
            scale: NonZeroU16::new(minimum_scale).expect("positive frame scale"),
        }
    }

    fn frame_size(self, viewport: [u16; 2]) -> FrameSize {
        self.resolution().fit(viewport)
    }

    fn frame_input(
        self,
        dimensions: [Fixed; 2],
        pointer: Option<[Fixed; 2]>,
    ) -> ([usize; 2], Option<[Fixed; 2]>) {
        let frame =
            self.frame_size(dimensions.map(|value| value.ceil().to_int().clamp(0, 65535) as u16));
        let viewport = Rect::new(Fixed::ZERO, Fixed::ZERO, dimensions[0], dimensions[1]);
        let point = pointer.and_then(|[x, y]| frame.frame_point(viewport, Point::new(x, y)));
        (frame.pixels().map(usize::from), point.map(|p| [p.x, p.y]))
    }
}

pub const VIEWPORT: (u16, u16) = (1280, 720);
pub const COMPACT_VIEWPORT: (u16, u16) = (128, 128);
