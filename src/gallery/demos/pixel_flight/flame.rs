#![doc = " Continuous curved flame shading with ordinary SourceOver and Add textures."]
use super::math::FixedMath as _;
use super::{flight::smooth, raster::Flame};
use crate::{
    render::{
        command::{CompositeMode, DrawCommand},
        renderer::RenderError,
        texture::{AlphaMode, ColorFormat, Texture, TexturePixelsMut},
    },
    types::{Color, Fixed, Fixed64, Point, Rect},
    ui::view::PaintCtx,
};
pub const TILE: usize = 256;
const PUFFS: usize = 36;
fn mix(a: Fixed, b: Fixed, t: Fixed) -> Fixed {
    a + (b - a) * t
}
fn ramp(a: Fixed, b: Fixed, x: Fixed) -> Fixed {
    smooth((x - a) / (b - a))
}
fn fract(x: Fixed) -> Fixed {
    x - x.floor()
}
fn hash(p: [Fixed; 2]) -> Fixed {
    let mut p = [
        fract(p[0] * Fixed::from_ratio(12334, 100)),
        fract(p[1] * Fixed::from_ratio(45621, 100)),
    ];
    let dot =
        p[0] * (p[0] + Fixed::from_ratio(4532, 100)) + p[1] * (p[1] + Fixed::from_ratio(4532, 100));
    p[0] += dot;
    p[1] += dot;
    fract(p[0] * p[1])
}
fn noise(p: [Fixed; 2]) -> Fixed {
    let i = p.map(Fixed::floor);
    let f = p.map(|v| {
        let f = fract(v);
        f * f * (Fixed::from_int(3) - Fixed::from_int(2) * f)
    });
    mix(
        mix(hash(i), hash([i[0] + Fixed::ONE, i[1]]), f[0]),
        mix(
            hash([i[0], i[1] + Fixed::ONE]),
            hash([i[0] + Fixed::ONE, i[1] + Fixed::ONE]),
            f[0],
        ),
        f[1],
    )
}
fn fbm(mut p: [Fixed; 2]) -> Fixed {
    let mut value = Fixed::ZERO;
    let mut amplitude = Fixed::from_ratio(5, 10);
    for _ in 0..4 {
        value += amplitude * noise(p);
        p = [
            p[0] * Fixed::from_ratio(202, 100) + Fixed::from_ratio(131, 10),
            p[1] * Fixed::from_ratio(202, 100) + Fixed::from_ratio(77, 10),
        ];
        amplitude = amplitude * Fixed::from_ratio(5, 10);
    }
    value
}
fn bezier(f: &Flame, t: Fixed) -> [Fixed; 2] {
    core::array::from_fn(|k| {
        (Fixed::ONE - t).powi(2) * f.points[0][k]
            + Fixed::from_int(2) * (Fixed::ONE - t) * t * f.points[1][k]
            + t * t * f.points[2][k]
    })
}
fn normalize(v: [Fixed; 2]) -> [Fixed; 2] {
    let largest = v[0].abs().max(v[1].abs());
    if largest.is_zero() {
        return [Fixed::ZERO; 2];
    }
    let scaled = v.map(|value| value / largest);
    let norm = (scaled[0] * scaled[0] + scaled[1] * scaled[1]).sqrt();
    scaled.map(|value| value / norm)
}
#[derive(Clone, Copy, Default)]
struct Puff {
    center: [Fixed; 2],
    inverse: Fixed64,
    cutoff: Fixed64,
    extinction: Fixed,
    t: Fixed,
    reach: Fixed,
    reach_squared: Fixed,
}

impl Puff {
    fn scaled_distance(&self, point: [Fixed; 2]) -> Fixed {
        // Small-screen radii have squares below a Q24.8 unit.
        let dx = Fixed64::from_fixed(point[0] - self.center[0]);
        let dy = Fixed64::from_fixed(point[1] - self.center[1]);
        let distance = dx * dx + dy * dy;
        // Beyond this bound density and the Q24.8 exponential are both zero.
        // It also bounds the following product to less than 16 * 2^32.
        if distance >= self.cutoff {
            Fixed::from_int(16)
        } else {
            (distance * self.inverse).to_fixed()
        }
    }
}
struct Prepared {
    flame: Flame,
    puffs: [Puff; PUFFS],
    direction: [Fixed; 2],
    across: [Fixed; 2],
    per: Fixed,
    side: Fixed,
    warp_origin: [Fixed; 2],
    streak_origin: [Fixed; 2],
    diamond_power: Fixed,
    heat_boost: Fixed,
    glow_power: Fixed,
}
impl Prepared {
    fn new(flame: Flame, time: Fixed) -> Self {
        crate::trace_span!("flight.flame_prepare");
        let direction = normalize([
            flame.points[1][0] - flame.points[0][0],
            flame.points[1][1] - flame.points[0][1],
        ]);
        let mut puffs = core::array::from_fn(|i| {
            let t = (Fixed::from_int((i) as i32) + Fixed::from_ratio(5, 10))
                / Fixed::from_int((PUFFS) as i32);
            let r = flame.radius
                * mix(Fixed::ONE, flame.tip_scale, t)
                * mix(
                    Fixed::ONE,
                    Fixed::from_ratio(5, 10),
                    ramp(Fixed::from_ratio(1, 10), Fixed::ONE, t),
                );
            let radius = Fixed64::from_fixed(r);
            let square = (radius * radius).max(Fixed64::from_ratio(1, 65536));
            Puff {
                center: bezier(&flame, t),
                inverse: Fixed64::ONE / square,
                cutoff: square * 16,
                extinction: (Fixed::ONE - t).pow_fraction(Fixed::from_ratio(75, 100)),
                t,
                reach: r,
                reach_squared: Fixed::ZERO,
            }
        });
        let extinction_sum = puffs
            .iter()
            .fold(Fixed::ZERO, |sum, puff| sum + puff.extinction);
        let glow_radius = ((flame.length / Fixed::from_int((PUFFS) as i32) * extinction_sum
            / Fixed::from_ratio(609, 10000))
        .max(Fixed::ONE)
        .ln()
            / Fixed::from_ratio(42, 100)
            + Fixed::from_ratio(1, 10000))
        .sqrt();
        for puff in &mut puffs {
            let radius = puff.reach;
            puff.reach =
                (radius * glow_radius).max(radius + Fixed::from_ratio(604, 1000) * flame.radius);
            puff.reach_squared = puff.reach * puff.reach;
        }
        let flow = time.rem_euclid(Fixed::from_int(600))
            * (Fixed::from_int(5) + Fixed::from_int(6) * flame.power);
        let length = ((flame.points[2][0] - flame.points[0][0]).powi(2)
            + (flame.points[2][1] - flame.points[0][1]).powi(2))
        .sqrt();
        let side = ramp(
            Fixed::from_ratio(3, 10),
            Fixed::from_ratio(6, 10),
            length / (flame.length * flame.radius),
        );
        Self {
            flame,
            puffs,
            direction,
            across: [-direction[1], direction[0]],
            per: flame.length / Fixed::from_int(PUFFS as i32),
            side,
            warp_origin: [
                flame.seed * Fixed::from_ratio(53, 10),
                flow * Fixed::from_ratio(6, 10),
            ],
            streak_origin: [
                flame.seed * Fixed::from_ratio(21, 10),
                flow * Fixed::from_ratio(15, 10),
            ],
            diamond_power: Fixed::from_ratio(3, 10) + Fixed::from_ratio(7, 10) * flame.power,
            heat_boost: flame.power * Fixed::from_ratio(6, 100),
            glow_power: Fixed::from_ratio(2, 10) + Fixed::from_ratio(18, 100) * flame.power,
        }
    }
    fn row_support(&self, y: Fixed) -> Option<[Fixed; 2]> {
        let mut left = Fixed::MAX;
        let mut right = Fixed::MIN;
        for puff in &self.puffs {
            let dy = y - puff.center[1];
            let remaining = puff.reach_squared - dy * dy;
            if remaining < Fixed::ZERO {
                continue;
            }
            let span = remaining.sqrt();
            left = left.min(puff.center[0] - span);
            right = right.max(puff.center[0] + span);
        }
        (left <= right).then_some([left, right])
    }
    fn shade<const CULL: bool>(&self, p: [Fixed; 2]) -> [Fixed; 4] {
        let f = &self.flame;
        if CULL
            && !self.puffs.iter().any(|puff| {
                (p[0] - puff.center[0]).powi(2) + (p[1] - puff.center[1]).powi(2)
                    <= puff.reach_squared
            })
        {
            return [Fixed::ZERO; 4];
        }
        let root = [p[0] - f.points[0][0], p[1] - f.points[0][1]];
        let q = [
            (root[0] * self.across[0] + root[1] * self.across[1]) / f.radius,
            (root[0] * self.direction[0] + root[1] * self.direction[1]) / f.radius,
        ];
        let wq = [
            q[0] * Fixed::from_ratio(6, 10) + self.warp_origin[0],
            q[1] * Fixed::from_ratio(6, 10) - self.warp_origin[1],
        ];
        let warp = [
            fbm(wq) - Fixed::from_ratio(47, 100),
            fbm([
                wq[0] + Fixed::from_ratio(317, 10),
                wq[1] + Fixed::from_ratio(113, 10),
            ]) - Fixed::from_ratio(47, 100),
        ];
        let loose = ramp(Fixed::from_ratio(2, 10), Fixed::from_int(4), q[1]);
        let pw = core::array::from_fn::<_, 2, _>(|k| {
            p[k] + (self.across[k] * warp[0]
                + self.direction[k] * warp[1] * Fixed::from_ratio(6, 10))
                * f.radius
                * loose
                * Fixed::from_ratio(11, 10)
        });
        let mut density = Fixed::ZERO;
        let mut halo = Fixed::ZERO;
        let mut sum = Fixed::ZERO;
        let mut weighted = Fixed::ZERO;
        for puff in &self.puffs {
            let dd = puff.scaled_distance(pw);
            let g = (Fixed::ONE - dd).max(Fixed::ZERO).powi(2);
            density += g * puff.extinction;
            sum += g;
            weighted += g * puff.t;
            let d0 = puff.scaled_distance(p);
            halo += (-d0 * Fixed::from_ratio(42, 100)).exp() * puff.extinction;
        }
        let per = self.per;
        density = density * per;
        halo = halo * per;
        let glow =
            (Fixed::ONE - (-halo * Fixed::from_ratio(5, 10)).exp() - Fixed::from_ratio(3, 100))
                .max(Fixed::ZERO)
                * self.glow_power;
        if density.is_zero() && glow.is_zero() {
            return [Fixed::ZERO; 4];
        }
        let t = if sum > Fixed::from_ratio(1, 1000000) {
            weighted / sum
        } else {
            Fixed::ONE
        };
        let tangent = normalize(core::array::from_fn(|k| {
            mix(
                f.points[1][k] - f.points[0][k],
                f.points[2][k] - f.points[1][k],
                t,
            )
        }));
        let center = bezier(f, t);
        let off = [pw[0] - center[0], pw[1] - center[1]];
        let x = (tangent[0] * off[1] - tangent[1] * off[0])
            / (f.radius * mix(Fixed::ONE, f.tip_scale, t));
        let y = t * f.length;
        let streak = fbm([
            x * Fixed::from_ratio(24, 10) + self.streak_origin[0],
            y * Fixed::from_ratio(8, 10) - self.streak_origin[1],
        ]);
        let fire = Fixed::ONE - (-density * Fixed::from_ratio(145, 100)).exp();
        let mut heat = fire * (Fixed::from_ratio(62, 100) + Fixed::from_ratio(6, 10) * streak);
        let cell = y / Fixed::from_ratio(22, 10) - Fixed::from_ratio(25, 100);
        let knot = (fract(cell) - Fixed::from_ratio(5, 10)).abs() * Fixed::from_int(2);
        let diamond = ramp(
            Fixed::ONE,
            Fixed::from_ratio(1, 10),
            knot + x.abs() * Fixed::from_ratio(22, 10),
        ) * (-t * Fixed::from_ratio(22, 10)).exp()
            * self.diamond_power
            * Fixed::from_int((cell >= Fixed::ZERO) as i32);
        heat = (heat
            + (diamond * Fixed::from_ratio(5, 10) + self.heat_boost)
                * ramp(Fixed::from_ratio(5, 100), Fixed::from_ratio(5, 10), fire))
        .clamp(Fixed::ZERO, Fixed::ONE);
        let mut color = [
            Fixed::from_ratio(6, 10),
            Fixed::from_ratio(7, 100),
            Fixed::from_ratio(2, 100),
        ];
        for (target, a, b) in [
            (
                [
                    Fixed::ONE,
                    Fixed::from_ratio(34, 100),
                    Fixed::from_ratio(5, 100),
                ],
                Fixed::ZERO,
                Fixed::from_ratio(28, 100),
            ),
            (
                [
                    Fixed::ONE,
                    Fixed::from_ratio(7, 10),
                    Fixed::from_ratio(2, 10),
                ],
                Fixed::from_ratio(24, 100),
                Fixed::from_ratio(52, 100),
            ),
            (
                [
                    Fixed::ONE,
                    Fixed::from_ratio(93, 100),
                    Fixed::from_ratio(66, 100),
                ],
                Fixed::from_ratio(48, 100),
                Fixed::from_ratio(78, 100),
            ),
            (
                [
                    Fixed::from_ratio(96, 100),
                    Fixed::from_ratio(98, 100),
                    Fixed::ONE,
                ],
                Fixed::from_ratio(76, 100),
                Fixed::ONE,
            ),
        ] {
            let blend = ramp(a, b, heat);
            for k in 0..3 {
                color[k] = mix(color[k], target[k], blend);
            }
        }
        let skin = (Fixed::ONE - ramp(Fixed::ZERO, Fixed::from_ratio(4, 10), t))
            * ramp(Fixed::from_ratio(3, 100), Fixed::from_ratio(14, 100), heat)
            * (Fixed::ONE - ramp(Fixed::from_ratio(2, 10), Fixed::from_ratio(5, 10), heat));
        for k in 0..3 {
            color[k] = mix(
                color[k],
                [
                    Fixed::from_ratio(34, 100),
                    Fixed::from_ratio(54, 100),
                    Fixed::ONE,
                ][k],
                skin * Fixed::from_ratio(8, 10),
            );
        }
        let alpha = ramp(Fixed::from_ratio(2, 100), Fixed::from_ratio(42, 100), heat);
        let open = mix(
            Fixed::ONE,
            ramp(-Fixed::from_ratio(5, 10), Fixed::from_ratio(3, 10), q[1]),
            self.side,
        );
        let visible = if t < Fixed::from_ratio(5, 10) {
            mix(f.visibility[0], f.visibility[1], t * Fixed::from_int(2))
        } else {
            mix(
                f.visibility[1],
                f.visibility[2],
                t * Fixed::from_int(2) - Fixed::ONE,
            )
        };
        let strength = open * visible * f.alpha;
        [
            (color[0] * alpha + glow) * strength,
            (color[1] * alpha + Fixed::from_ratio(46, 100) * glow) * strength,
            (color[2] * alpha + Fixed::from_ratio(14, 100) * glow) * strength,
            (alpha * Fixed::from_ratio(88, 100) + glow * Fixed::from_ratio(2, 10)) * strength,
        ]
    }
}
fn encode_sample(value: [Fixed; 4]) -> (Color, Color) {
    let alpha = value[3].clamp(Fixed::ZERO, Fixed::ONE);
    let mut base = [0; 3];
    let mut emission = [0; 3];
    for (k, channel) in value[..3].iter().enumerate() {
        let premul = (*channel).clamp(Fixed::ZERO, Fixed::ONE);
        let straight = if alpha > Fixed::ZERO {
            (premul / alpha).min(Fixed::ONE)
        } else {
            Fixed::ZERO
        };
        base[k] = (straight * Fixed::from_int(255)).round().to_int() as u8;
        emission[k] = ((premul - straight * alpha).max(Fixed::ZERO) * Fixed::from_int(255))
            .round()
            .to_int() as u8;
    }
    (
        Color::rgba(
            base[0],
            base[1],
            base[2],
            (alpha * Fixed::from_int(255)).round().to_int() as u8,
        ),
        Color::rgb(emission[0], emission[1], emission[2]),
    )
}
pub(super) fn composite_native(pixels: &mut TexturePixelsMut<'_>, flames: &[Flame], time: Fixed) {
    crate::trace_span!("flight.flames");
    for mut flame in flames.iter().copied() {
        if flame.radius <= Fixed::ZERO || flame.alpha <= Fixed::ZERO {
            continue;
        }
        for point in &mut flame.points {
            point[0] += Fixed::from_ratio(1, 2);
            point[1] += Fixed::from_ratio(1, 2);
        }
        let margin = flame.radius * flame.tip_scale.max(Fixed::ONE) * Fixed::from_ratio(42, 10)
            + Fixed::from_int(2);
        let low: [i32; 2] = core::array::from_fn(|k| {
            (flame
                .points
                .iter()
                .map(|p| p[k])
                .fold(Fixed::MAX, Fixed::min)
                - margin)
                .floor()
                .to_int()
                .max(0)
        });
        let high: [i32; 2] = core::array::from_fn(|k| {
            (flame
                .points
                .iter()
                .map(|p| p[k])
                .fold(Fixed::MIN, Fixed::max)
                + margin)
                .ceil()
                .to_int()
                .min(if k == 0 {
                    pixels.width().into()
                } else {
                    pixels.height().into()
                })
        });
        let prepared = Prepared::new(flame, time);
        for y in low[1]..high[1] {
            let row_y = Fixed::from_int(y) + Fixed::HALF;
            let Some([left, right]) = prepared.row_support(row_y) else {
                continue;
            };
            // Keep a pixel halo around the quantized circle/square-root bounds.
            let start = (left.floor().to_int() - 1).max(low[0]);
            let end = (right.ceil().to_int() + 1).min(high[0]);
            for x in start..end {
                let value = prepared.shade::<true>([
                    Fixed::from_int(x) + Fixed::from_ratio(1, 2),
                    Fixed::from_int(y) + Fixed::from_ratio(1, 2),
                ]);
                let (base, emission) = encode_sample(value);
                pixels.composite_pixel_int(x, y, &base, base.a, CompositeMode::SourceOver);
                if emission.r != 0 || emission.g != 0 || emission.b != 0 {
                    pixels.composite_pixel_int(x, y, &emission, 255, CompositeMode::Add);
                }
            }
        }
    }
}
pub struct Tile {
    pub base: Texture<'static>,
    pub emission: Texture<'static>,
    pub bounds: [Fixed; 4],
    pub active: bool,
    pub size: [u16; 2],
}
pub struct Flames {
    pub tiles: [Tile; 6],
    limit: usize,
}
impl Flames {
    pub fn new() -> Result<Self, RenderError> {
        Self::with_limit(TILE)
    }
    pub(super) fn with_limit(limit: usize) -> Result<Self, RenderError> {
        assert!(limit > 0 && limit <= TILE);
        let mut values = alloc::vec::Vec::with_capacity(6);
        for _ in 0..6 {
            let mut base = Texture::owned(limit as u16, limit as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            base.alpha_mode = AlphaMode::Blend;
            let mut emission = Texture::owned(limit as u16, limit as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            emission.alpha_mode = AlphaMode::Blend;
            values.push(Tile {
                base,
                emission,
                bounds: [Fixed::ZERO; 4],
                active: false,
                size: [0; 2],
            });
        }
        let tiles = match values.try_into() {
            Ok(tiles) => tiles,
            Err(_) => unreachable!("six prepared flames"),
        };
        Ok(Self { tiles, limit })
    }
    pub fn bytes(&self) -> usize {
        12 * self.limit * self.limit * 4 + core::mem::size_of::<Self>()
    }
    pub fn prepare(
        &mut self,
        flames: &[Flame],
        time: Fixed,
        display_scale: Fixed,
        viewport: [usize; 2],
    ) -> Result<(), RenderError> {
        for (i, tile) in self.tiles.iter_mut().enumerate() {
            tile.active = false;
            let Some(mut flame) = flames.get(i).copied() else {
                continue;
            };
            if flame.radius <= Fixed::ZERO || flame.alpha <= Fixed::ZERO {
                continue;
            }
            for point in &mut flame.points {
                point[0] += Fixed::from_ratio(5, 10);
                point[1] += Fixed::from_ratio(5, 10);
            }
            let margin = flame.radius * flame.tip_scale.max(Fixed::ONE) * Fixed::from_ratio(42, 10)
                + Fixed::from_int(2);
            let low = core::array::from_fn::<_, 2, _>(|k| {
                flame
                    .points
                    .iter()
                    .map(|p| p[k])
                    .fold(Fixed::MAX, Fixed::min)
                    - margin
            });
            let high = core::array::from_fn::<_, 2, _>(|k| {
                flame
                    .points
                    .iter()
                    .map(|p| p[k])
                    .fold(Fixed::MIN, Fixed::max)
                    + margin
            });
            let low = low.map(|v| v.max(Fixed::ZERO));
            let high = core::array::from_fn::<_, 2, _>(|k| {
                high[k].min(Fixed::from_int((viewport[k]) as i32))
            });
            if high[0] <= low[0] || high[1] <= low[1] {
                continue;
            }
            tile.bounds = [low[0], low[1], high[0] - low[0], high[1] - low[1]];
            tile.size = [
                ((tile.bounds[2] * display_scale)
                    .ceil()
                    .clamp(Fixed::ONE, Fixed::from_int(self.limit as i32)))
                .trunc_to_int() as u16,
                ((tile.bounds[3] * display_scale)
                    .ceil()
                    .clamp(Fixed::ONE, Fixed::from_int(self.limit as i32)))
                .trunc_to_int() as u16,
            ];
            let [width, height] = tile.size.map(usize::from);
            let prepared = Prepared::new(flame, time);
            let mut base = tile.base.pixels_mut()?;
            let mut emission = tile.emission.pixels_mut()?;
            base.bytes_mut()[..width * height * 4].fill(0);
            emission.bytes_mut()[..width * height * 4].fill(0);
            for y in 0..height {
                let row_y = low[1]
                    + (Fixed::from_int((y) as i32) + Fixed::from_ratio(5, 10))
                        / Fixed::from_int((height) as i32)
                        * tile.bounds[3];
                let Some([left, right]) = prepared.row_support(row_y) else {
                    continue;
                };
                if left >= right {
                    continue;
                }
                let start = ((((left - low[0]) / tile.bounds[2] * Fixed::from_int((width) as i32))
                    .floor())
                .trunc_to_int() as i32
                    - 1)
                .clamp(0, width as i32) as usize;
                let end = ((((right - low[0]) / tile.bounds[2] * Fixed::from_int((width) as i32))
                    .ceil())
                .trunc_to_int() as i32
                    + 1)
                .clamp(0, width as i32) as usize;
                for x in start..end {
                    let position = [
                        low[0]
                            + (Fixed::from_int((x) as i32) + Fixed::from_ratio(5, 10))
                                / Fixed::from_int((width) as i32)
                                * tile.bounds[2],
                        low[1]
                            + (Fixed::from_int((y) as i32) + Fixed::from_ratio(5, 10))
                                / Fixed::from_int((height) as i32)
                                * tile.bounds[3],
                    ];
                    let value = prepared.shade::<true>(position);
                    let (base_color, emission_color) = encode_sample(value);
                    let at = (y * width + x) * 4;
                    base.bytes_mut()[at..at + 4].copy_from_slice(&[
                        base_color.r,
                        base_color.g,
                        base_color.b,
                        base_color.a,
                    ]);
                    emission.bytes_mut()[at..at + 4].copy_from_slice(&[
                        emission_color.r,
                        emission_color.g,
                        emission_color.b,
                        emission_color.a,
                    ]);
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
                origin.x + (x) * scale,
                origin.y + (y) * scale,
                (w) * scale,
                (h) * scale,
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
            points: [
                [Fixed::ZERO, Fixed::ZERO],
                [Fixed::ZERO, Fixed::from_int(5)],
                [Fixed::from_int(2), Fixed::from_int(20)],
            ],
            radius: Fixed::from_int(2),
            tip_scale: Fixed::ONE,
            power: Fixed::ONE,
            alpha: Fixed::ONE,
            visibility: [Fixed::ONE; 3],
            seed: Fixed::ZERO,
            length: Fixed::from_int(10),
        };
        let visible = Prepared::new(flame, Fixed::from_int(3));
        let mut lit = false;
        let mut emission = false;
        for y in -4..24 {
            for x in -8..8 {
                let c = visible.shade::<true>([Fixed::from_int(x), Fixed::from_int(y)]);
                assert!(
                    c.iter()
                        .all(|v| *v >= Fixed::ZERO && *v <= Fixed::from_int(2))
                );
                lit |= c[3] > Fixed::from_ratio(1, 10);
                emission |= c[0] > c[3];
            }
        }
        assert!(lit && emission);
        flame.visibility = [Fixed::ZERO; 3];
        let hidden = Prepared::new(flame, Fixed::from_int(3));
        assert_eq!(
            hidden.shade::<true>([Fixed::from_ratio(0, 1), Fixed::from_ratio(3, 1)]),
            [Fixed::from_ratio(0, 1); 4]
        );
    }
    #[test]
    fn prepared_tiles_reuse_storage_and_fit_the_scene_budget() {
        use super::super::{
            cockpit::Cockpit,
            flight::{Flight, Settings},
            raster::Raster,
            world::World,
        };
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let flight = Flight::new(&world, Settings::default());
        let raster = Raster::new(480, 300).unwrap();
        let cockpit = Cockpit::new(480, 300, &raster.palette);
        let mut flames = Flames::new().unwrap();
        let total = world.bytes()
            + core::mem::size_of::<World>()
            + flight.bytes()
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
            points: [
                [Fixed::ZERO, Fixed::ZERO],
                [Fixed::ZERO, Fixed::from_int(5)],
                [Fixed::from_int(2), Fixed::from_int(12)],
            ],
            radius: Fixed::ONE,
            tip_scale: Fixed::ONE,
            power: Fixed::ZERO,
            alpha: Fixed::ONE,
            visibility: [Fixed::ONE; 3],
            seed: Fixed::ZERO,
            length: Fixed::from_int(8),
        };
        let allocations = crate::test_support::tracked_allocations(|| {
            flames
                .prepare(&[flame], flight.time, Fixed::from_int(3), [480, 300])
                .unwrap()
        });
        assert_eq!(allocations, 0);
        assert!(flames.tiles[0].active);
        assert!(flames.tiles[0].size.iter().all(|n| usize::from(*n) <= TILE));
        assert!(flames.tiles[1..].iter().all(|tile| !tile.active));
    }
    #[test]
    fn support_culling_preserves_quantized_shader_output() {
        for tip_scale in [
            Fixed::from_ratio(1, 10),
            Fixed::from_ratio(5, 10),
            Fixed::ONE,
            Fixed::from_int(3),
        ] {
            for length in [Fixed::ONE, Fixed::from_int(8), Fixed::from_int(25)] {
                let flame = Flame {
                    points: [
                        [Fixed::ZERO, Fixed::ZERO],
                        [Fixed::ZERO, Fixed::from_int(5)],
                        [Fixed::from_int(2), Fixed::from_int(20)],
                    ],
                    radius: Fixed::from_int(2),
                    tip_scale,
                    power: Fixed::ONE,
                    alpha: Fixed::ONE,
                    visibility: [Fixed::ONE; 3],
                    seed: Fixed::from_int(2),
                    length,
                };
                let prepared = Prepared::new(flame, Fixed::from_int(12));
                for y in -16..40 {
                    for x in -20..20 {
                        let position = [
                            Fixed::from_int(x) * Fixed::from_ratio(7, 10),
                            Fixed::from_int(y) * Fixed::from_ratio(7, 10),
                        ];
                        let optimized = prepared.shade::<true>(position);
                        let reference = prepared.shade::<false>(position);
                        if optimized != [Fixed::ZERO; 4] {
                            let [left, right] = prepared
                                .row_support(position[1])
                                .expect("nonzero sample has row support");
                            assert!(
                                position[0] >= left.floor() - Fixed::ONE
                                    && position[0] < right.ceil() + Fixed::ONE
                            );
                        }
                        assert_eq!(
                            optimized.map(|v| ((v
                                .clamp(Fixed::from_ratio(0, 1), Fixed::from_ratio(1, 1))
                                * Fixed::from_ratio(255, 1))
                            .round())
                            .trunc_to_int() as u8),
                            reference.map(|v| ((v
                                .clamp(Fixed::from_ratio(0, 1), Fixed::from_ratio(1, 1))
                                * Fixed::from_ratio(255, 1))
                            .round())
                            .trunc_to_int() as u8)
                        );
                    }
                }
            }
        }
    }
}
