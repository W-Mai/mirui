use super::Flame;
use mirui::types::{Color, Fixed};

const CORE: Color = Color::rgb(255, 242, 187);
const BODY: Color = Color::rgb(255, 157, 49);
const EDGE: Color = Color::rgb(244, 75, 21);

pub(super) struct ScreenFlame {
    pub(super) flame: Flame,
    pub(super) low: [Fixed; 2],
    pub(super) high: [Fixed; 2],
}

impl ScreenFlame {
    pub(super) fn new(mut flame: Flame) -> Option<Self> {
        if flame.radius <= Fixed::ZERO
            || flame.alpha <= Fixed::ZERO
            || flame.visibility.iter().all(|sample| sample.is_zero())
        {
            return None;
        }
        let max_width = flame.radius * flame.tip_scale.max(Fixed::ONE);
        let margin = (max_width * Fixed::from_ratio(17, 10)).max(Fixed::ONE);
        for point in &mut flame.points {
            for axis in point {
                *axis += Fixed::HALF;
            }
        }
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

/// Shades the curved exhaust silhouette in scene coordinates.
pub(super) struct PixelFlame {
    flame: Flame,
    middle: [Fixed; 2],
    chord: [Fixed; 2],
    axial: [Fixed; 2],
    phase: Fixed,
}

impl PixelFlame {
    pub(super) fn prepare(screen: ScreenFlame, time: Fixed) -> Self {
        let flame = screen.flame;
        let middle = core::array::from_fn(|i| flame.points[1][i] - flame.points[0][i]);
        let chord = core::array::from_fn(|i| flame.points[2][i] - flame.points[0][i]);
        let squared = dot(chord, chord);
        let axial = if squared >= Fixed::from_ratio(1, 16) {
            chord.map(|component| component / squared)
        } else {
            [Fixed::ZERO; 2]
        };
        let phase = fract(time) * 8 + flame.seed * Fixed::from_ratio(3, 10);
        Self {
            flame,
            middle,
            chord,
            axial,
            phase,
        }
    }

    pub(super) fn shade(&self, point: [Fixed; 2]) -> Option<(Color, Color)> {
        let flame = &self.flame;
        let offset = core::array::from_fn(|i| point[i] - flame.points[0][i]);
        let t = dot(offset, self.axial).clamp(Fixed::ZERO, Fixed::ONE);
        // Work relative to the nozzle so fixed-point weights cannot shift the curve on screen.
        let middle_weight = t * (Fixed::ONE - t) * 2;
        let end_weight = t * t;
        let delta = core::array::from_fn(|i| {
            offset[i] - self.middle[i] * middle_weight - self.chord[i] * end_weight
        });
        let distance = dot(delta, delta);
        let width = flame.radius * mix(Fixed::ONE, flame.tip_scale, t);
        let taper = mix(Fixed::from_ratio(8, 10), Fixed::from_ratio(2, 10), t);
        let core_width = (width * taper).max(Fixed::from_ratio(1, 4));
        let core_radius = core_width * core_width;
        let glow_width =
            (core_width + width * Fixed::from_ratio(7, 10)).max(Fixed::from_ratio(3, 8));
        if distance >= glow_width * glow_width {
            return None;
        }

        let visible = if t < Fixed::HALF {
            mix(flame.visibility[0], flame.visibility[1], t * 2)
        } else {
            mix(flame.visibility[1], flame.visibility[2], t * 2 - Fixed::ONE)
        };
        let strength = flame.alpha * visible * mix(Fixed::ONE, Fixed::from_ratio(3, 10), t);
        if strength.is_zero() {
            return None;
        }
        let pulse = fract(t * 3 - self.phase) < Fixed::from_ratio(2, 5);
        let hot = t < Fixed::HALF || pulse;
        let mut base = if distance < core_radius * Fixed::from_ratio(1, 4) && hot {
            CORE
        } else if distance < core_radius * Fixed::from_ratio(2, 3) {
            BODY
        } else {
            EDGE
        };
        let opacity = if distance < core_radius {
            if base == EDGE {
                Fixed::from_ratio(3, 5)
            } else {
                Fixed::ONE
            }
        } else {
            Fixed::ZERO
        };
        base.a = channel(strength * opacity, 255);
        let glow = strength
            * if distance < core_radius {
                Fixed::from_ratio(1, 5)
            } else {
                Fixed::from_ratio(1, 12)
            };
        Some((
            base,
            Color::rgb(channel(glow, 255), channel(glow, 108), channel(glow, 32)),
        ))
    }
}

fn fract(value: Fixed) -> Fixed {
    value - value.floor()
}

fn mix(a: Fixed, b: Fixed, t: Fixed) -> Fixed {
    a + (b - a) * t
}

fn dot([ax, ay]: [Fixed; 2], [bx, by]: [Fixed; 2]) -> Fixed {
    ax * bx + ay * by
}

fn channel(value: Fixed, peak: i32) -> u8 {
    (value * peak).round().to_int().clamp(0, peak) as u8
}
