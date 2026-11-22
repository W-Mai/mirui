use super::{BAYER, Daylight, Palette, blend, rgba};
use crate::{
    flight::{Flight, Projection, TRAIL_CAPACITY},
    math::FixedMath as _,
};
use mirui::types::Fixed;
use mirui_sprite_stack::raster::SceneCanvas;

pub(super) fn draw_trails(
    canvas: &mut SceneCanvas<'_, '_>,
    flight: &Flight,
    p: &Projection,
    heading: (Fixed, Fixed),
    pal: &Palette,
) {
    mirui::trace_span!("flight.trails");
    let first = usize::from(p.jet_alpha < Fixed::HALF).min(flight.count);
    let jets = &flight.jets[first..flight.count];
    if jets.is_empty() {
        return;
    }
    let [left, top, right, bottom] = canvas.color.clip;
    for jet in jets {
        let lifetime = Fixed::from_ratio(26, 10)
            * (Fixed::from_ratio(3, 10) + Fixed::from_ratio(7, 10) * jet.load);
        for wing in &jet.trails {
            let mut last: Option<[Fixed; 4]> = None;
            for n in 0..jet.trail_len {
                let point = wing[(jet.trail_head + TRAIL_CAPACITY - 1 - n) % TRAIL_CAPACITY];
                let age = (flight.time - point.born) / lifetime;
                if age >= Fixed::ONE {
                    break;
                }
                let Some(next) = p.project_with_yaw(point.position, heading) else {
                    last = None;
                    continue;
                };
                let Some(prev) = last.replace(next) else {
                    continue;
                };
                let dx = next[0] - prev[0];
                let dy = next[1] - prev[1];
                let span = dx.abs().max(dy.abs());
                if span >= Fixed::from_int(600) {
                    continue;
                }
                let width = if next[2] > Fixed::from_ratio(24, 10) {
                    2
                } else {
                    1
                };
                // Every rounded sample is between the two rounded endpoints.
                if prev[0].max(next[0]).round().to_int() + width <= left
                    || prev[0].min(next[0]).round().to_int() >= right
                    || prev[1].max(next[1]).round().to_int() + width <= top
                    || prev[1].min(next[1]).round().to_int() >= bottom
                {
                    continue;
                }
                let steps = span.ceil().to_int().max(1);
                let alpha = if age < Fixed::from_ratio(5, 10) {
                    Fixed::ONE
                } else {
                    (Fixed::ONE - age) * Fixed::from_int(2)
                };
                let color = pal.trails[if age < Fixed::from_ratio(25, 100) {
                    0
                } else if age < Fixed::from_ratio(6, 10) {
                    1
                } else {
                    2
                }];
                for k in 0..steps {
                    let x = (prev[0] + dx * k / steps).round().to_int();
                    let y = (prev[1] + dy * k / steps).round().to_int();
                    for yy in y..y + width {
                        for xx in x..x + width {
                            if !canvas.color.contains(xx, yy) {
                                continue;
                            }
                            let at = canvas.color.index(xx, yy);
                            if canvas.depth[at] >= next[3] - Fixed::ONE
                                && alpha * Fixed::from_int(16)
                                    > Fixed::from_int(
                                        (BAYER[((yy & 3) * 4 + (xx & 3)) as usize]) as i32,
                                    )
                            {
                                canvas.put(at, color);
                            }
                        }
                    }
                }
            }
        }
    }
}
pub(super) fn draw_smoke(
    canvas: &mut SceneCanvas<'_, '_>,
    flight: &Flight,
    p: &Projection,
    heading: (Fixed, Fixed),
    day: Daylight,
) {
    mirui::trace_span!("flight.smoke");
    for smoke in &flight.smoke {
        let age = flight.time - smoke.born;
        if smoke.life <= Fixed::ZERO || age < Fixed::ZERO || age >= smoke.life {
            continue;
        }
        let t = age / smoke.life;
        let push = (Fixed::ONE - (-age * Fixed::from_int(5)).exp()) / Fixed::from_int(5);
        let Some(point) = p.project_with_yaw(
            [
                smoke.position[0] + smoke.push[0] * push,
                smoke.position[1] + smoke.push[1] * push,
                smoke.position[2] + age * Fixed::from_ratio(25, 10),
            ],
            heading,
        ) else {
            continue;
        };
        let radius = (smoke.size * (Fixed::ONE + Fixed::from_ratio(24, 10) * t) * point[2])
            .max(Fixed::from_ratio(8, 10));
        if radius > Fixed::from_int(60) {
            continue;
        }
        let alpha = smoke.strength
            * (Fixed::ONE - t).powi(2)
            * if t < Fixed::from_ratio(8, 100) {
                t / Fixed::from_ratio(8, 100)
            } else {
                Fixed::ONE
            }
            * if day == Daylight::Night {
                Fixed::from_ratio(55, 100)
            } else {
                Fixed::ONE
            };
        let shade = Fixed::from_int(26) + Fixed::from_int(70) * t;
        let color = rgba(shade, shade + Fixed::ONE, shade + Fixed::from_int(7));
        for y in ((point[1] - radius).floor())
            .trunc_to_int()
            .max(canvas.color.clip[1])
            ..=((point[1] + radius).ceil())
                .trunc_to_int()
                .min(canvas.color.clip[3] - 1)
        {
            for x in ((point[0] - radius).floor())
                .trunc_to_int()
                .max(canvas.color.clip[0])
                ..=((point[0] + radius).ceil())
                    .trunc_to_int()
                    .min(canvas.color.clip[2] - 1)
            {
                let d = ((Fixed::from_int(x) - point[0]).powi(2)
                    + (Fixed::from_int(y) - point[1]).powi(2))
                    / (radius * radius);
                let at = canvas.color.index(x, y);
                if d <= Fixed::ONE && canvas.depth[at] >= point[3] - Fixed::ONE {
                    canvas.put(
                        at,
                        blend(
                            canvas.get(at),
                            color,
                            alpha * (Fixed::ONE - d * Fixed::from_ratio(7, 10)),
                        ),
                    );
                }
            }
        }
    }
}
