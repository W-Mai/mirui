//! A pixel flight scene with a wrapped height field and layered aircraft.

#![no_std]

extern crate alloc;
use mirui::types::Fixed;
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

    fn frame_input(
        self,
        dimensions: [Fixed; 2],
        pointer: Option<[Fixed; 2]>,
    ) -> ([usize; 2], Option<[Fixed; 2]>) {
        let [w, h] = dimensions.map(|dimension| dimension.ceil().to_int().clamp(0, 65535) as u16);
        let (w, h, scale) = self.scene_size(w, h);
        let size = [w, h];
        let scale = Fixed::from_int(scale as i32);
        let point = pointer.map(|point| {
            core::array::from_fn(|axis| {
                let frame = Fixed::from_int(size[axis] as i32) * scale;
                let offset = ((dimensions[axis] - frame) / 2).round();
                (point[axis] - offset) / scale
            })
        });
        (size, point)
    }
}

pub const VIEWPORT: (u16, u16) = (1280, 720);
pub const COMPACT_VIEWPORT: (u16, u16) = (128, 128);
