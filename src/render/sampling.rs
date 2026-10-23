use crate::types::{Fixed, fixed::storage};

pub(crate) fn dda_step(samples: u16, extent: i32) -> u32 {
    (u32::from(samples) << 16) / extent as u32
}

/// Exact pixel-center sampling for fractional axis-aligned blits.
#[derive(Clone, Copy)]
pub(crate) struct SampleAxis {
    index: i64,
    remainder: u32,
    step: i64,
    step_remainder: u32,
    extent: u32,
}

impl SampleAxis {
    pub(crate) fn new(first_pixel: i32, origin: Fixed, extent: Fixed, samples: i32) -> Self {
        let scale = i64::from(storage::to_i32(Fixed::ONE));
        let first = i64::from(first_pixel) * scale + i64::from(storage::to_i32(Fixed::HALF))
            - i64::from(storage::to_i32(origin));
        if extent == Fixed::from_int(samples) {
            return Self {
                index: first.div_euclid(scale),
                remainder: 0,
                step: 1,
                step_remainder: 0,
                extent: 1,
            };
        }
        let extent = i64::from(storage::to_i32(extent));
        let step = i64::from(samples) * scale;
        let position = first * i64::from(samples);
        let mut index = position / extent;
        let mut remainder = position - index * extent;
        if remainder < 0 {
            index -= 1;
            remainder += extent;
        }
        let whole_step = step / extent;
        Self {
            index,
            remainder: remainder as u32,
            step: whole_step,
            step_remainder: (step - whole_step * extent) as u32,
            extent: extent as u32,
        }
    }

    pub(crate) fn next(&mut self, samples: i32) -> i32 {
        let sample = self.index.clamp(0, i64::from(samples - 1)) as i32;
        self.index += self.step;
        // Remainders are below the positive Q24.8 extent; their sum fits u32.
        self.remainder += self.step_remainder;
        if self.remainder >= self.extent {
            self.remainder -= self.extent;
            self.index += 1;
        }
        sample
    }
}
