#![doc = " Height-field scan conversion into reusable ordinary Texture storage."]
use super::math::FixedMath as _;
use super::{
    flight::{Aircraft, Flight, Projection, TRAIL_CAPACITY},
    jet,
    world::{self, World},
};
use crate::render::{
    renderer::RenderError,
    texture::{ColorFormat, Texture},
};
use crate::types::{Fixed, Fixed64};
use alloc::{vec, vec::Vec};
pub const MAX_WIDTH: usize = 480;
pub const MAX_HEIGHT: usize = 300;
pub const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
pub fn rgba(r: Fixed, g: Fixed, b: Fixed) -> u32 {
    u32::from_le_bytes([
        (r.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        (g.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        (b.clamp(Fixed::ZERO, Fixed::from_int(255))).trunc_to_int() as u8,
        255,
    ])
}
pub fn blend(bg: u32, fg: u32, amount: Fixed) -> u32 {
    let a = bg.to_le_bytes();
    let b = fg.to_le_bytes();
    let t = amount.clamp(Fixed::ZERO, Fixed::ONE);
    rgba(
        Fixed::from_int((a[0]) as i32)
            + (Fixed::from_int((b[0]) as i32) - Fixed::from_int((a[0]) as i32)) * t,
        Fixed::from_int((a[1]) as i32)
            + (Fixed::from_int((b[1]) as i32) - Fixed::from_int((a[1]) as i32)) * t,
        Fixed::from_int((a[2]) as i32)
            + (Fixed::from_int((b[2]) as i32) - Fixed::from_int((a[2]) as i32)) * t,
    )
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Daylight {
    #[default]
    Day,
    Sunset,
    Night,
}
impl Daylight {
    pub fn next(self) -> Self {
        match self {
            Self::Day => Self::Sunset,
            Self::Sunset => Self::Night,
            Self::Night => Self::Day,
        }
    }
}
pub struct Palette {
    pub day: Daylight,
    pub top: [u32; 17],
    pub wall: [u32; 17],
    pub top_shade: [u32; 17],
    pub wall_shade: [u32; 17],
    pub cloud_top: [u32; 8],
    pub cloud_wall: [u32; 8],
    pub cloud_under: u32,
    pub sky: [u32; 8],
    pub haze: u32,
    pub sun: [Fixed; 2],
    pub sun_color: u32,
    pub sun_glow: u32,
    pub jets: [[u32; 96]; 3],
    pub outline: u32,
    pub spark: [u32; 2],
    pub foam: u32,
    pub trails: [u32; 3],
}
fn grade(c: [u8; 3], day: Daylight, amount: Fixed) -> u32 {
    let [r, g, b] = c.map(|value| Fixed::from_int(i32::from(value)));
    let target = match day {
        Daylight::Day => [r, g, b],
        Daylight::Sunset => [
            Fixed::from_ratio(95, 100) * r + Fixed::from_ratio(1, 10) * g + Fixed::from_int(12),
            Fixed::from_ratio(72, 100) * g + Fixed::from_ratio(5, 100) * r + Fixed::from_int(4),
            Fixed::from_ratio(7, 10) * b + Fixed::from_ratio(8, 100) * r + Fixed::from_int(10),
        ],
        Daylight::Night => [
            Fixed::from_ratio(22, 100) * r + Fixed::from_int(2),
            Fixed::from_ratio(3, 10) * g + Fixed::from_int(6),
            Fixed::from_ratio(5, 10) * b + Fixed::from_int(22),
        ],
    };
    rgba(
        r + (target[0] - r) * amount,
        g + (target[1] - g) * amount,
        b + (target[2] - b) * amount,
    )
}
impl Palette {
    pub fn new(day: Daylight) -> Self {
        let top = world::TOP.map(|c| grade(c, day, Fixed::ONE));
        let wall = world::WALL.map(|c| grade(c, day, Fixed::ONE));
        let shade = |c: u32| {
            let c = c.to_le_bytes();
            rgba(
                Fixed::from_int((c[0]) as i32) * Fixed::from_ratio(62, 100),
                Fixed::from_int((c[1]) as i32) * Fixed::from_ratio(67, 100),
                Fixed::from_int((c[2]) as i32) * Fixed::from_ratio(8, 10),
            )
        };
        let cloud_amount = if day == Daylight::Night {
            Fixed::from_ratio(72, 100)
        } else {
            Fixed::ONE
        };
        let cloud_top = core::array::from_fn(|k| {
            let f = (Fixed::from_int((k) as i32) / Fixed::from_int(2)).min(Fixed::ONE);
            grade(
                [
                    (Fixed::from_int(218) + Fixed::from_int(37) * f).trunc_to_int() as u8,
                    (Fixed::from_int(230) + Fixed::from_int(25) * f).trunc_to_int() as u8,
                    (Fixed::from_int(247) + Fixed::from_int(8) * f).trunc_to_int() as u8,
                ],
                day,
                cloud_amount,
            )
        });
        let cloud_wall = core::array::from_fn(|k| {
            let f = Fixed::from_int((k) as i32) / Fixed::from_int(7);
            grade(
                [
                    (Fixed::from_int(158) + Fixed::from_int(44) * f).trunc_to_int() as u8,
                    (Fixed::from_int(182) + Fixed::from_int(36) * f).trunc_to_int() as u8,
                    (Fixed::from_int(222) + Fixed::from_int(18) * f).trunc_to_int() as u8,
                ],
                day,
                cloud_amount,
            )
        });
        let (sky_a, sky_m, sky_b, haze, sun) = match day {
            Daylight::Day => (
                [198, 232, 250],
                [122, 190, 240],
                [62, 142, 222],
                [188, 224, 246],
                [Fixed::from_ratio(5, 10), Fixed::from_ratio(32, 100)],
            ),
            Daylight::Sunset => (
                [255, 198, 132],
                [238, 122, 130],
                [84, 68, 150],
                [250, 182, 134],
                [Fixed::from_ratio(95, 100), Fixed::from_ratio(5, 10)],
            ),
            Daylight::Night => (
                [36, 56, 110],
                [18, 28, 70],
                [7, 10, 34],
                [28, 44, 92],
                [Fixed::from_ratio(3, 10), Fixed::from_ratio(42, 100)],
            ),
        };
        let pack = |c: [u8; 3]| {
            rgba(
                Fixed::from_int((c[0]) as i32),
                Fixed::from_int((c[1]) as i32),
                Fixed::from_int((c[2]) as i32),
            )
        };
        let sky = core::array::from_fn(|i| {
            let t = Fixed::from_int((i) as i32) / Fixed::from_int(7);
            if t < Fixed::from_ratio(5, 10) {
                blend(pack(sky_a), pack(sky_m), t * Fixed::from_int(2))
            } else {
                blend(
                    pack(sky_m),
                    pack(sky_b),
                    t * Fixed::from_int(2) - Fixed::ONE,
                )
            }
        });
        let mut jets = [[0; 96]; 3];
        for row in jet::PALETTE {
            jets[0][usize::from(row[0])] = grade(
                [row[1], row[2], row[3]],
                day,
                if day == Daylight::Sunset {
                    Fixed::from_ratio(5, 10)
                } else if day == Daylight::Night {
                    Fixed::from_ratio(16, 100)
                } else {
                    Fixed::ZERO
                },
            );
        }
        for row in jet::PALETTE {
            let i = usize::from(row[0]);
            jets[1][i] = if row[4] < 3 && jets[0][i + 1] != 0 {
                jets[0][i + 1]
            } else {
                jets[0][i]
            };
            jets[2][i] = if row[4] < 3 && (i - 1) & 15 > 0 {
                jets[0][i - 1]
            } else {
                jets[0][i]
            };
        }
        Self {
            day,
            top,
            wall,
            top_shade: top.map(shade),
            wall_shade: wall.map(shade),
            cloud_top,
            cloud_wall,
            cloud_under: grade([138, 160, 200], day, cloud_amount),
            sky,
            haze: pack(haze),
            sun,
            sun_color: pack(if day == Daylight::Night {
                [232, 238, 255]
            } else {
                [255, 244, 200]
            }),
            sun_glow: pack(if day == Daylight::Night {
                [70, 96, 160]
            } else {
                [255, 222, 150]
            }),
            jets,
            outline: if day == Daylight::Night {
                pack([44, 64, 118])
            } else {
                grade([13, 15, 26], day, Fixed::from_ratio(3, 10))
            },
            spark: match day {
                Daylight::Day => [pack([120, 190, 236]), pack([236, 250, 255])],
                Daylight::Sunset => [pack([255, 170, 110]), pack([255, 226, 170])],
                Daylight::Night => [pack([70, 110, 190]), pack([170, 200, 240])],
            },
            foam: pack(match day {
                Daylight::Day => [240, 252, 255],
                Daylight::Sunset => [255, 226, 214],
                Daylight::Night => [110, 140, 200],
            }),
            trails: [[255, 255, 255], [214, 232, 250], [170, 204, 240]]
                .map(|c| grade(c, day, Fixed::from_ratio(8, 10))),
        }
    }
}
pub struct Frame {
    pub texture: Texture<'static>,
    pub width: usize,
    pub height: usize,
    depth: Vec<Fixed>,
    mask: Vec<u8>,
    backdrop: Vec<u32>,
    columns: Vec<i32>,
}
#[doc = " A short mutable borrow. No texture or allocator borrow survives the call."]
pub struct Canvas<'a> {
    pub bytes: &'a mut [u8],
    pub depth: &'a mut [Fixed],
    pub mask: &'a mut [u8],
    pub backdrop: &'a mut [u32],
    pub columns: &'a mut [i32],
    pub width: usize,
    pub height: usize,
    pub bounds: [i32; 4],
}
impl Frame {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        let count = width * height;
        Ok(Self {
            texture: Texture::owned(width as u16, height as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0),
            width,
            height,
            depth: vec![Fixed::MAX; count],
            mask: vec![0; count],
            backdrop: vec![0; count],
            columns: vec![height as i32; width],
        })
    }
    pub fn with_canvas<R>(&mut self, f: impl FnOnce(&mut Canvas<'_>) -> R) -> R {
        let mut pixels = self.texture.pixels_mut().expect("owned flight frame");
        f(&mut Canvas {
            bytes: pixels.bytes_mut(),
            depth: &mut self.depth,
            mask: &mut self.mask,
            backdrop: &mut self.backdrop,
            columns: &mut self.columns,
            width: self.width,
            height: self.height,
            bounds: [0, 0, self.width as i32, self.height as i32],
        })
    }
    pub fn bytes(&self) -> usize {
        self.width * self.height * 4
            + self.depth.capacity() * 4
            + self.mask.capacity()
            + self.backdrop.capacity() * 4
            + self.columns.capacity() * 4
    }
}
impl Canvas<'_> {
    pub fn get(&self, index: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[index * 4..index * 4 + 4]
                .try_into()
                .expect("RGBA pixel"),
        )
    }
    pub fn put(&mut self, index: usize, color: u32) {
        self.bytes[index * 4..index * 4 + 4].copy_from_slice(&color.to_le_bytes());
    }
    pub fn point(&mut self, x: i32, y: i32, color: u32) {
        if x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32 {
            self.put(y as usize * self.width + x as usize, color);
        }
    }
    fn fill_depth(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed) {
        for y in y0.max(self.bounds[1])..y1.min(self.bounds[3]) {
            let at = y as usize * self.width + x;
            if self.get(at) == 0 {
                self.put(at, color);
                self.depth[at] = depth;
            }
        }
    }
    fn cloud_span(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed, thin: bool) {
        for y in y0.max(self.bounds[1])..y1.min(self.bounds[3]) {
            if thin && (x + y as usize) & 1 == 0 {
                continue;
            }
            let at = y as usize * self.width + x;
            if self.get(at) == 0 {
                self.put(at, color);
                self.depth[at] = depth;
            }
        }
    }
    fn clear(&mut self) {
        for y in self.bounds[1]..self.bounds[3] {
            let start = y as usize * self.width + self.bounds[0] as usize;
            let end = y as usize * self.width + self.bounds[2] as usize;
            self.bytes[start * 4..end * 4].fill(0);
            self.depth[start..end].fill(Fixed::MAX);
        }
        self.columns.fill(self.bounds[3]);
    }
}
#[derive(Clone, Copy, Default)]
pub struct Flame {
    pub points: [[Fixed; 2]; 3],
    pub radius: Fixed,
    pub tip_scale: Fixed,
    pub power: Fixed,
    pub alpha: Fixed,
    pub visibility: [Fixed; 3],
    pub seed: Fixed,
    pub length: Fixed,
}
pub struct Raster {
    pub frame: Frame,
    pub sheet: Frame,
    pub palette: Palette,
    pub flames: [Flame; 6],
    pub flame_count: usize,
    slices: [&'static [jet::Voxel]; jet::LAYERS],
    shadow: [u8; jet::WIDTH * jet::LENGTH],
    seen: [[Fixed; 3]; 6],
}
#[doc = " Integral scaling preserves hard scene pixels and caps CPU work independently of window size."]
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    super::Profile::Desktop.scene_size(width, height)
}
impl Raster {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        let width = width.clamp(8, MAX_WIDTH);
        let height = height.clamp(8, MAX_HEIGHT);
        let pivot = (Fixed::from_int((height) as i32) * Fixed::from_ratio(34, 100)).round();
        let tall = pivot.max(Fixed::from_int((height) as i32) - pivot);
        let side = ((((Fixed::from_int((width) as i32) / Fixed::from_int(2)).powi(2) + tall * tall)
            .sqrt()
            .ceil())
        .trunc_to_int() as usize
            + 2)
            * 2;
        let slices = jet::slices();
        let mut shadow = [0; jet::WIDTH * jet::LENGTH];
        for slice in &slices {
            for voxel in *slice {
                shadow[usize::from(voxel.y) * jet::WIDTH + usize::from(voxel.x)] = 1;
            }
        }
        Ok(Self {
            frame: Frame::new(width, height)?,
            sheet: Frame::new(side, side)?,
            palette: Palette::new(Daylight::Day),
            flames: [Flame::default(); 6],
            flame_count: 0,
            slices,
            shadow,
            seen: [[Fixed::ONE; 3]; 6],
        })
    }
    pub fn bytes(&self) -> usize {
        self.frame.bytes() + self.sheet.bytes() + core::mem::size_of::<Self>()
    }
    pub fn render(&mut self, world: &World, flight: &Flight) {
        let p = flight.projection(self.frame.width, self.frame.height);
        self.flame_count = 0;
        let rolled = p.cockpit > Fixed::ZERO || p.roll != Fixed::ZERO;
        let Self {
            frame,
            sheet,
            palette,
            flames,
            flame_count,
            slices,
            shadow,
            seen,
        } = self;
        let frame_height = frame.height;
        let mut draw = |canvas: &mut Canvas<'_>, projection: Projection| {
            draw_world(canvas, world, flight, &projection, palette, shadow);
            draw_sky(canvas, &projection, palette, frame_height);
            draw_trails(canvas, flight, &projection, palette);
            draw_smoke(canvas, flight, &projection, palette.day);
            let mut order = [0, 1, 2];
            let (sn, cs) = projection.yaw.sin_cos();
            order[..flight.count].sort_unstable_by(|a, b| {
                let depth = |i: usize| {
                    (flight.jets[i].position[0] - projection.position[0]) * sn
                        + (flight.jets[i].position[1] - projection.position[1]) * cs
                };
                depth(*a).cmp(&depth(*b))
            });
            for i in &order[..flight.count] {
                draw_jet(canvas, flight, *i, &projection, palette, slices);
                record_flames(canvas, flight, *i, &projection, flames, flame_count, seen);
            }
        };
        if rolled {
            let mut projection = p;
            let half = Fixed::from_int((sheet.width / 2) as i32);
            let pivot =
                (Fixed::from_int((frame.height) as i32) * Fixed::from_ratio(34, 100)).round();
            projection.center = [half, half + (p.center[1] - pivot)];
            sheet.with_canvas(|canvas| {
                let c = p.roll.cos().abs();
                let s = p.roll.sin().abs();
                let tall = pivot.max(Fixed::from_int((frame.height) as i32) - pivot);
                let x = (c * Fixed::from_int((frame.width) as i32) / Fixed::from_int(2) + s * tall)
                    .ceil()
                    + Fixed::from_int(2);
                let top = (c * pivot
                    + s * Fixed::from_int((frame.width) as i32) / Fixed::from_int(2))
                .ceil()
                    + Fixed::from_int(2);
                let bottom = (c * (Fixed::from_int((frame.height) as i32) - pivot)
                    + s * Fixed::from_int((frame.width) as i32) / Fixed::from_int(2))
                .ceil()
                    + Fixed::from_int(2);
                canvas.bounds = [
                    ((half - x).max(Fixed::ZERO)).trunc_to_int(),
                    ((half - top).max(Fixed::ZERO)).trunc_to_int(),
                    ((half + x).min(Fixed::from_int((canvas.width) as i32))).trunc_to_int(),
                    ((half + bottom).min(Fixed::from_int((canvas.height) as i32))).trunc_to_int(),
                ];
                draw(canvas, projection);
            });
            let pixels = sheet.texture.read_pixels().expect("flight roll sheet");
            frame.with_canvas(|canvas| {
                let (s, c) = p.roll.sin_cos();
                let cx = Fixed::from_int((canvas.width) as i32) / Fixed::from_int(2);
                for y in 0..canvas.height {
                    let dy = Fixed::from_int((y) as i32) - pivot;
                    for x in 0..canvas.width {
                        let dx = Fixed::from_int((x) as i32) - cx;
                        let sx = (half + c * dx - s * dy + Fixed::from_ratio(5, 10)).trunc_to_int()
                            as usize;
                        let sy = (half + s * dx + c * dy + Fixed::from_ratio(5, 10)).trunc_to_int()
                            as usize;
                        let at = (sy * sheet.width + sx) * 4;
                        canvas.bytes[(y * canvas.width + x) * 4..(y * canvas.width + x + 1) * 4]
                            .copy_from_slice(&pixels.bytes()[at..at + 4]);
                    }
                }
            });
            for flame in &mut flames[..*flame_count] {
                for point in &mut flame.points {
                    let (s, c) = p.roll.sin_cos();
                    let dx = point[0] - half;
                    let dy = point[1] - half;
                    *point = [
                        Fixed::from_int((frame.width) as i32) / Fixed::from_int(2)
                            + c * dx
                            + s * dy,
                        pivot - s * dx + c * dy,
                    ];
                }
            }
        } else {
            frame.with_canvas(|canvas| draw(canvas, p));
        }
    }
}
fn draw_world(
    canvas: &mut Canvas<'_>,
    world: &World,
    flight: &Flight,
    p: &Projection,
    pal: &Palette,
    shadow: &[u8],
) {
    canvas.clear();
    let (sn, cs) = p.yaw.sin_cos();
    let q = p.perspective;
    let scale = p.scale;
    let slope = p.slope;
    let [cx, cy] = p.center;
    let alt = p.position[2];
    let bx = p.position[0].rem_euclid(Fixed::from_int(world.period() as i32))
        + Fixed::from_int(world.period() as i32) * Fixed::from_int(2);
    let by = p.position[1].rem_euclid(Fixed::from_int(world.period() as i32))
        + Fixed::from_int(world.period() as i32) * Fixed::from_int(2);
    let rank = ((flight.settings.clouds.clamp(Fixed::ZERO, Fixed::ONE) * Fixed::from_int(16))
        .round())
    .trunc_to_int() as u8;
    let drift = [
        (flight.time * Fixed::from_int(7)).rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
        (flight.time * Fixed::from_int(3)).rem_euclid(Fixed::from_int(world.cloud_period() as i32)),
    ];
    let below = Fixed::from_int(canvas.bounds[3]) - cy;
    let nearest = if q > Fixed64::ZERO {
        -p.divide_perspective(Fixed::from_ratio(85, 100))
    } else {
        -Fixed::from_int(1_000_000)
    };
    let eye_height = if q > Fixed64::ZERO {
        alt + p.divide_perspective(slope)
    } else {
        Fixed::from_int(1_000_000)
    };
    let dn = scale * slope + p.apply_perspective(below);
    let mut v = if eye_height > Fixed::from_int((world::MAX_HEIGHT) as i32) + Fixed::ONE
        && dn > Fixed::from_ratio(1, 1000000)
    {
        nearest.max(
            (scale * (alt - Fixed::from_int((world::MAX_HEIGHT) as i32)) - below) / dn
                - Fixed::from_int(2),
        )
    } else {
        nearest
    };
    let limit = Fixed::from_int(1500) + Fixed::from_int(3) * alt;
    let fog_start = Fixed::from_int(300) + Fixed::from_int(2) * alt;
    let above = cy - Fixed::from_int(canvas.bounds[1]) + Fixed::from_int(2);
    let end = if scale * slope > p.apply_perspective(above) {
        limit.min((scale * alt + above) / (scale * slope - p.apply_perspective(above)))
    } else {
        limit
    };
    let mut den = Fixed::ONE + p.apply_perspective(v);
    let mut m = scale / den;
    let mut row = [0i32; world::MAX_HEIGHT + 1];
    for (z, r) in row.iter_mut().enumerate() {
        *r = ((cy - m * (slope * v - alt) - m * Fixed::from_int((z) as i32)
            + Fixed::from_ratio(1, 100000))
        .floor())
        .trunc_to_int();
    }
    let rows = scale * (slope + p.apply_perspective(alt));
    for _ in 0..4000 {
        if v >= end {
            break;
        }
        let step = (den * den / rows)
            .min(Fixed::from_ratio(25, 10) * den / scale)
            .max(Fixed::from_ratio(5, 10));
        v += step;
        den = Fixed::ONE + p.apply_perspective(v);
        m = scale / den;
        let previous = row;
        for (z, r) in row.iter_mut().enumerate() {
            *r = ((cy - m * (slope * v - alt) - m * Fixed::from_int((z) as i32)
                + Fixed::from_ratio(1, 100000))
            .floor())
            .trunc_to_int();
        }
        let inverse = den / scale;
        let ax = cs * inverse;
        let ay = -sn * inverse;
        let mut wx = bx - cx * ax - v * sn + ax * Fixed::from_int(canvas.bounds[0]);
        let mut wy = by - cx * ay - v * cs + ay * Fixed::from_int(canvas.bounds[0]);
        let fog = if q > Fixed64::ZERO && v > fog_start {
            (((v - fog_start) / (limit - fog_start)).sqrt() * Fixed::from_int(10))
                .round()
                .min(Fixed::from_int(10))
                / Fixed::from_int(10)
        } else {
            Fixed::ZERO
        };
        for px in canvas.bounds[0]..canvas.bounds[2] {
            let x = px as usize;
            let gx = (wx).trunc_to_int();
            let gy = (wy).trunc_to_int();
            wx += ax;
            wy += ay;
            if rank > 0 {
                let cloud = world.cloud(
                    (Fixed::from_int(gx) - drift[0]).trunc_to_int(),
                    (Fixed::from_int(gy) - drift[1]).trunc_to_int(),
                );
                let thick = usize::from(cloud & 15);
                if thick > 0 && cloud >> 4 < rank {
                    let top = (world::CLOUD_ALT).trunc_to_int() as usize + thick - 1;
                    canvas.cloud_span(
                        x,
                        row[top],
                        previous[top],
                        blend(pal.cloud_top[thick - 1], pal.haze, fog),
                        v,
                        thick == 1,
                    );
                    canvas.cloud_span(
                        x,
                        previous[(world::CLOUD_ALT).trunc_to_int() as usize],
                        row[(world::CLOUD_ALT).trunc_to_int() as usize],
                        blend(pal.cloud_under, pal.haze, fog),
                        v,
                        thick == 1,
                    );
                    for layer in (0..thick - 1).rev() {
                        let z = (world::CLOUD_ALT).trunc_to_int() as usize + layer;
                        canvas.fill_depth(
                            x,
                            previous[z + 1],
                            previous[z],
                            blend(pal.cloud_wall[layer], pal.haze, fog),
                            v,
                        );
                    }
                }
            }
            let h = usize::from(world.height(gx, gy));
            let mut column = canvas.columns[x];
            let yt = row[h];
            let yr = previous[h];
            if yt >= column && yr >= column {
                continue;
            }
            let material = usize::from(world.material(gx, gy));
            let mut shaded = false;
            if rank > 0 {
                let c = world.cloud(
                    (Fixed::from_int(gx) + pal.sun[0] * world::CLOUD_ALT - drift[0]).trunc_to_int(),
                    (Fixed::from_int(gy) + pal.sun[1] * world::CLOUD_ALT - drift[1]).trunc_to_int(),
                );
                shaded = c & 15 > 1 && c >> 4 < rank;
            }
            if !shaded {
                for jet in &flight.jets[..flight.count] {
                    let dx = Fixed::from_int(gx)
                        - (bx + jet.position[0] - p.position[0] + pal.sun[0] * jet.position[2])
                        + pal.sun[0] * Fixed::from_int((h) as i32);
                    let dy = Fixed::from_int(gy)
                        - (by + jet.position[1] - p.position[1] + pal.sun[1] * jet.position[2])
                        + pal.sun[1] * Fixed::from_int((h) as i32);
                    let reach = Fixed::from_int((jet::LENGTH) as i32)
                        * Fixed::from_ratio(75, 100)
                        * flight.settings.jet_scale
                        + Fixed::from_int(3);
                    if dx.abs() < reach && dy.abs() < reach {
                        let (s, c) = jet.yaw.sin_cos();
                        let a = (dx * c + dy * s)
                            / (jet.roll.cos().abs().max(Fixed::from_ratio(3, 10))
                                * flight.settings.jet_scale);
                        let b = (dx * s - dy * c) / flight.settings.jet_scale;
                        let i = ((a + Fixed::from_int((jet::WIDTH) as i32) / Fixed::from_int(2))
                            .floor())
                        .trunc_to_int();
                        let k =
                            ((jet::PIVOT[0] - b + Fixed::from_ratio(5, 10)).floor()).trunc_to_int();
                        if (0..jet::WIDTH as i32).contains(&i)
                            && (0..jet::LENGTH as i32).contains(&k)
                            && shadow[k as usize * jet::WIDTH + i as usize] != 0
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
                if material == 3
                    && noise(gx, gy)
                        .wrapping_add((flight.time * Fixed::from_int(36)).trunc_to_int() as u8)
                        < 120
                {
                    top = if shaded { pal.top_shade[2] } else { pal.foam };
                } else if !shaded && inverse < Fixed::from_ratio(18, 10) {
                    let n = noise(
                        (gx + ((flight.time * Fixed::from_int(6)).trunc_to_int() & 1023)) >> 2,
                        gy,
                    );
                    if n > 243
                        && noise(
                            (gx + 1024
                                - ((flight.time * Fixed::from_int(4)).trunc_to_int() & 1023))
                                >> 2,
                            gy + 91,
                        ) > 128
                    {
                        top = pal.spark[usize::from(n > 252)];
                    }
                }
                wall = top;
            }
            if yr < column {
                canvas.fill_depth(x, yr, column, blend(wall, pal.haze, fog), v);
                column = yr;
            }
            if yt < column {
                canvas.fill_depth(x, yt, column, blend(top, pal.haze, fog), v);
                column = yt;
            }
            canvas.columns[x] = column;
        }
    }
}
fn draw_sky(canvas: &mut Canvas<'_>, p: &Projection, pal: &Palette, frame_height: usize) {
    let horizon = if p.perspective > Fixed64::ZERO {
        p.center[1] - p.divide_perspective(p.scale * p.slope)
    } else {
        -Fixed::from_int(1_000_000)
    };
    let focal = if p.perspective > Fixed64::ZERO {
        p.divide_perspective(p.scale)
    } else {
        Fixed::from_int(1_000_000)
    };
    for y in canvas.bounds[1]..canvas.bounds[3] {
        for x in canvas.bounds[0]..canvas.bounds[2] {
            let at = y as usize * canvas.width + x as usize;
            if canvas.get(at) != 0 {
                continue;
            }
            let mut color = pal.haze;
            if (Fixed::from_int(y)) < horizon {
                let t = (horizon - Fixed::from_int(y))
                    / (Fixed::from_int((frame_height) as i32) * Fixed::from_ratio(75, 100))
                    * Fixed::from_int(7);
                let mut band = (t).trunc_to_int() as usize;
                if (t - Fixed::from_int((band) as i32) - Fixed::from_ratio(7, 10))
                    * Fixed::from_ratio(34, 10)
                    * Fixed::from_int(16)
                    > Fixed::from_int((BAYER[((y & 3) * 4 + (x & 3)) as usize]) as i32)
                {
                    band += 1;
                }
                color = pal.sky[band.min(7)];
                if pal.day == Daylight::Night
                    && band > 1
                    && world::hash8(
                        (((x + (p.yaw * focal).trunc_to_int()) & 4095) * 73 + y * 9973) as u32,
                    ) < 2
                {
                    color = pal.sun_color;
                }
                if pal.day != Daylight::Day {
                    let length = (pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1]).sqrt();
                    let dx = -pal.sun[0] / length;
                    let dy = -pal.sun[1] / length;
                    let (sn, cs) = p.yaw.sin_cos();
                    let along = -(dx * sn + dy * cs);
                    if along > Fixed::from_ratio(2, 10) {
                        let radius = Fixed::from_int((canvas.width) as i32)
                            * if pal.day == Daylight::Sunset {
                                Fixed::from_ratio(5, 100)
                            } else {
                                Fixed::from_ratio(22, 1000)
                            };
                        let sx = p.center[0] + focal * (dx * cs - dy * sn) / along;
                        let sy = horizon
                            - focal
                                * if pal.day == Daylight::Sunset {
                                    Fixed::from_ratio(12, 100)
                                } else {
                                    Fixed::from_ratio(3, 10)
                                }
                                / along;
                        let distance = ((Fixed::from_int(x) - sx).powi(2)
                            + (Fixed::from_int(y) - sy).powi(2))
                        .sqrt()
                            / radius;
                        if distance < Fixed::ONE {
                            color = pal.sun_color;
                        } else if distance < Fixed::from_ratio(19, 10)
                            && (Fixed::from_ratio(19, 10) - distance) * Fixed::from_int(14)
                                > Fixed::from_int((BAYER[((y & 3) * 4 + (x & 3)) as usize]) as i32)
                        {
                            color = pal.sun_glow;
                        }
                    }
                }
            } else if p.perspective.is_zero() {
                color = pal.top[0];
            }
            canvas.put(at, color);
        }
    }
}
fn draw_trails(canvas: &mut Canvas<'_>, flight: &Flight, p: &Projection, pal: &Palette) {
    for (i, jet) in flight.jets[..flight.count].iter().enumerate() {
        if i == 0 && p.jet_alpha < Fixed::from_ratio(5, 10) {
            continue;
        }
        for wing in &jet.trails {
            let mut last: Option<[Fixed; 4]> = None;
            for n in 0..jet.trail_len {
                let point = wing[(jet.trail_head + TRAIL_CAPACITY - 1 - n) % TRAIL_CAPACITY];
                let age = (flight.time - point.born)
                    / (Fixed::from_ratio(26, 10)
                        * (Fixed::from_ratio(3, 10) + Fixed::from_ratio(7, 10) * jet.load));
                if age >= Fixed::ONE {
                    break;
                }
                let Some(next) = p.project(point.position) else {
                    last = None;
                    continue;
                };
                if let Some(prev) = last {
                    let span = (next[0] - prev[0]).abs().max((next[1] - prev[1]).abs());
                    if span < Fixed::from_int(600) {
                        let steps = (span.ceil().max(Fixed::ONE)).trunc_to_int() as usize;
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
                            let x = ((prev[0]
                                + (next[0] - prev[0]) * Fixed::from_int((k) as i32)
                                    / Fixed::from_int((steps) as i32))
                            .round())
                            .trunc_to_int();
                            let y = ((prev[1]
                                + (next[1] - prev[1]) * Fixed::from_int((k) as i32)
                                    / Fixed::from_int((steps) as i32))
                            .round())
                            .trunc_to_int();
                            for yy in y..y + if next[2] > Fixed::from_ratio(24, 10) {
                                2
                            } else {
                                1
                            } {
                                for xx in x..x + if next[2] > Fixed::from_ratio(24, 10) {
                                    2
                                } else {
                                    1
                                } {
                                    if xx < 0
                                        || yy < 0
                                        || xx >= canvas.width as i32
                                        || yy >= canvas.height as i32
                                    {
                                        continue;
                                    }
                                    let at = yy as usize * canvas.width + xx as usize;
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
                last = Some(next);
            }
        }
    }
}
fn draw_smoke(canvas: &mut Canvas<'_>, flight: &Flight, p: &Projection, day: Daylight) {
    for smoke in &flight.smoke {
        let age = flight.time - smoke.born;
        if smoke.life <= Fixed::ZERO || age < Fixed::ZERO || age >= smoke.life {
            continue;
        }
        let t = age / smoke.life;
        let push = (Fixed::ONE - (-age * Fixed::from_int(5)).exp()) / Fixed::from_int(5);
        let Some(point) = p.project([
            smoke.position[0] + smoke.push[0] * push,
            smoke.position[1] + smoke.push[1] * push,
            smoke.position[2] + age * Fixed::from_ratio(25, 10),
        ]) else {
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
        for y in ((point[1] - radius).floor()).trunc_to_int().max(0)
            ..=((point[1] + radius).ceil())
                .trunc_to_int()
                .min(canvas.height as i32 - 1)
        {
            for x in ((point[0] - radius).floor()).trunc_to_int().max(0)
                ..=((point[0] + radius).ceil())
                    .trunc_to_int()
                    .min(canvas.width as i32 - 1)
            {
                let d = ((Fixed::from_int(x) - point[0]).powi(2)
                    + (Fixed::from_int(y) - point[1]).powi(2))
                    / (radius * radius);
                let at = y as usize * canvas.width + x as usize;
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
fn draw_jet(
    canvas: &mut Canvas<'_>,
    flight: &Flight,
    index: usize,
    p: &Projection,
    pal: &Palette,
    slices: &[&[jet::Voxel]; jet::LAYERS],
) {
    let jet = &flight.jets[index];
    let fade = if index == 0 { p.jet_alpha } else { Fixed::ONE };
    if fade < Fixed::from_ratio(4, 1000) {
        return;
    }
    let Some(center) = p.project(jet.position) else {
        return;
    };
    if center[2] <= Fixed::ZERO {
        return;
    }
    let reach = Fixed::from_int((jet::LENGTH) as i32) * flight.settings.jet_scale * center[2]
        + Fixed::from_int(40);
    if center[0] < -reach
        || center[1] < -reach - Fixed::from_int((canvas.height) as i32)
        || center[0] > Fixed::from_int((canvas.width) as i32) + reach
        || center[1] > Fixed::from_int((canvas.height) as i32) + reach
    {
        return;
    }
    canvas.mask.fill(0);
    let axes = jet.axes();
    let (sn, cs) = p.yaw.sin_cos();
    let sc = flight.settings.jet_scale;
    let project_axis = |a: [Fixed; 3]| {
        [
            sc * (a[0] * cs - a[1] * sn),
            -sc * (a[0] * sn + a[1] * cs),
            sc * a[2],
        ]
    };
    let [right, forward, up] = axes.map(project_axis);
    let dx = jet.position[0] - p.position[0];
    let dy = jet.position[1] - p.position[1];
    let cu = dx * cs - dy * sn;
    let cv = -(dx * sn + dy * cs);
    let cw = jet.position[2] - p.position[2];
    let plane = if p.perspective.is_zero() {
        if p.slope * up[2] - up[1] >= Fixed::ZERO {
            Fixed::from_int(1_000_000)
        } else {
            -Fixed::from_int(1_000_000)
        }
    } else {
        (-cu * up[0]
            + (-p.divide_perspective(Fixed::ONE) - cv) * up[1]
            + (p.divide_perspective(p.slope) - cw) * up[2])
            / (sc * sc)
    };
    let mut order: [usize; jet::LAYERS] = core::array::from_fn(|i| i);
    order.sort_unstable_by(|a, b| {
        (((Fixed::from_int((*b) as i32) - jet::PIVOT[1]) * flight.spread - plane).abs())
            .cmp(&((Fixed::from_int((*a) as i32) - jet::PIVOT[1]) * flight.spread - plane).abs())
    });
    let light = |side: Fixed| {
        let n = core::array::from_fn::<_, 3, _>(|i| {
            axes[2][i] + side * Fixed::from_ratio(4, 10) * axes[0][i]
        });
        let dot = -n[0] * pal.sun[0] - n[1] * pal.sun[1] + n[2] * Fixed::from_ratio(11, 10);
        let norm =
            ((pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1] + Fixed::from_ratio(121, 100))
                * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
                .sqrt();
        let v = dot / norm;
        if v > if pal.day == Daylight::Night {
            Fixed::from_ratio(74, 100)
        } else {
            Fixed::from_ratio(9, 10)
        } {
            1
        } else if v < if pal.day == Daylight::Night {
            Fixed::from_ratio(4, 10)
        } else {
            Fixed::from_ratio(66, 100)
        } {
            2
        } else {
            0
        }
    };
    let lights = [light(-Fixed::ONE), light(Fixed::ONE)];
    let big = sc * p.scale
        / (Fixed::ONE + p.apply_perspective(cv)
            - p.apply_perspective(
                Fixed::from_int(jet::LENGTH as i32) * Fixed::from_ratio(6, 10) * sc,
            ))
        .max(Fixed::from_ratio(2, 10));
    let samples = ((big * Fixed::from_int(2) - Fixed::from_ratio(24, 100))
        .ceil()
        .clamp(Fixed::from_int(2), Fixed::from_int(8)))
    .trunc_to_int() as usize;
    let mut offsets = [[Fixed::ZERO; 2]; 64];
    for i in 0..samples {
        for k in 0..samples {
            let a = (Fixed::from_int((i) as i32) + Fixed::from_ratio(5, 10))
                / Fixed::from_int((samples) as i32)
                - Fixed::from_ratio(5, 10);
            let b = (Fixed::from_int((k) as i32) + Fixed::from_ratio(5, 10))
                / Fixed::from_int((samples) as i32)
                - Fixed::from_ratio(5, 10);
            offsets[i * samples + k] = [
                center[2] * (a * right[0] + b * forward[0]),
                -center[2]
                    * (a * (right[2] + p.slope * right[1])
                        + b * (forward[2] + p.slope * forward[1])),
            ];
        }
    }
    let mut bounds = [canvas.width as i32, canvas.height as i32, -1, -1];
    let first_slice = slices
        .iter()
        .position(|slice| !slice.is_empty())
        .unwrap_or(jet::LAYERS);
    for layer in order {
        let intro = flight.intro - Fixed::from_int((index) as i32) * Fixed::from_ratio(25, 100);
        let t = (intro
            - Fixed::from_int((layer.saturating_sub(first_slice)) as i32)
                * Fixed::from_ratio(7, 100))
            / Fixed::from_ratio(36, 100);
        if t <= Fixed::ZERO {
            continue;
        }
        let drop = if t < Fixed::ONE {
            -(Fixed::ONE - t * t)
                * (Fixed::from_int((canvas.height) as i32) / Fixed::from_int(2)
                    + Fixed::from_int(44))
        } else {
            Fixed::ZERO
        };
        let height = (Fixed::from_int((layer) as i32) - jet::PIVOT[1]) * flight.spread;
        let bu = cu + height * up[0];
        let bv = cv + height * up[1];
        let bw = cw + height * up[2];
        for voxel in slices[layer] {
            let a = voxel.a();
            let b = voxel.b();
            let depth = bv + a * right[1] + b * forward[1];
            let den = Fixed::ONE + p.apply_perspective(depth);
            if den < Fixed::from_ratio(2, 10) {
                continue;
            }
            let m = p.scale / den;
            let color = pal.jets[lights[usize::from(a >= Fixed::ZERO)]][usize::from(voxel.color)];
            let x = p.center[0] + m * (bu + a * right[0] + b * forward[0]);
            let y = p.center[1] - m * (bw + a * right[2] + b * forward[2] + p.slope * depth) + drop;
            for offset in &offsets[..samples * samples] {
                let xx = ((x + offset[0] * m / center[2]).round()).trunc_to_int();
                let yy = ((y + offset[1] * m / center[2]).round()).trunc_to_int();
                if xx < 0 || yy < 0 || xx >= canvas.width as i32 || yy >= canvas.height as i32 {
                    continue;
                }
                let at = yy as usize * canvas.width + xx as usize;
                if canvas.depth[at] < depth - Fixed::ONE {
                    continue;
                }
                if fade < Fixed::ONE && canvas.mask[at] == 0 {
                    canvas.backdrop[at] = canvas.get(at);
                }
                canvas.put(at, color);
                canvas.mask[at] = 1;
                bounds = [
                    bounds[0].min(xx),
                    bounds[1].min(yy),
                    bounds[2].max(xx),
                    bounds[3].max(yy),
                ];
            }
        }
    }
    for y in (bounds[1] - 1).max(1)..=(bounds[3] + 1).min(canvas.height as i32 - 2) {
        for x in (bounds[0] - 1).max(1)..=(bounds[2] + 1).min(canvas.width as i32 - 2) {
            let at = y as usize * canvas.width + x as usize;
            if canvas.mask[at] != 0 {
                if fade < Fixed::ONE {
                    canvas.put(at, blend(canvas.backdrop[at], canvas.get(at), fade));
                }
            } else if fade >= Fixed::from_ratio(5, 10)
                && (canvas.mask[at - 1] != 0
                    || canvas.mask[at + 1] != 0
                    || canvas.mask[at - canvas.width] != 0
                    || canvas.mask[at + canvas.width] != 0)
            {
                canvas.put(at, pal.outline);
            }
        }
    }
}
fn record_flames(
    canvas: &Canvas<'_>,
    flight: &Flight,
    index: usize,
    p: &Projection,
    out: &mut [Flame; 6],
    count: &mut usize,
    seen: &mut [[Fixed; 3]; 6],
) {
    let jet: &Aircraft = &flight.jets[index];
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
            if let Some(value) = p.project(jet.local(local, flight.settings.jet_scale)) {
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
        let mut visibility = [Fixed::ZERO; 3];
        for n in 0..3 {
            let point = if n == 1 {
                core::array::from_fn(|k| {
                    Fixed::from_ratio(25, 100) * projected[0][k]
                        + Fixed::from_ratio(5, 10) * projected[1][k]
                        + Fixed::from_ratio(25, 100) * projected[2][k]
                })
            } else {
                projected[n]
            };
            let mut open = 0;
            for (dx, dy) in [
                (Fixed::ZERO, Fixed::ZERO),
                (-radius, Fixed::ZERO),
                (radius, Fixed::ZERO),
                (Fixed::ZERO, -radius),
                (Fixed::ZERO, radius),
            ] {
                let x = ((point[0] + dx).round()).trunc_to_int() as i32;
                let y = ((point[1] + dy).round()).trunc_to_int() as i32;
                if x < 0
                    || y < 0
                    || x >= canvas.width as i32
                    || y >= canvas.height as i32
                    || canvas.depth[y as usize * canvas.width + x as usize] >= point[3] - Fixed::ONE
                {
                    open += 1;
                }
            }
            seen[slot][n] += (Fixed::from_int(open) / Fixed::from_int(5) - seen[slot][n])
                * Fixed::from_ratio(3, 10);
            visibility[n] = seen[slot][n];
        }
        out[*count] = Flame {
            points: projected.map(|v| [v[0], v[1]]),
            radius,
            tip_scale: projected[2][2] / projected[0][2],
            power: flight.boost,
            alpha,
            visibility,
            seed: Fixed::from_int((slot) as i32),
            length: length / width,
        };
        *count += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::super::flight::{Settings, View};
    use super::*;
    #[test]
    fn all_camera_frames_are_opaque_and_storage_is_bounded() {
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let mut flight = Flight::new(&world, Settings::default());
        let mut raster = Raster::new(240, 150).unwrap();
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            for _ in 0..240 {
                flight.step(&world, 16, 240, 150);
            }
            raster.render(&world, &flight);
            let pixels = raster.frame.texture.read_pixels().unwrap();
            assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
            let first = &pixels.bytes()[..4];
            assert!(pixels.bytes().chunks_exact(4).any(|c| c != first));
        }
        let allocation =
            crate::test_support::tracked_allocations(|| raster.render(&world, &flight));
        assert_eq!(allocation, 0);
        let max = Raster::new(MAX_WIDTH, MAX_HEIGHT).unwrap();
        assert!(max.sheet.width <= 628);
        assert!(max.bytes() < 8 * 1024 * 1024);
        for size in [(0, 0), (1280, 720), (65535, 65535), (1, 65535), (65535, 1)] {
            let (w, h, _) = scene_size(size.0, size.1);
            assert!(w <= MAX_WIDTH && h <= MAX_HEIGHT);
        }
    }
    #[test]
    fn foreground_depth_blocks_background_writes() {
        let mut frame = Frame::new(8, 8).unwrap();
        frame.with_canvas(|canvas| {
            canvas.clear();
            canvas.fill_depth(
                3,
                2,
                5,
                rgba(
                    Fixed::from_int(10),
                    Fixed::from_int(20),
                    Fixed::from_int(30),
                ),
                Fixed::ONE,
            );
            canvas.fill_depth(
                3,
                0,
                8,
                rgba(
                    Fixed::from_int(200),
                    Fixed::from_int(20),
                    Fixed::from_int(30),
                ),
                Fixed::from_int(9),
            );
            assert_eq!(
                canvas.get(3 * 8 + 3),
                rgba(
                    Fixed::from_ratio(10, 1),
                    Fixed::from_ratio(20, 1),
                    Fixed::from_ratio(30, 1)
                )
            );
            assert_eq!(canvas.depth[3 * 8 + 3], Fixed::from_ratio(1, 1));
        });
    }
}
