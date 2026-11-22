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
        let first = pixel_center(first_pixel, origin);
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
        let (index, remainder) = scaled_position(first, samples, extent);
        let whole_step = step / extent;
        Self {
            index,
            remainder: remainder as u32,
            step: whole_step,
            step_remainder: (step - whole_step * extent) as u32,
            extent: extent as u32,
        }
    }

    /// Sample one endpoint without preparing an unused iteration step.
    pub(crate) fn at(pixel: i32, origin: Fixed, extent: Fixed, samples: i32) -> i32 {
        let first = pixel_center(pixel, origin);
        let index = if extent == Fixed::from_int(samples) {
            first.div_euclid(i64::from(storage::to_i32(Fixed::ONE)))
        } else {
            scaled_position(first, samples, i64::from(storage::to_i32(extent))).0
        };
        index.clamp(0, i64::from(samples - 1)) as i32
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

    /// Consume a nonempty bounded run with the same nearest source index.
    pub(crate) fn next_run(&mut self, samples: i32, limit: u32) -> (i32, u32) {
        debug_assert!(limit > 0);
        if self.step != 0
            || self.step_remainder == 0
            || self.index < 0
            || self.index >= i64::from(samples)
        {
            return (self.next(samples), 1);
        }
        let sample = self.index as i32;
        let count = (self.extent - self.remainder)
            .div_ceil(self.step_remainder)
            .min(limit);
        // At most one carry occurs; the sum is below twice a positive Q24.8 extent.
        self.remainder += count * self.step_remainder;
        if self.remainder >= self.extent {
            self.remainder -= self.extent;
            self.index += 1;
        }
        (sample, count)
    }
}

#[inline]
fn pixel_center(pixel: i32, origin: Fixed) -> i64 {
    i64::from(pixel) * i64::from(storage::to_i32(Fixed::ONE))
        + i64::from(storage::to_i32(Fixed::HALF))
        - i64::from(storage::to_i32(origin))
}

#[inline]
fn scaled_position(first: i64, samples: i32, extent: i64) -> (i64, i64) {
    let position = first * i64::from(samples);
    let mut index = position / extent;
    let mut remainder = position - index * extent;
    if remainder < 0 {
        index -= 1;
        remainder += extent;
    }
    (index, remainder)
}

#[cfg(test)]
#[path = "../../tests/unit/render/sampling.rs"]
mod tests;
