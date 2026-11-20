use super::{Flame, PlacedAircraft};
use crate::{
    flight::{Flight, Projection, aircraft_point},
    jet,
    math::FixedMath as _,
};
use mirui::types::Fixed;
use mirui_sprite_stack::raster::{DepthQuery, DepthSamples, RollMapping, SceneCanvas};

#[derive(Clone, Copy, Default)]
pub(super) struct PendingFlame {
    flame: Flame,
    projected: [[Fixed; 4]; 3],
    slot: usize,
    samples: DepthSamples<15>,
}
impl PendingFlame {
    fn query(&self, index: usize) -> (i32, i32, Fixed) {
        let n = index / 5;
        let point = if n == 1 {
            core::array::from_fn(|k| {
                Fixed::from_ratio(25, 100) * self.projected[0][k]
                    + Fixed::from_ratio(5, 10) * self.projected[1][k]
                    + Fixed::from_ratio(25, 100) * self.projected[2][k]
            })
        } else {
            self.projected[n]
        };
        let radius = self.flame.radius;
        let (dx, dy) = [
            (Fixed::ZERO, Fixed::ZERO),
            (-radius, Fixed::ZERO),
            (radius, Fixed::ZERO),
            (Fixed::ZERO, -radius),
            (Fixed::ZERO, radius),
        ][index % 5];
        (
            (point[0] + dx).round().to_int(),
            (point[1] + dy).round().to_int(),
            point[3],
        )
    }
}
impl DepthQuery for PendingFlame {
    fn resolve(&mut self, canvas: &SceneCanvas<'_, '_>) {
        let mut samples = self.samples;
        samples.resolve(canvas, Fixed::ONE, |i| self.query(i));
        self.samples = samples;
    }
    fn next_point(&self) -> Option<[i32; 2]> {
        self.samples.next_missing().map(|i| {
            let (x, y, _) = self.query(i);
            [x, y]
        })
    }
    fn visit_pending(&self, visit: &mut dyn FnMut([i32; 2], Fixed)) {
        for index in self.samples.missing() {
            let (x, y, depth) = self.query(index);
            visit([x, y], depth - Fixed::ONE);
        }
    }
}
impl PendingFlame {
    fn finish(mut self, seen: &mut [[Fixed; 3]; 6]) -> Flame {
        let visible = self
            .samples
            .visibility()
            .expect("unresolved flame visibility");
        for (n, (value, target)) in seen[self.slot]
            .iter_mut()
            .zip(self.flame.visibility.iter_mut())
            .enumerate()
        {
            let open = ((visible >> (n * 5)) & 31).count_ones() as i32;
            *value +=
                (Fixed::from_int(open) / Fixed::from_int(5) - *value) * Fixed::from_ratio(3, 10);
            *target = *value;
        }
        self.flame
    }
}
pub(super) fn project_flames(
    flight: &Flight,
    (index, jet): (usize, PlacedAircraft),
    p: &Projection,
    heading: (Fixed, Fixed),
    out: &mut [PendingFlame],
    count: &mut usize,
) {
    let alpha = if index == 0 { p.jet_alpha } else { Fixed::ONE } * (Fixed::ONE - p.cockpit_in());
    let flick = Fixed::from_ratio(82, 100)
        + Fixed::from_ratio(18, 100)
            * (flight.time * Fixed::from_int(47)
                + Fixed::from_int((index) as i32) * Fixed::from_int(2))
            .sin()
            * (flight.time * Fixed::from_int(31)).sin();
    let length = flight.flame_length()
        * (Fixed::from_ratio(94, 100)
            + Fixed::from_ratio(3, 10) * (flick - Fixed::from_ratio(82, 100)))
        * Fixed::from_ratio(13, 10);
    if length <= Fixed::from_ratio(6, 10)
        || alpha < Fixed::from_ratio(1, 100)
        || flight.spread >= Fixed::from_ratio(26, 10)
    {
        return;
    }
    let width = Fixed::from_ratio(163, 100)
        * (Fixed::ONE + Fixed::from_ratio(12, 100) * flight.boost)
        * Fixed::from_ratio(15, 10);
    for side in [-Fixed::ONE, Fixed::ONE] {
        if *count == out.len() {
            break;
        }
        let mut projected = [[Fixed::ZERO; 4]; 3];
        let mut visible = true;
        for (n, target) in projected.iter_mut().enumerate() {
            let curl = if n == 2 { length } else { Fixed::ZERO };
            let local = [
                side * Fixed::from_ratio(196, 100) + flight.flame_bend() * curl,
                jet::PIVOT[0]
                    - Fixed::from_ratio(436, 10)
                    - Fixed::from_int((n) as i32) * Fixed::from_ratio(5, 10) * length,
                (Fixed::from_ratio(41, 10) - Fixed::from_ratio(5, 10) - jet::PIVOT[1])
                    * flight.spread
                    + flight.climb_tilt * Fixed::from_ratio(35, 100) * curl,
            ];
            if let Some(value) = p.project_with_yaw(
                aircraft_point(jet.position, jet.axes, local, flight.settings.jet_scale),
                heading,
            ) {
                *target = value;
            } else {
                visible = false;
                break;
            }
        }
        if !visible {
            continue;
        }
        let radius = width * flight.settings.jet_scale * projected[0][2];
        let slot = index * 2 + usize::from(side > Fixed::ZERO);
        out[*count] = PendingFlame {
            flame: Flame {
                points: projected.map(|v| [v[0], v[1]]),
                radius,
                tip_scale: projected[2][2] / projected[0][2],
                power: flight.boost,
                alpha,
                visibility: [Fixed::ZERO; 3],
                seed: Fixed::from_int((slot) as i32),
                length: length / width,
            },
            projected,
            slot,
            samples: DepthSamples::default(),
        };
        *count += 1;
    }
}

pub(super) fn finish_flames(
    mapping: &RollMapping,
    pending: &[PendingFlame],
    seen: &mut [[Fixed; 3]; 6],
    flames: &mut [Flame],
) {
    for (out, query) in flames.iter_mut().zip(pending.iter().copied()) {
        *out = query.finish(seen);
        if mapping.rolled {
            for point in &mut out.points {
                *point = mapping.unroll(*point);
            }
        }
    }
}
