use super::{BAYER, Daylight, Palette, Placements, blend};
use crate::{
    flight::{Flight, Projection},
    jet,
    math::FixedMath as _,
    world::{self, Ground, World},
};
use mirui::types::{Fixed, Fixed64};
use mirui_sprite_stack::raster::SceneCanvas;

#[derive(Clone, Copy, Default)]
struct AircraftShadow {
    center: [Fixed; 2],
    sin: Fixed,
    cos: Fixed,
    span: Fixed,
}
pub(super) struct PreparedTerrain {
    base: [Fixed; 2],
    drift: [Fixed; 2],
    cloud_sun: [Fixed; 2],
    shadows: [AircraftShadow; 3],
    shadow_count: u8,
    jet_scale: Fixed,
    shadow_bounds: [i32; 4],
    reach: Fixed,
    water: [u16; 3],
    rank: u8,
}
impl PreparedTerrain {
    pub(super) fn new(
        ground: Ground<'_>,
        flight: &Flight,
        p: &Projection,
        pal: &Palette,
        placed: &Placements,
    ) -> Self {
        let world = ground.world;
        let period = Fixed::from_int(world.period() as i32);
        let base = [
            p.position[0].rem_euclid(period) + period * Fixed::from_int(2),
            p.position[1].rem_euclid(period) + period * Fixed::from_int(2),
        ];
        let reach = Fixed::from_int(jet::LENGTH as i32)
            * Fixed::from_ratio(75, 100)
            * flight.settings.jet_scale
            + Fixed::from_int(3);
        let displacement = pal.sun.map(|sun| sun * world::MAX_HEIGHT as i32);
        let mut shadow_bounds = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
        let mut shadows = [AircraftShadow::default(); 3];
        let mut shadow_count = 0;
        for (index, placed) in placed.iter().enumerate() {
            let Some(jet) = placed else {
                continue;
            };
            let center = core::array::from_fn(|axis| {
                base[axis] + jet.position[axis] - p.position[axis] + pal.sun[axis] * jet.position[2]
            });
            let (sin, cos) = jet.heading;
            let bank = jet.roll - flight.jets[index].spin();
            let span = bank.cos().abs().max(Fixed::from_ratio(3, 10)) * flight.settings.jet_scale;
            shadows[shadow_count] = AircraftShadow {
                center,
                sin,
                cos,
                span,
            };
            shadow_count += 1;
            for axis in 0..2 {
                let far = center[axis] - displacement[axis];
                shadow_bounds[axis] =
                    shadow_bounds[axis].min((center[axis].min(far) - reach).to_int());
                shadow_bounds[axis + 2] =
                    shadow_bounds[axis + 2].max((center[axis].max(far) + reach).ceil().to_int());
            }
        }
        let elevation = ground.elevation();
        if shadow_count != 0 && !elevation.is_zero() {
            for (axis, sun) in pal.sun.iter().enumerate() {
                let offset = *sun * elevation;
                shadow_bounds[axis] -= offset.ceil().to_int();
                shadow_bounds[axis + 2] -= offset.floor().to_int();
            }
        }
        Self {
            base,
            drift: [
                (flight.time * Fixed::from_int(7))
                    .rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
                (flight.time * Fixed::from_int(3))
                    .rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
            ],
            cloud_sun: pal.sun.map(|value| value * world::CLOUD_ALT),
            shadows,
            shadow_count: shadow_count as u8,
            jet_scale: flight.settings.jet_scale,
            shadow_bounds,
            reach,
            water: [36, 6, 4]
                .map(|rate| ((flight.time * Fixed::from_int(rate)).trunc_to_int() & 1023) as u16),
            rank: (flight.settings.clouds.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_int(16))
                .round()
                .trunc_to_int() as u8,
        }
    }
}
use mirui_sprite_stack::raster::terrain::{TerrainColumn, TerrainRow, TerrainSurface, TerrainView};

struct Landscape<'a, const PLACED: bool, const DEPTH_ONLY: bool> {
    ground: Ground<'a>,
    to_world: [Fixed; 2],
    world: &'a World,
    palette: &'a Palette,
    prepared: &'a PreparedTerrain,
    levels: &'a [u8],
    materials: &'a [u8],
    fog_start: Fixed,
    far: Fixed,
    fog: Fixed,
    pixel_width: Fixed,
    cast_shadows: bool,
}

impl<const PLACED: bool, const DEPTH_ONLY: bool> TerrainSurface
    for Landscape<'_, PLACED, DEPTH_ONLY>
{
    type Sample = usize;

    fn shade(&self, color: u32) -> u32 {
        if DEPTH_ONLY || self.fog.is_zero() {
            color | 0xff00_0000
        } else {
            blend(color, self.palette.haze, self.fog)
        }
    }
    fn begin_row(&mut self, row: TerrainRow) {
        if DEPTH_ONLY {
            return;
        }
        let [xmin, ymin, xmax, ymax] = self.prepared.shadow_bounds;
        let [left, top, right, bottom] = row.bounds.map(Fixed::trunc_to_int);
        self.cast_shadows = left <= xmax && right >= xmin && top <= ymax && bottom >= ymin;
        self.pixel_width = row.pixel_width;
        self.fog = if row.depth > self.fog_start {
            (((row.depth - self.fog_start) / (self.far - self.fog_start)).sqrt()
                * Fixed::from_int(10))
            .round()
            .min(Fixed::from_int(10))
                / 10
        } else {
            Fixed::ZERO
        };
    }

    fn overlay(&self, position: [Fixed; 2], column: &mut TerrainColumn<'_, '_, '_>) {
        let prepared = self.prepared;
        if prepared.rank == 0 {
            return;
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let cloud = self.world.cloud(
            (Fixed::from_int(gx) - prepared.drift[0]).trunc_to_int(),
            (Fixed::from_int(gy) - prepared.drift[1]).trunc_to_int(),
        );
        let thick = usize::from(cloud & 15);
        if thick > 0 && cloud >> 4 < prepared.rank {
            let colors = self.palette;
            column.slab(
                world::CLOUD_ALT,
                Fixed::ONE,
                self.shade(colors.cloud_top[thick - 1]),
                self.shade(colors.cloud_under),
                colors.cloud_wall[..thick - 1]
                    .iter()
                    .map(|&color| self.shade(color)),
            );
        }
    }

    fn height_at(&self, position: [Fixed; 2]) -> Option<(Fixed, usize)> {
        if PLACED {
            return self.ground.placed_sample([
                position[0] + self.to_world[0],
                position[1] + self.to_world[1],
            ]);
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let index = self.world.ground_index(gx, gy);
        Some((Fixed::from_int(i32::from(self.levels[index])), index))
    }

    #[inline(always)]
    fn colors(&self, position: [Fixed; 2], height: Fixed, index: usize) -> [u32; 2] {
        if DEPTH_ONLY {
            return [0; 2];
        }
        let [gx, gy] = position.map(Fixed::trunc_to_int);
        let world = self.world;
        let pal = self.palette;
        let prepared = self.prepared;
        let material = usize::from(self.materials[index]);
        let mut shaded = false;
        if prepared.rank > 0 {
            let c = world.cloud(
                (Fixed::from_int(gx) + prepared.cloud_sun[0] - prepared.drift[0]).trunc_to_int(),
                (Fixed::from_int(gy) + prepared.cloud_sun[1] - prepared.drift[1]).trunc_to_int(),
            );
            shaded = c & 15 > 1 && c >> 4 < prepared.rank;
        }
        if !shaded && self.cast_shadows {
            for shadow in &prepared.shadows[..usize::from(prepared.shadow_count)] {
                let dx = Fixed::from_int(gx) - shadow.center[0] + pal.sun[0] * height;
                let dy = Fixed::from_int(gy) - shadow.center[1] + pal.sun[1] * height;
                if dx.abs() < prepared.reach && dy.abs() < prepared.reach {
                    let a = (dx * shadow.cos + dy * shadow.sin) / shadow.span;
                    let b = (dx * shadow.sin - dy * shadow.cos) / prepared.jet_scale;
                    let i = (a + (jet::PIVOT_X + Fixed::HALF)).floor().trunc_to_int();
                    let k = ((jet::PIVOT[0] - b + Fixed::from_ratio(5, 10)).floor()).trunc_to_int();
                    if (0..jet::WIDTH as i32).contains(&i)
                        && (0..jet::LENGTH as i32).contains(&k)
                        && jet::PROJECTIONS.shadow[k as usize * jet::WIDTH + i as usize] != 0
                    {
                        shaded = true;
                        break;
                    }
                }
            }
        }
        let mut top = if shaded {
            pal.top_shade[material]
        } else {
            pal.top[material]
        };
        let mut wall = if shaded {
            pal.wall_shade[material]
        } else {
            pal.wall[material]
        };
        if material < 4 {
            let noise = |x: i32, y: i32| world.noise_at(x, y);
            if material == 3 && noise(gx, gy).wrapping_add(prepared.water[0] as u8) < 120 {
                top = if shaded { pal.top_shade[2] } else { pal.foam };
            } else if !shaded && self.pixel_width < Fixed::from_ratio(18, 10) {
                let n = noise((gx + i32::from(prepared.water[1])) >> 2, gy);
                if n > 243 && noise((gx + 1024 - i32::from(prepared.water[2])) >> 2, gy + 91) > 128
                {
                    top = pal.spark[usize::from(n > 252)];
                }
            }
            wall = top;
        }
        [top, wall]
    }
}

pub(super) fn terrain_distance(altitude: Fixed) -> Fixed {
    Fixed::from_int(1500) + altitude * 3
}

pub(super) fn draw_world<const PLACED: bool, const DEPTH_ONLY: bool>(
    canvas: &mut SceneCanvas<'_, '_>,
    ground: Ground<'_>,
    p: &Projection,
    heading: (Fixed, Fixed),
    pal: &Palette,
    prepared: &PreparedTerrain,
    limit: Fixed,
) {
    mirui::trace_span!("flight.terrain");
    canvas.clear_depth();
    let altitude = p.position[2];
    let far = terrain_distance(altitude);
    let world = ground.world;
    let (levels, materials) = world.ground_planes();
    let mut surface = Landscape::<PLACED, DEPTH_ONLY> {
        ground,
        to_world: [
            p.position[0] - prepared.base[0],
            p.position[1] - prepared.base[1],
        ],
        world,
        palette: pal,
        prepared,
        levels,
        materials,
        far,
        fog_start: if p.camera.perspective > Fixed64::ZERO {
            Fixed::from_int(300) + altitude * 2
        } else {
            Fixed::MAX
        },
        fog: Fixed::ZERO,
        pixel_width: Fixed::ZERO,
        cast_shadows: false,
    };
    TerrainView {
        projection: p.camera,
        origin: prepared.base,
        altitude,
        yaw: heading,
        heights: if PLACED {
            let elevation = ground.elevation();
            [
                elevation.min(world::CLOUD_ALT),
                (elevation + Fixed::from_int(world::MAX_HEIGHT as i32))
                    .max(world::CLOUD_ALT + Fixed::from_int(world::CLOUD_LAYERS as i32)),
            ]
        } else {
            [Fixed::ZERO, Fixed::from_int(world::MAX_HEIGHT as i32)]
        },
        depth: [-Fixed::from_int(1_000_000), far.min(limit)],
    }
    .draw(canvas, &mut surface);
}
pub(super) fn draw_sky(
    canvas: &mut SceneCanvas<'_, '_>,
    p: &Projection,
    heading: (Fixed, Fixed),
    pal: &Palette,
    frame_height: usize,
) {
    mirui::trace_span!("flight.sky");
    canvas.paint_background(
        |top, width| {
            let horizon = if p.camera.perspective > Fixed64::ZERO {
                p.camera.center[1] - p.camera.divide_perspective(p.camera.scale * p.camera.slope)
            } else {
                -Fixed::from_int(1_000_000)
            };
            let mut star_offset = 0;
            let mut disk = None;
            if Fixed::from_int(top) < horizon && pal.day != Daylight::Day {
                let focal = p.camera.divide_perspective(p.camera.scale);
                if pal.day == Daylight::Night {
                    star_offset = (p.yaw * focal).trunc_to_int();
                }
                let length = (pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1]).sqrt();
                let dx = -pal.sun[0] / length;
                let dy = -pal.sun[1] / length;
                let (sn, cs) = heading;
                let along = -(dx * sn + dy * cs);
                if along > Fixed::from_ratio(2, 10) {
                    let (size, elevation) = if pal.day == Daylight::Sunset {
                        (Fixed::from_ratio(5, 100), Fixed::from_ratio(12, 100))
                    } else {
                        (Fixed::from_ratio(22, 1000), Fixed::from_ratio(3, 10))
                    };
                    disk = Some((
                        p.camera.center[0] + focal * (dx * cs - dy * sn) / along,
                        horizon - focal * elevation / along,
                        Fixed::from_int(width as i32) * size,
                    ));
                }
            }
            (horizon, star_offset, disk)
        },
        |&(horizon, _, _), y| {
            (Fixed::from_int(y) < horizon).then(|| {
                let t = (horizon - Fixed::from_int(y))
                    / (Fixed::from_int(frame_height as i32) * Fixed::from_ratio(75, 100))
                    * Fixed::from_int(7);
                let band = t.trunc_to_int();
                let threshold = (t - Fixed::from_int(band) - Fixed::from_ratio(7, 10))
                    * Fixed::from_ratio(34, 10)
                    * Fixed::from_int(16);
                (band as usize, threshold)
            })
        },
        |&(_, star_offset, disk), &gradient, x, y| {
            let mut color = pal.haze;
            if let Some((base, threshold)) = gradient {
                let dither = Fixed::from_int(BAYER[((y & 3) * 4 + (x & 3)) as usize] as i32);
                let band = base + usize::from(threshold > dither);
                color = pal.sky[band.min(7)];
                if pal.day == Daylight::Night
                    && band > 1
                    && world::hash8((((x + star_offset) & 4095) * 73 + y * 9973) as u32) < 2
                {
                    color = pal.sun_color;
                }
                if let Some((sx, sy, radius)) = disk {
                    let distance = ((Fixed::from_int(x) - sx).powi(2)
                        + (Fixed::from_int(y) - sy).powi(2))
                    .sqrt()
                        / radius;
                    if distance < Fixed::ONE {
                        color = pal.sun_color;
                    } else if distance < Fixed::from_ratio(19, 10)
                        && (Fixed::from_ratio(19, 10) - distance) * Fixed::from_int(14) > dither
                    {
                        color = pal.sun_glow;
                    }
                }
            } else if p.camera.perspective.is_zero() {
                color = pal.top[0];
            }
            color
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{flight::Settings, raster::PlacedAircraft};

    #[test]
    fn terrain_frame_preparation_keeps_every_active_shadow_caster() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.jets[2].position = [
            Fixed::from_int(2000),
            Fixed::from_int(-2000),
            Fixed::from_int(77),
        ];
        let palette = Palette::new(Daylight::Sunset);
        let projection = flight.projection(128, 128);
        for elevation in [0, -82, 137] {
            let prepared = PreparedTerrain::new(
                world.at(Some(mirui_sprite_stack::Pose::at(0, elevation, 0))),
                &flight,
                &projection,
                &palette,
                &PlacedAircraft::from_flight(&flight),
            );
            for (index, jet) in flight.jets[..flight.count].iter().enumerate() {
                for axis in 0..2 {
                    let base = projection.position[axis]
                        .rem_euclid(Fixed::from_int(world.period() as i32))
                        + Fixed::from_int(world.period() as i32) * Fixed::from_int(2);
                    assert_eq!(
                        prepared.shadows[index].center[axis],
                        base + jet.position[axis] - projection.position[axis]
                            + palette.sun[axis] * jet.position[2]
                    );
                    for h in elevation..=elevation + world::MAX_HEIGHT as i32 {
                        let center = prepared.shadows[index].center[axis] - palette.sun[axis] * h;
                        assert!(
                            Fixed::from_int(prepared.shadow_bounds[axis])
                                <= center - prepared.reach
                        );
                        assert!(
                            Fixed::from_int(prepared.shadow_bounds[axis + 2])
                                >= center + prepared.reach
                        );
                    }
                }
            }
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let _ = PreparedTerrain::new(
                    world.at(Some(Default::default())),
                    &flight,
                    &projection,
                    &palette,
                    &PlacedAircraft::from_flight(&flight),
                );
            }),
            0
        );
        assert!(core::mem::size_of::<PreparedTerrain>() <= 120);
        let mut placed = PlacedAircraft::from_flight(&flight);
        placed[1] = None;
        let previous = placed[2].unwrap();
        let yaw = Fixed::from_ratio(1, 3);
        let roll = Fixed::from_ratio(1, 5);
        placed[2] = Some(PlacedAircraft::new(
            previous.position,
            [yaw, Fixed::ZERO, roll],
            previous.model,
        ));
        let (sin, cos) = yaw.sin_cos();
        let bank = roll - flight.jets[2].spin();
        let span = bank.cos().abs().max(Fixed::from_ratio(3, 10)) * flight.settings.jet_scale;
        let prepared = PreparedTerrain::new(
            world.at(Some(Default::default())),
            &flight,
            &projection,
            &palette,
            &placed,
        );
        assert_eq!(prepared.shadow_count, 2);
        let shadow = prepared.shadows[1];
        assert_eq!((shadow.sin, shadow.cos, shadow.span), (sin, cos, span));
    }
}
