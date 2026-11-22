#![doc = " Continuous curved flame shading with ordinary SourceOver and Add textures."]
use super::math::FixedMath as _;
use super::{
    flight::smooth,
    raster::{Canvas, Flame},
};
use mirui::{
    render::{
        command::{CompositeMode, DrawCommand},
        renderer::RenderError,
        texture::{AlphaMode, ColorFormat, Texture, TexturePixelsMut},
    },
    types::{Color, Fixed, Fixed64, Point, Rect},
    ui::view::PaintCtx,
};
pub const TILE: usize = 256;
mod sampling;

use sampling::{SampleAxis, ScreenFlame};

const PUFFS: usize = 36;
// Q24.8 samples of (1 - t)^0.75 at the fixed puff centers.
const EXTINCTION: [u8; PUFFS] = [
    253, 247, 242, 236, 230, 225, 219, 214, 208, 203, 197, 191, 186, 179, 173, 167, 161, 155, 149,
    142, 136, 130, 122, 115, 109, 101, 94, 86, 79, 71, 63, 54, 44, 35, 24, 11,
];
const EXTINCTION_SUM: Fixed = {
    let mut sum = 0;
    let mut i = 0;
    while i < PUFFS {
        sum += EXTINCTION[i] as i32;
        i += 1;
    }
    Fixed::from_ratio(sum, 256)
};
const PUFF_DISTANCE_LIMIT: i32 = 16;
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
    radius: Fixed,
    reach_squared: Fixed,
}

struct PuffRow {
    y: Fixed,
    density_mask: u64,
    halo_mask: u64,
}

impl Puff {
    fn density_row_may_contribute(&self, interval: [Fixed; 2]) -> bool {
        let closest = self.center[1].clamp(interval[0], interval[1]);
        if self.radius >= Fixed::ZERO && self.radius <= Fixed::ONE {
            // One Q24.8 quantum covers reciprocal truncation in this radius range.
            (closest - self.center[1]).abs() <= self.radius + Fixed::from_ratio(1, 256)
        } else {
            self.scaled_distance([self.center[0], closest]) < Fixed::ONE
        }
    }
    fn scaled_distance(&self, point: [Fixed; 2]) -> Fixed {
        // Small-screen radii have squares below a Q24.8 unit.
        let dx = Fixed64::from_fixed(point[0] - self.center[0]);
        let dy = Fixed64::from_fixed(point[1] - self.center[1]);
        let distance = dx * dx + dy * dy;
        // Beyond this bound density and the Q24.8 exponential are both zero.
        // It also bounds the following product to less than 16 * 2^32.
        if distance >= self.cutoff {
            Fixed::from_int(PUFF_DISTANCE_LIMIT)
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
        mirui::trace_span!("flight.flame_prepare");
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
                cutoff: square * i64::from(PUFF_DISTANCE_LIMIT),
                extinction: Fixed::from_ratio(i32::from(EXTINCTION[i]), 256),
                t,
                radius: r,
                reach_squared: Fixed::ZERO,
            }
        });
        let glow_radius = ((flame.length / Fixed::from_int((PUFFS) as i32) * EXTINCTION_SUM
            / Fixed::from_ratio(609, 10000))
        .max(Fixed::ONE)
        .ln()
            / Fixed::from_ratio(42, 100)
            + Fixed::from_ratio(1, 10000))
        .sqrt();
        for puff in &mut puffs {
            let radius = puff.radius;
            let reach =
                (radius * glow_radius).max(radius + Fixed::from_ratio(604, 1000) * flame.radius);
            puff.reach_squared = reach * reach;
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
    fn warp_y_interval(&self) -> [Fixed; 2] {
        // The noise octave weights bound both warp components within half a unit.
        let a = [-Fixed::HALF * self.across[1], Fixed::HALF * self.across[1]];
        let b = [
            -Fixed::HALF * self.direction[1],
            Fixed::HALF * self.direction[1],
        ]
        .map(|v| v * Fixed::from_ratio(6, 10));
        let ends = [
            (a[0].min(a[1]) + b[0].min(b[1])) * self.flame.radius,
            (a[0].max(a[1]) + b[0].max(b[1])) * self.flame.radius,
        ];
        [
            ends[0].min(ends[1]).min(Fixed::ZERO) * Fixed::from_ratio(11, 10),
            ends[0].max(ends[1]).max(Fixed::ZERO) * Fixed::from_ratio(11, 10),
        ]
    }
    fn row_candidates(&self, y: Fixed, warp_y: [Fixed; 2]) -> PuffRow {
        let interval = warp_y.map(|d| y + d);
        let mut density_mask = 0;
        let mut halo_mask = 0;
        for (index, puff) in self.puffs.iter().enumerate() {
            let bit = 1 << index;
            if puff.density_row_may_contribute(interval) {
                density_mask |= bit;
            }
            let dy = Fixed64::from_fixed(y - puff.center[1]);
            if dy * dy < puff.cutoff {
                halo_mask |= bit;
            }
        }
        PuffRow {
            y,
            density_mask,
            halo_mask,
        }
    }
    fn shade_row<const CULL: bool>(&self, x: Fixed, row: &PuffRow) -> [Fixed; 4] {
        let p = [x, row.y];
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
        let pw = if row.density_mask != 0 {
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
            core::array::from_fn::<_, 2, _>(|k| {
                p[k] + (self.across[k] * warp[0]
                    + self.direction[k] * warp[1] * Fixed::from_ratio(6, 10))
                    * f.radius
                    * loose
                    * Fixed::from_ratio(11, 10)
            })
        } else {
            p
        };
        let mut density = Fixed::ZERO;
        let mut halo = Fixed::ZERO;
        let mut sum = Fixed::ZERO;
        let mut weighted = Fixed::ZERO;
        let mut active = row.density_mask | row.halo_mask;
        while active != 0 {
            let index = active.trailing_zeros() as usize;
            let bit = 1 << index;
            active &= active - 1;
            let puff = &self.puffs[index];
            if row.density_mask & bit != 0 {
                let dd = puff.scaled_distance(pw);
                if dd < Fixed::ONE {
                    let g = (Fixed::ONE - dd).powi(2);
                    density += g * puff.extinction;
                    sum += g;
                    weighted += g * puff.t;
                }
            }
            if row.halo_mask & bit != 0 {
                let d0 = puff.scaled_distance(p);
                if d0 < Fixed::from_int(PUFF_DISTANCE_LIMIT) {
                    halo += (-d0 * Fixed::from_ratio(42, 100)).exp() * puff.extinction;
                }
            }
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
        let finish = |color: [Fixed; 3], alpha: Fixed| {
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
        };
        // Zero density cannot activate body heat or alpha; retain the weighted visibility.
        if density.is_zero() {
            return finish([Fixed::ZERO; 3], Fixed::ZERO);
        }
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
        finish(color, alpha)
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
pub(super) fn paint_region(
    canvas: &mut Canvas<'_>,
    flames: &[Flame],
    time: Fixed,
) -> Result<(), RenderError> {
    if flames.is_empty() {
        return Ok(());
    }
    let origin = canvas.origin;
    let clip = canvas.clip;
    canvas.with_pixels(|pixels| composite_region(pixels, origin, clip, flames, time))
}

fn composite_region(
    pixels: &mut TexturePixelsMut<'_>,
    [origin_x, origin_y]: [i32; 2],
    clip: [i32; 4],
    flames: &[Flame],
    time: Fixed,
) {
    mirui::trace_span!("flight.flames");
    for &flame in flames {
        let Some(screen) = ScreenFlame::new(flame) else {
            continue;
        };
        let [left, top, right, bottom] = screen.native_clip(clip);
        if left >= right || top >= bottom {
            continue;
        }
        Prepared::new(screen.flame, time).sample::<false>(
            [left, top, right, bottom],
            [SampleAxis::UNIT; 2],
            |x, y, base, emission| {
                if base.a != 0 {
                    pixels.composite_pixel(
                        x - origin_x,
                        y - origin_y,
                        &base,
                        CompositeMode::SourceOver,
                    );
                }
                if emission.r != 0 || emission.g != 0 || emission.b != 0 {
                    pixels.composite_pixel(
                        x - origin_x,
                        y - origin_y,
                        &emission,
                        CompositeMode::Add,
                    );
                }
            },
        );
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
            let Some(screen) = flames.get(i).copied().and_then(ScreenFlame::new) else {
                continue;
            };
            let low = screen.low.map(|value| value.max(Fixed::ZERO));
            let high: [Fixed; 2] =
                core::array::from_fn(|k| screen.high[k].min(Fixed::from_int(viewport[k] as i32)));
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
            let [width, height] = tile.size.map(i32::from);
            let axes = core::array::from_fn(|k| SampleAxis {
                origin: low[k],
                extent: tile.bounds[2 + k],
                pixels: Fixed::from_int(i32::from(tile.size[k])),
            });
            let mut base = tile.base.pixels_mut()?;
            let mut emission = tile.emission.pixels_mut()?;
            let bytes = (width * height * 4) as usize;
            base.bytes_mut()[..bytes].fill(0);
            emission.bytes_mut()[..bytes].fill(0);
            Prepared::new(screen.flame, time).sample::<true>(
                [0, 0, width, height],
                axes,
                |x, y, color, glow| {
                    let at = ((y * width + x) * 4) as usize;
                    base.bytes_mut()[at..at + 4]
                        .copy_from_slice(&[color.r, color.g, color.b, color.a]);
                    emission.bytes_mut()[at..at + 4]
                        .copy_from_slice(&[glow.r, glow.g, glow.b, glow.a]);
                },
            );
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
#[path = "../tests/unit/flame.rs"]
mod tests;
