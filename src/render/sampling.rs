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
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_iteration_match_exact_pixel_center_coordinates() {
        for samples in [1, 2, 7, 127, 1024] {
            for extent in [1, 128, 384, 1792, 32512, 262144] {
                for origin in [-1048575, -256, 0, 128, 16383] {
                    let extent = Fixed::from_ratio(extent, 256);
                    let origin = Fixed::from_ratio(origin, 256);
                    for start in [-4096, -7, 0, 1, 127, 4096] {
                        let mut axis = SampleAxis::new(start, origin, extent, samples);
                        for pixel in start..start + 17 {
                            let center =
                                i128::from(pixel) * 256 + 128 - i128::from(storage::to_i32(origin));
                            let expected = (center * i128::from(samples))
                                .div_euclid(i128::from(storage::to_i32(extent)))
                                .clamp(0, i128::from(samples - 1))
                                as i32;
                            assert_eq!(SampleAxis::at(pixel, origin, extent, samples), expected);
                            assert_eq!(axis.next(samples), expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn runs_preserve_rational_samples_and_following_iterator_state() {
        for samples in [1, 2, 7, 127, 1024] {
            for extent in [1, 128, 384, 1792, 32512, 262144] {
                for origin in [-1048575, -256, 0, 128, 16383] {
                    for start in [-4096, -7, 0, 1, 127, 4096] {
                        for limit in [1, 2, 7, 31] {
                            let extent = Fixed::from_ratio(extent, 256);
                            let origin = Fixed::from_ratio(origin, 256);
                            let mut runs = SampleAxis::new(start, origin, extent, samples);
                            let mut points = runs;
                            let mut consumed = 0;
                            while consumed < 33 {
                                let (sample, count) =
                                    runs.next_run(samples, limit.min(33 - consumed));
                                assert!(count > 0 && count <= limit);
                                for offset in consumed..consumed + count {
                                    let pixel = i128::from(start) + i128::from(offset);
                                    let center =
                                        pixel * 256 + 128 - i128::from(storage::to_i32(origin));
                                    let expected = (center * i128::from(samples))
                                        .div_euclid(i128::from(storage::to_i32(extent)))
                                        .clamp(0, i128::from(samples - 1))
                                        as i32;
                                    assert_eq!(sample, expected);
                                    assert_eq!(sample, points.next(samples));
                                }
                                consumed += count;
                            }
                            for _ in 0..17 {
                                assert_eq!(runs.next(samples), points.next(samples));
                            }
                        }
                    }
                }
            }
        }
        for samples in [1, 2, 65_535, 8_388_607] {
            let extent = Fixed::from_ratio(i32::MAX, 256);
            let mut axis = SampleAxis::new(0, Fixed::ZERO, extent, samples);
            let (_, count) = axis.next_run(samples, u32::MAX);
            assert_eq!(
                axis.next(samples),
                SampleAxis::at(count as i32, Fixed::ZERO, extent, samples)
            );
        }
    }
}
