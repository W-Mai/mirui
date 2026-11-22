use super::{Flame, Prepared, encode_sample};
use mirui::types::{Color, Fixed};

pub(super) struct ScreenFlame {
    pub(super) flame: Flame,
    pub(super) low: [Fixed; 2],
    pub(super) high: [Fixed; 2],
}

impl ScreenFlame {
    pub(super) fn new(mut flame: Flame) -> Option<Self> {
        if flame.radius <= Fixed::ZERO || flame.alpha <= Fixed::ZERO {
            return None;
        }
        for point in &mut flame.points {
            for axis in point {
                *axis += Fixed::HALF;
            }
        }
        let margin = flame.radius * flame.tip_scale.max(Fixed::ONE) * Fixed::from_ratio(42, 10)
            + Fixed::from_int(2);
        let [first, middle, last] = flame.points;
        let low = core::array::from_fn(|i| first[i].min(middle[i]).min(last[i]) - margin);
        let high = core::array::from_fn(|i| first[i].max(middle[i]).max(last[i]) + margin);
        Some(Self { flame, low, high })
    }

    pub(super) fn native_clip(&self, clip: [i32; 4]) -> [i32; 4] {
        let [x0, y0] = self.low.map(|value| value.floor().to_int());
        let [x1, y1] = self.high.map(|value| value.ceil().to_int());
        [
            x0.max(clip[0]),
            y0.max(clip[1]),
            x1.min(clip[2]),
            y1.min(clip[3]),
        ]
    }
}

#[derive(Clone, Copy)]
pub(super) struct SampleAxis {
    pub(super) origin: Fixed,
    pub(super) extent: Fixed,
    pub(super) pixels: Fixed,
}

impl SampleAxis {
    pub(super) const UNIT: Self = Self {
        origin: Fixed::ZERO,
        extent: Fixed::ONE,
        pixels: Fixed::ONE,
    };

    fn center<const RESAMPLED: bool>(self, index: i32) -> Fixed {
        let center = Fixed::from_int(index) + Fixed::HALF;
        if RESAMPLED {
            self.origin + center / self.pixels * self.extent
        } else {
            center
        }
    }

    fn to_sample<const RESAMPLED: bool>(self, position: Fixed) -> Fixed {
        if RESAMPLED {
            (position - self.origin) / self.extent * self.pixels
        } else {
            position
        }
    }
}

impl Prepared {
    pub(super) fn sample<const RESAMPLED: bool>(
        &self,
        [left, top, right, bottom]: [i32; 4],
        [columns, rows]: [SampleAxis; 2],
        mut put: impl FnMut(i32, i32, Color, Color),
    ) {
        let warp_y = self.warp_y_interval();
        for y in top..bottom {
            let sample_y = rows.center::<RESAMPLED>(y);
            let Some([low, high]) = self.row_support(sample_y) else {
                continue;
            };
            if RESAMPLED && low >= high {
                continue;
            }
            // Include the pixel halo around the quantized support boundaries.
            let start = (columns.to_sample::<RESAMPLED>(low).floor().to_int() - 1).max(left);
            let end = (columns.to_sample::<RESAMPLED>(high).ceil().to_int() + 1).min(right);
            if start >= end {
                continue;
            }
            let row = self.row_candidates(sample_y, warp_y);
            for x in start..end {
                let value = self.shade_row::<true>(columns.center::<RESAMPLED>(x), &row);
                let (base, emission) = encode_sample(value);
                put(x, y, base, emission);
            }
        }
    }
}
