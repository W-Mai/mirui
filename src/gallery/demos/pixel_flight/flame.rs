//! Continuous curved flame shading with ordinary SourceOver and Add textures.

use super::{flight::smooth, raster::Flame};
use crate::{
    render::{
        command::{CompositeMode, DrawCommand},
        renderer::RenderError,
        texture::{AlphaMode, ColorFormat, Texture},
    },
    types::{Fixed, Point, Rect},
    ui::view::PaintCtx,
};

pub const TILE: usize = 256;
const PUFFS: usize = 36;
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ramp(a: f32, b: f32, x: f32) -> f32 {
    smooth((x - a) / (b - a))
}
fn fract(x: f32) -> f32 {
    x - x.floor()
}
fn hash(p: [f32; 2]) -> f32 {
    let mut p = [fract(p[0] * 123.34), fract(p[1] * 456.21)];
    let dot = p[0] * (p[0] + 45.32) + p[1] * (p[1] + 45.32);
    p[0] += dot;
    p[1] += dot;
    fract(p[0] * p[1])
}
fn noise(p: [f32; 2]) -> f32 {
    let i = p.map(f32::floor);
    let f = p.map(|v| {
        let f = fract(v);
        f * f * (3.0 - 2.0 * f)
    });
    mix(
        mix(hash(i), hash([i[0] + 1.0, i[1]]), f[0]),
        mix(
            hash([i[0], i[1] + 1.0]),
            hash([i[0] + 1.0, i[1] + 1.0]),
            f[0],
        ),
        f[1],
    )
}
fn fbm(mut p: [f32; 2]) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..4 {
        value += amplitude * noise(p);
        p = [p[0] * 2.02 + 13.1, p[1] * 2.02 + 7.7];
        amplitude *= 0.5;
    }
    value
}
fn bezier(f: &Flame, t: f32) -> [f32; 2] {
    core::array::from_fn(|k| {
        (1.0 - t).powi(2) * f.points[0][k]
            + 2.0 * (1.0 - t) * t * f.points[1][k]
            + t * t * f.points[2][k]
    })
}
fn normalize(v: [f32; 2]) -> [f32; 2] {
    let n = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-3);
    [v[0] / n, v[1] / n]
}

#[derive(Clone, Copy, Default)]
struct Puff {
    center: [f32; 2],
    inverse: f32,
    extinction: f32,
    t: f32,
    reach: f32,
}
struct Prepared {
    flame: Flame,
    puffs: [Puff; PUFFS],
    direction: [f32; 2],
    across: [f32; 2],
    flow: f32,
}
impl Prepared {
    fn new(flame: Flame, time: f32) -> Self {
        let direction = normalize([
            flame.points[1][0] - flame.points[0][0],
            flame.points[1][1] - flame.points[0][1],
        ]);
        // Outside every support circle both density and the thresholded glow are zero.
        // The warp's displacement is bounded by 0.604 nozzle radii.
        let extinction_sum: f32 = (0..PUFFS)
            .map(|i| (1.0 - (i as f32 + 0.5) / PUFFS as f32).powf(0.75))
            .sum();
        let glow_radius = ((flame.length / PUFFS as f32 * extinction_sum / 0.0609)
            .max(1.0)
            .ln()
            / 0.42
            + 0.0001)
            .sqrt();
        let puffs = core::array::from_fn(|i| {
            let t = (i as f32 + 0.5) / PUFFS as f32;
            let r = flame.radius * mix(1.0, flame.tip_scale, t) * mix(1.0, 0.5, ramp(0.1, 1.0, t));
            Puff {
                center: bezier(&flame, t),
                inverse: 1.0 / (r * r).max(1e-6),
                extinction: (1.0 - t).powf(0.75),
                t,
                reach: (r * glow_radius).max(r + 0.604 * flame.radius),
            }
        });
        Self {
            flame,
            puffs,
            direction,
            across: [-direction[1], direction[0]],
            flow: time.rem_euclid(600.0) * (5.0 + 6.0 * flame.power),
        }
    }
    fn shade<const CULL: bool>(&self, p: [f32; 2]) -> [f32; 4] {
        let f = &self.flame;
        if CULL
            && !self.puffs.iter().any(|puff| {
                (p[0] - puff.center[0]).powi(2) + (p[1] - puff.center[1]).powi(2)
                    <= puff.reach * puff.reach
            })
        {
            return [0.0; 4];
        }
        let root = [p[0] - f.points[0][0], p[1] - f.points[0][1]];
        let q = [
            (root[0] * self.across[0] + root[1] * self.across[1]) / f.radius,
            (root[0] * self.direction[0] + root[1] * self.direction[1]) / f.radius,
        ];
        let wq = [q[0] * 0.6 + f.seed * 5.3, q[1] * 0.6 - self.flow * 0.6];
        let warp = [fbm(wq) - 0.47, fbm([wq[0] + 31.7, wq[1] + 11.3]) - 0.47];
        let loose = ramp(0.2, 4.0, q[1]);
        let pw = core::array::from_fn::<_, 2, _>(|k| {
            p[k] + (self.across[k] * warp[0] + self.direction[k] * warp[1] * 0.6)
                * f.radius
                * loose
                * 1.1
        });
        let mut density = 0.0;
        let mut halo = 0.0;
        let mut sum = 0.0;
        let mut weighted = 0.0;
        for puff in &self.puffs {
            let dd = ((pw[0] - puff.center[0]).powi(2) + (pw[1] - puff.center[1]).powi(2))
                * puff.inverse;
            let g = (1.0 - dd).max(0.0).powi(2);
            density += g * puff.extinction;
            sum += g;
            weighted += g * puff.t;
            let d0 =
                ((p[0] - puff.center[0]).powi(2) + (p[1] - puff.center[1]).powi(2)) * puff.inverse;
            halo += (-d0 * 0.42).exp() * puff.extinction;
        }
        let per = f.length / PUFFS as f32;
        density *= per;
        halo *= per;
        let t = if sum > 1e-6 { weighted / sum } else { 1.0 };
        let tangent = normalize(core::array::from_fn(|k| {
            mix(
                f.points[1][k] - f.points[0][k],
                f.points[2][k] - f.points[1][k],
                t,
            )
        }));
        let center = bezier(f, t);
        let off = [pw[0] - center[0], pw[1] - center[1]];
        let x = (tangent[0] * off[1] - tangent[1] * off[0]) / (f.radius * mix(1.0, f.tip_scale, t));
        let y = t * f.length;
        let streak = fbm([x * 2.4 + f.seed * 2.1, y * 0.8 - self.flow * 1.5]);
        let fire = 1.0 - (-density * 1.45).exp();
        let mut heat = fire * (0.62 + 0.6 * streak);
        let cell = y / 2.2 - 0.25;
        let knot = (fract(cell) - 0.5).abs() * 2.0;
        let diamond = ramp(1.0, 0.1, knot + x.abs() * 2.2)
            * (-t * 2.2).exp()
            * (0.3 + 0.7 * f.power)
            * f32::from(cell >= 0.0);
        heat = (heat + (diamond * 0.5 + f.power * 0.06) * ramp(0.05, 0.5, fire)).clamp(0.0, 1.0);
        let mut color = [0.6, 0.07, 0.02];
        for (target, a, b) in [
            ([1.0, 0.34, 0.05], 0.0, 0.28),
            ([1.0, 0.7, 0.2], 0.24, 0.52),
            ([1.0, 0.93, 0.66], 0.48, 0.78),
            ([0.96, 0.98, 1.0], 0.76, 1.0),
        ] {
            let blend = ramp(a, b, heat);
            for k in 0..3 {
                color[k] = mix(color[k], target[k], blend);
            }
        }
        let skin =
            (1.0 - ramp(0.0, 0.4, t)) * ramp(0.03, 0.14, heat) * (1.0 - ramp(0.2, 0.5, heat));
        for k in 0..3 {
            color[k] = mix(color[k], [0.34, 0.54, 1.0][k], skin * 0.8);
        }
        let alpha = ramp(0.02, 0.42, heat);
        let glow = (1.0 - (-halo * 0.5).exp() - 0.03).max(0.0) * (0.2 + 0.18 * f.power);
        let length = ((f.points[2][0] - f.points[0][0]).powi(2)
            + (f.points[2][1] - f.points[0][1]).powi(2))
        .sqrt();
        let side = ramp(0.3, 0.6, length / (f.length * f.radius));
        let open = mix(1.0, ramp(-0.5, 0.3, q[1]), side);
        let visible = if t < 0.5 {
            mix(f.visibility[0], f.visibility[1], t * 2.0)
        } else {
            mix(f.visibility[1], f.visibility[2], t * 2.0 - 1.0)
        };
        let strength = open * visible * f.alpha;
        [
            (color[0] * alpha + glow) * strength,
            (color[1] * alpha + 0.46 * glow) * strength,
            (color[2] * alpha + 0.14 * glow) * strength,
            (alpha * 0.88 + glow * 0.2) * strength,
        ]
    }
}

pub struct Tile {
    pub base: Texture<'static>,
    pub emission: Texture<'static>,
    pub bounds: [f32; 4],
    pub active: bool,
    pub size: [u16; 2],
}
pub struct Flames {
    pub tiles: [Tile; 6],
}
impl Flames {
    pub fn new() -> Result<Self, RenderError> {
        let mut values = alloc::vec::Vec::with_capacity(6);
        for _ in 0..6 {
            let mut base = Texture::owned(TILE as u16, TILE as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            base.alpha_mode = AlphaMode::Blend;
            let mut emission = Texture::owned(TILE as u16, TILE as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            emission.alpha_mode = AlphaMode::Blend;
            values.push(Tile {
                base,
                emission,
                bounds: [0.0; 4],
                active: false,
                size: [0; 2],
            });
        }
        let tiles = match values.try_into() {
            Ok(tiles) => tiles,
            Err(_) => unreachable!("six prepared flames"),
        };
        Ok(Self { tiles })
    }
    pub fn bytes(&self) -> usize {
        12 * TILE * TILE * 4 + core::mem::size_of::<Self>()
    }
    pub fn prepare(
        &mut self,
        flames: &[Flame],
        time: f32,
        display_scale: f32,
        viewport: [usize; 2],
    ) -> Result<(), RenderError> {
        for (i, tile) in self.tiles.iter_mut().enumerate() {
            tile.active = false;
            let Some(mut flame) = flames.get(i).copied() else {
                continue;
            };
            if flame.radius <= 0.0 || flame.alpha <= 0.0 {
                continue;
            }
            for point in &mut flame.points {
                point[0] += 0.5;
                point[1] += 0.5;
            }
            let margin = flame.radius * flame.tip_scale.max(1.0) * 4.2 + 2.0;
            let low = core::array::from_fn::<_, 2, _>(|k| {
                flame
                    .points
                    .iter()
                    .map(|p| p[k])
                    .fold(f32::INFINITY, f32::min)
                    - margin
            });
            let high = core::array::from_fn::<_, 2, _>(|k| {
                flame
                    .points
                    .iter()
                    .map(|p| p[k])
                    .fold(f32::NEG_INFINITY, f32::max)
                    + margin
            });
            let low = low.map(|v| v.max(0.0));
            let high = core::array::from_fn::<_, 2, _>(|k| high[k].min(viewport[k] as f32));
            if high[0] <= low[0] || high[1] <= low[1] {
                continue;
            }
            tile.bounds = [low[0], low[1], high[0] - low[0], high[1] - low[1]];
            tile.size = [
                (tile.bounds[2] * display_scale)
                    .ceil()
                    .clamp(1.0, TILE as f32) as u16,
                (tile.bounds[3] * display_scale)
                    .ceil()
                    .clamp(1.0, TILE as f32) as u16,
            ];
            let [width, height] = tile.size.map(usize::from);
            let prepared = Prepared::new(flame, time);
            let mut base = tile.base.pixels_mut()?;
            let mut emission = tile.emission.pixels_mut()?;
            base.bytes_mut()[..width * height * 4].fill(0);
            emission.bytes_mut()[..width * height * 4].fill(0);
            for y in 0..height {
                let row_y = low[1] + (y as f32 + 0.5) / height as f32 * tile.bounds[3];
                let mut left = f32::INFINITY;
                let mut right = f32::NEG_INFINITY;
                for puff in &prepared.puffs {
                    let dy = row_y - puff.center[1];
                    let remaining = puff.reach * puff.reach - dy * dy;
                    if remaining < 0.0 {
                        continue;
                    }
                    let span = remaining.sqrt();
                    left = left.min(puff.center[0] - span);
                    right = right.max(puff.center[0] + span);
                }
                if left >= right {
                    continue;
                }
                let start = (((left - low[0]) / tile.bounds[2] * width as f32).floor() as i32 - 1)
                    .clamp(0, width as i32) as usize;
                let end = (((right - low[0]) / tile.bounds[2] * width as f32).ceil() as i32 + 1)
                    .clamp(0, width as i32) as usize;
                for x in start..end {
                    let position = [
                        low[0] + (x as f32 + 0.5) / width as f32 * tile.bounds[2],
                        low[1] + (y as f32 + 0.5) / height as f32 * tile.bounds[3],
                    ];
                    let value = prepared.shade::<true>(position);
                    let alpha = value[3].clamp(0.0, 1.0);
                    let at = (y * width + x) * 4;
                    for (k, channel) in value[..3].iter().enumerate() {
                        let premul = channel.clamp(0.0, 1.0);
                        let straight = if alpha > 1e-6 {
                            (premul / alpha).min(1.0)
                        } else {
                            0.0
                        };
                        base.bytes_mut()[at + k] = (straight * 255.0).round() as u8;
                        emission.bytes_mut()[at + k] =
                            ((premul - straight * alpha).max(0.0) * 255.0).round() as u8;
                    }
                    base.bytes_mut()[at + 3] = (alpha * 255.0).round() as u8;
                    emission.bytes_mut()[at + 3] = 255;
                }
            }
            tile.active = true;
        }
        Ok(())
    }
    pub fn paint(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        for tile in self.tiles.iter().filter(|tile| tile.active) {
            let [x, y, w, h] = tile.bounds;
            let rect = Rect::new(
                origin.x + Fixed::from_f32(x) * scale,
                origin.y + Fixed::from_f32(y) * scale,
                Fixed::from_f32(w) * scale,
                Fixed::from_f32(h) * scale,
            );
            for (texture, composite) in [
                (&tile.base, CompositeMode::SourceOver),
                (&tile.emission, CompositeMode::Add),
            ] {
                let pixels = texture.read_pixels().expect("owned flame pixels");
                let [width, height] = tile.size;
                let length = usize::from(width) * usize::from(height) * 4;
                let mut view = Texture::from_ref(
                    &pixels.bytes()[..length],
                    width,
                    height,
                    ColorFormat::RGBA8888,
                )
                .expect("bounded flame tile")
                .with_cache_revision(texture.cache_revision);
                view.alpha_mode = AlphaMode::Blend;
                paint.draw(&DrawCommand::Blit {
                    pos: Point::new(rect.x, rect.y),
                    size: Point::new(rect.w, rect.h),
                    transform: paint.transform(),
                    quad: None,
                    texture: &view,
                    opa: 255,
                    radius: Fixed::ZERO,
                    composite,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flame_shading_is_finite_emissive_and_visibility_gated() {
        let mut flame = Flame {
            points: [[0.0, 0.0], [0.0, 5.0], [2.0, 20.0]],
            radius: 2.0,
            tip_scale: 1.0,
            power: 1.0,
            alpha: 1.0,
            visibility: [1.0; 3],
            seed: 0.0,
            length: 10.0,
        };
        let visible = Prepared::new(flame, 3.0);
        let mut lit = false;
        let mut emission = false;
        for y in -4..24 {
            for x in -8..8 {
                let c = visible.shade::<true>([x as f32, y as f32]);
                assert!(c.iter().all(|v| v.is_finite()));
                lit |= c[3] > 0.1;
                emission |= c[0] > c[3];
            }
        }
        assert!(lit && emission);
        flame.visibility = [0.0; 3];
        let hidden = Prepared::new(flame, 3.0);
        assert_eq!(hidden.shade::<true>([0.0, 3.0]), [0.0; 4]);
    }
    #[test]
    fn prepared_tiles_reuse_storage_and_fit_the_scene_budget() {
        use super::super::{
            cockpit::Cockpit,
            flight::{Flight, Settings},
            raster::Raster,
            world::World,
        };
        let world = World::new(7, 0.33);
        let flight = Flight::new(&world, Settings::default());
        let raster = Raster::new(480, 300).unwrap();
        let cockpit = Cockpit::new(480, 300, &raster.palette);
        let mut flames = Flames::new().unwrap();
        let total = world.bytes()
            + core::mem::size_of::<World>()
            + core::mem::size_of::<Flight>()
            + raster.bytes()
            + cockpit.bytes()
            + flames.bytes();
        eprintln!(
            "flight resident bytes: world={} raster={} cockpit={} flames={} total={total}",
            world.bytes(),
            raster.bytes(),
            cockpit.bytes(),
            flames.bytes()
        );
        assert!(total < 16 * 1024 * 1024);
        let flame = Flame {
            points: [[0.0, 0.0], [0.0, 5.0], [2.0, 12.0]],
            radius: 1.0,
            tip_scale: 1.0,
            power: 0.0,
            alpha: 1.0,
            visibility: [1.0; 3],
            seed: 0.0,
            length: 8.0,
        };
        let allocations = crate::test_support::tracked_allocations(|| {
            flames
                .prepare(&[flame], flight.time, 3.0, [480, 300])
                .unwrap()
        });
        assert_eq!(allocations, 0);
        assert!(flames.tiles[0].active);
        assert!(flames.tiles[0].size.iter().all(|n| usize::from(*n) <= TILE));
        assert!(flames.tiles[1..].iter().all(|tile| !tile.active));
    }
    #[test]
    fn support_culling_preserves_quantized_shader_output() {
        for tip_scale in [0.1, 0.5, 1.0, 3.0] {
            for length in [1.0, 8.0, 25.0] {
                let flame = Flame {
                    points: [[0.0, 0.0], [0.0, 5.0], [2.0, 20.0]],
                    radius: 2.0,
                    tip_scale,
                    power: 1.0,
                    alpha: 1.0,
                    visibility: [1.0; 3],
                    seed: 2.0,
                    length,
                };
                let prepared = Prepared::new(flame, 12.0);
                for y in -16..40 {
                    for x in -20..20 {
                        let position = [x as f32 * 0.7, y as f32 * 0.7];
                        let optimized = prepared.shade::<true>(position);
                        let reference = prepared.shade::<false>(position);
                        assert_eq!(
                            optimized.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8),
                            reference.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
                        );
                    }
                }
            }
        }
    }
}
