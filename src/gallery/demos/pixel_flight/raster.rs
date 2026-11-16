//! Height-field scan conversion into reusable ordinary Texture storage.

use super::{
    flight::{Aircraft, Flight, Projection, TRAIL_CAPACITY},
    jet,
    world::{self, World},
};
use crate::render::{
    renderer::RenderError,
    texture::{ColorFormat, Texture},
};
use alloc::{vec, vec::Vec};

pub const MAX_WIDTH: usize = 480;
pub const MAX_HEIGHT: usize = 300;
pub const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
pub fn rgba(r: f32, g: f32, b: f32) -> u32 {
    u32::from_le_bytes([
        r.clamp(0.0, 255.0) as u8,
        g.clamp(0.0, 255.0) as u8,
        b.clamp(0.0, 255.0) as u8,
        255,
    ])
}
pub fn blend(bg: u32, fg: u32, amount: f32) -> u32 {
    let a = bg.to_le_bytes();
    let b = fg.to_le_bytes();
    let t = amount.clamp(0.0, 1.0);
    rgba(
        f32::from(a[0]) + (f32::from(b[0]) - f32::from(a[0])) * t,
        f32::from(a[1]) + (f32::from(b[1]) - f32::from(a[1])) * t,
        f32::from(a[2]) + (f32::from(b[2]) - f32::from(a[2])) * t,
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
    pub sun: [f32; 2],
    pub sun_color: u32,
    pub sun_glow: u32,
    pub jets: [[u32; 96]; 3],
    pub outline: u32,
    pub spark: [u32; 2],
    pub foam: u32,
    pub trails: [u32; 3],
}
fn grade(c: [u8; 3], day: Daylight, amount: f32) -> u32 {
    let [r, g, b] = c.map(f32::from);
    let target = match day {
        Daylight::Day => [r, g, b],
        Daylight::Sunset => [
            0.95 * r + 0.1 * g + 12.0,
            0.72 * g + 0.05 * r + 4.0,
            0.7 * b + 0.08 * r + 10.0,
        ],
        Daylight::Night => [0.22 * r + 2.0, 0.3 * g + 6.0, 0.5 * b + 22.0],
    };
    rgba(
        r + (target[0] - r) * amount,
        g + (target[1] - g) * amount,
        b + (target[2] - b) * amount,
    )
}
impl Palette {
    pub fn new(day: Daylight) -> Self {
        let top = world::TOP.map(|c| grade(c, day, 1.0));
        let wall = world::WALL.map(|c| grade(c, day, 1.0));
        let shade = |c: u32| {
            let c = c.to_le_bytes();
            rgba(
                f32::from(c[0]) * 0.62,
                f32::from(c[1]) * 0.67,
                f32::from(c[2]) * 0.8,
            )
        };
        let cloud_amount = if day == Daylight::Night { 0.72 } else { 1.0 };
        let cloud_top = core::array::from_fn(|k| {
            let f = (k as f32 / 2.0).min(1.0);
            grade(
                [
                    (218.0 + 37.0 * f) as u8,
                    (230.0 + 25.0 * f) as u8,
                    (247.0 + 8.0 * f) as u8,
                ],
                day,
                cloud_amount,
            )
        });
        let cloud_wall = core::array::from_fn(|k| {
            let f = k as f32 / 7.0;
            grade(
                [
                    (158.0 + 44.0 * f) as u8,
                    (182.0 + 36.0 * f) as u8,
                    (222.0 + 18.0 * f) as u8,
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
                [0.5, 0.32],
            ),
            Daylight::Sunset => (
                [255, 198, 132],
                [238, 122, 130],
                [84, 68, 150],
                [250, 182, 134],
                [0.95, 0.5],
            ),
            Daylight::Night => (
                [36, 56, 110],
                [18, 28, 70],
                [7, 10, 34],
                [28, 44, 92],
                [0.3, 0.42],
            ),
        };
        let pack = |c: [u8; 3]| rgba(f32::from(c[0]), f32::from(c[1]), f32::from(c[2]));
        let sky = core::array::from_fn(|i| {
            let t = i as f32 / 7.0;
            if t < 0.5 {
                blend(pack(sky_a), pack(sky_m), t * 2.0)
            } else {
                blend(pack(sky_m), pack(sky_b), t * 2.0 - 1.0)
            }
        });
        let mut jets = [[0; 96]; 3];
        for row in jet::PALETTE {
            jets[0][usize::from(row[0])] = grade(
                [row[1], row[2], row[3]],
                day,
                if day == Daylight::Sunset {
                    0.5
                } else if day == Daylight::Night {
                    0.16
                } else {
                    0.0
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
                grade([13, 15, 26], day, 0.3)
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
            trails: [[255, 255, 255], [214, 232, 250], [170, 204, 240]].map(|c| grade(c, day, 0.8)),
        }
    }
}

pub struct Frame {
    pub texture: Texture<'static>,
    pub width: usize,
    pub height: usize,
    depth: Vec<f32>,
    mask: Vec<u8>,
    backdrop: Vec<u32>,
    columns: Vec<i32>,
}
/// A short mutable borrow. No texture or allocator borrow survives the call.
pub struct Canvas<'a> {
    pub bytes: &'a mut [u8],
    pub depth: &'a mut [f32],
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
            depth: vec![f32::INFINITY; count],
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
    fn fill_depth(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: f32) {
        for y in y0.max(self.bounds[1])..y1.min(self.bounds[3]) {
            let at = y as usize * self.width + x;
            if self.get(at) == 0 {
                self.put(at, color);
                self.depth[at] = depth;
            }
        }
    }
    fn cloud_span(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: f32, thin: bool) {
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
            self.depth[start..end].fill(f32::INFINITY);
        }
        self.columns.fill(self.bounds[3]);
    }
}

#[derive(Clone, Copy, Default)]
pub struct Flame {
    pub points: [[f32; 2]; 3],
    pub radius: f32,
    pub tip_scale: f32,
    pub power: f32,
    pub alpha: f32,
    pub visibility: [f32; 3],
    pub seed: f32,
    pub length: f32,
}
#[derive(Clone, Copy)]
struct Voxel {
    a: f32,
    b: f32,
    color: u8,
}
pub struct Raster {
    pub frame: Frame,
    pub sheet: Frame,
    pub palette: Palette,
    pub flames: [Flame; 6],
    pub flame_count: usize,
    slices: [Vec<Voxel>; jet::LAYERS],
    shadow: [u8; jet::WIDTH * jet::LENGTH],
    seen: [[f32; 3]; 6],
}
/// Integral scaling preserves hard scene pixels and caps CPU work independently of window size.
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    let scale = 3usize
        .max(usize::from(width).div_ceil(MAX_WIDTH))
        .max(usize::from(height).div_ceil(MAX_HEIGHT));
    (
        usize::from(width).div_ceil(scale).max(8),
        usize::from(height).div_ceil(scale).max(8),
        scale,
    )
}
impl Raster {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        let width = width.clamp(8, MAX_WIDTH);
        let height = height.clamp(8, MAX_HEIGHT);
        let pivot = (height as f32 * 0.34).round();
        let tall = pivot.max(height as f32 - pivot);
        let side = (((width as f32 / 2.0).powi(2) + tall * tall).sqrt().ceil() as usize + 2) * 2;
        let mut cells = [0; jet::WIDTH * jet::LENGTH * jet::LAYERS];
        jet::decode(&mut cells);
        let mut shadow = [0; jet::WIDTH * jet::LENGTH];
        let slices = core::array::from_fn(|layer| {
            cells[layer * jet::WIDTH * jet::LENGTH..(layer + 1) * jet::WIDTH * jet::LENGTH]
                .iter()
                .enumerate()
                .filter_map(|(i, c)| {
                    if *c == 0 {
                        None
                    } else {
                        shadow[i] = 1;
                        Some(Voxel {
                            a: (i % jet::WIDTH) as f32 - (jet::WIDTH - 1) as f32 / 2.0,
                            b: jet::PIVOT[0] - (i / jet::WIDTH) as f32,
                            color: *c,
                        })
                    }
                })
                .collect()
        });
        Ok(Self {
            frame: Frame::new(width, height)?,
            sheet: Frame::new(side, side)?,
            palette: Palette::new(Daylight::Day),
            flames: [Flame::default(); 6],
            flame_count: 0,
            slices,
            shadow,
            seen: [[1.0; 3]; 6],
        })
    }
    pub fn bytes(&self) -> usize {
        self.frame.bytes()
            + self.sheet.bytes()
            + self
                .slices
                .iter()
                .map(|v| v.capacity() * core::mem::size_of::<Voxel>())
                .sum::<usize>()
            + core::mem::size_of::<Self>()
    }
    pub fn render(&mut self, world: &World, flight: &Flight) {
        let p = flight.projection(self.frame.width, self.frame.height);
        self.flame_count = 0;
        let rolled = p.cockpit > 0.0 || p.roll != 0.0;
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
                depth(*a).total_cmp(&depth(*b))
            });
            for i in &order[..flight.count] {
                draw_jet(canvas, flight, *i, &projection, palette, slices);
                record_flames(canvas, flight, *i, &projection, flames, flame_count, seen);
            }
        };
        if rolled {
            let mut projection = p;
            let half = (sheet.width / 2) as f32;
            let pivot = (frame.height as f32 * 0.34).round();
            projection.center = [half, half + (p.center[1] - pivot)];
            sheet.with_canvas(|canvas| {
                let c = p.roll.cos().abs();
                let s = p.roll.sin().abs();
                let tall = pivot.max(frame.height as f32 - pivot);
                let x = (c * frame.width as f32 / 2.0 + s * tall).ceil() + 2.0;
                let top = (c * pivot + s * frame.width as f32 / 2.0).ceil() + 2.0;
                let bottom =
                    (c * (frame.height as f32 - pivot) + s * frame.width as f32 / 2.0).ceil() + 2.0;
                canvas.bounds = [
                    (half - x).max(0.0) as i32,
                    (half - top).max(0.0) as i32,
                    (half + x).min(canvas.width as f32) as i32,
                    (half + bottom).min(canvas.height as f32) as i32,
                ];
                draw(canvas, projection);
            });
            let pixels = sheet.texture.read_pixels().expect("flight roll sheet");
            frame.with_canvas(|canvas| {
                let (s, c) = p.roll.sin_cos();
                let cx = canvas.width as f32 / 2.0;
                for y in 0..canvas.height {
                    let dy = y as f32 - pivot;
                    for x in 0..canvas.width {
                        let dx = x as f32 - cx;
                        let sx = (half + c * dx - s * dy + 0.5) as usize;
                        let sy = (half + s * dx + c * dy + 0.5) as usize;
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
                        frame.width as f32 / 2.0 + c * dx + s * dy,
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
    let bx = p.position[0].rem_euclid(world::SIDE as f32) + world::SIDE as f32 * 2.0;
    let by = p.position[1].rem_euclid(world::SIDE as f32) + world::SIDE as f32 * 2.0;
    let rank = (flight.settings.clouds.clamp(0.0, 1.0) * 16.0).round() as u8;
    let drift = [
        (flight.time * 7.0).rem_euclid(world::CLOUD_SIDE as f32),
        (flight.time * 3.0).rem_euclid(world::CLOUD_SIDE as f32),
    ];
    let below = canvas.bounds[3] as f32 - cy;
    let nearest = if q > 0.0 { -0.85 / q } else { -1e9 };
    let eye_height = if q > 0.0 { alt + slope / q } else { 1e9 };
    let dn = scale * slope + q * below;
    let mut v = if eye_height > world::MAX_HEIGHT as f32 + 1.0 && dn > 1e-6 {
        nearest.max((scale * (alt - world::MAX_HEIGHT as f32) - below) / dn - 2.0)
    } else {
        nearest
    };
    let limit = 1500.0 + 3.0 * alt;
    let fog_start = 300.0 + 2.0 * alt;
    let above = cy - canvas.bounds[1] as f32 + 2.0;
    let end = if scale * slope > q * above {
        limit.min((scale * alt + above) / (scale * slope - q * above))
    } else {
        limit
    };
    let mut den = 1.0 + q * v;
    let mut m = scale / den;
    let mut row = [0i32; world::MAX_HEIGHT + 1];
    for (z, r) in row.iter_mut().enumerate() {
        *r = (cy - m * (slope * v - alt) - m * z as f32 + 1e-5).floor() as i32;
    }
    let rows = scale * (slope + q * alt);
    for _ in 0..4000 {
        if v >= end {
            break;
        }
        let step = (den * den / rows).min(2.5 * den / scale).max(0.5);
        v += step;
        den = 1.0 + q * v;
        m = scale / den;
        let previous = row;
        for (z, r) in row.iter_mut().enumerate() {
            *r = (cy - m * (slope * v - alt) - m * z as f32 + 1e-5).floor() as i32;
        }
        let inverse = den / scale;
        let ax = cs * inverse;
        let ay = -sn * inverse;
        let mut wx = bx - cx * ax - v * sn + ax * canvas.bounds[0] as f32;
        let mut wy = by - cx * ay - v * cs + ay * canvas.bounds[0] as f32;
        let fog = if q > 0.0 && v > fog_start {
            (((v - fog_start) / (limit - fog_start)).sqrt() * 10.0)
                .round()
                .min(10.0)
                / 10.0
        } else {
            0.0
        };
        for px in canvas.bounds[0]..canvas.bounds[2] {
            let x = px as usize;
            let gx = wx as i32;
            let gy = wy as i32;
            wx += ax;
            wy += ay;
            if rank > 0 {
                let cloud =
                    world.cloud((gx as f32 - drift[0]) as i32, (gy as f32 - drift[1]) as i32);
                let thick = usize::from(cloud & 15);
                if thick > 0 && cloud >> 4 < rank {
                    let top = world::CLOUD_ALT as usize + thick - 1;
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
                        previous[world::CLOUD_ALT as usize],
                        row[world::CLOUD_ALT as usize],
                        blend(pal.cloud_under, pal.haze, fog),
                        v,
                        thick == 1,
                    );
                    for layer in (0..thick - 1).rev() {
                        let z = world::CLOUD_ALT as usize + layer;
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
                    (gx as f32 + pal.sun[0] * world::CLOUD_ALT - drift[0]) as i32,
                    (gy as f32 + pal.sun[1] * world::CLOUD_ALT - drift[1]) as i32,
                );
                shaded = c & 15 > 1 && c >> 4 < rank;
            }
            if !shaded {
                for jet in &flight.jets[..flight.count] {
                    let dx = gx as f32
                        - (bx + jet.position[0] - p.position[0] + pal.sun[0] * jet.position[2])
                        + pal.sun[0] * h as f32;
                    let dy = gy as f32
                        - (by + jet.position[1] - p.position[1] + pal.sun[1] * jet.position[2])
                        + pal.sun[1] * h as f32;
                    let reach = jet::LENGTH as f32 * 0.75 * flight.settings.jet_scale + 3.0;
                    if dx.abs() < reach && dy.abs() < reach {
                        let (s, c) = jet.yaw.sin_cos();
                        let a = (dx * c + dy * s)
                            / (jet.roll.cos().abs().max(0.3) * flight.settings.jet_scale);
                        let b = (dx * s - dy * c) / flight.settings.jet_scale;
                        let i = (a + jet::WIDTH as f32 / 2.0).floor() as i32;
                        let k = (jet::PIVOT[0] - b + 0.5).floor() as i32;
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
                let noise =
                    |x: i32, y: i32| world.noise[((y as usize & 255) * 256) + (x as usize & 255)];
                if material == 3 && noise(gx, gy).wrapping_add((flight.time * 36.0) as u8) < 120 {
                    top = if shaded { pal.top_shade[2] } else { pal.foam };
                } else if !shaded && inverse < 1.8 {
                    let n = noise((gx + ((flight.time * 6.0) as i32 & 1023)) >> 2, gy);
                    if n > 243
                        && noise(
                            (gx + 1024 - ((flight.time * 4.0) as i32 & 1023)) >> 2,
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
    let horizon = if p.perspective > 1e-7 {
        p.center[1] - p.scale * p.slope / p.perspective
    } else {
        -1e9
    };
    let focal = if p.perspective > 1e-7 {
        p.scale / p.perspective
    } else {
        1e9
    };
    for y in canvas.bounds[1]..canvas.bounds[3] {
        for x in canvas.bounds[0]..canvas.bounds[2] {
            let at = y as usize * canvas.width + x as usize;
            if canvas.get(at) != 0 {
                continue;
            }
            let mut color = pal.haze;
            if (y as f32) < horizon {
                let t = (horizon - y as f32) / (frame_height as f32 * 0.75) * 7.0;
                let mut band = t as usize;
                if (t - band as f32 - 0.7) * 3.4 * 16.0
                    > f32::from(BAYER[((y & 3) * 4 + (x & 3)) as usize])
                {
                    band += 1;
                }
                color = pal.sky[band.min(7)];
                if pal.day == Daylight::Night
                    && band > 1
                    && world::hash8((((x + (p.yaw * focal) as i32) & 4095) * 73 + y * 9973) as u32)
                        < 2
                {
                    color = pal.sun_color;
                }
                if pal.day != Daylight::Day {
                    let length = (pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1]).sqrt();
                    let dx = -pal.sun[0] / length;
                    let dy = -pal.sun[1] / length;
                    let (sn, cs) = p.yaw.sin_cos();
                    let along = -(dx * sn + dy * cs);
                    if along > 0.2 {
                        let radius = canvas.width as f32
                            * if pal.day == Daylight::Sunset {
                                0.05
                            } else {
                                0.022
                            };
                        let sx = p.center[0] + focal * (dx * cs - dy * sn) / along;
                        let sy = horizon
                            - focal
                                * if pal.day == Daylight::Sunset {
                                    0.12
                                } else {
                                    0.3
                                }
                                / along;
                        let distance =
                            ((x as f32 - sx).powi(2) + (y as f32 - sy).powi(2)).sqrt() / radius;
                        if distance < 1.0 {
                            color = pal.sun_color;
                        } else if distance < 1.9
                            && (1.9 - distance) * 14.0
                                > f32::from(BAYER[((y & 3) * 4 + (x & 3)) as usize])
                        {
                            color = pal.sun_glow;
                        }
                    }
                }
            } else if p.perspective == 0.0 {
                color = pal.top[0];
            }
            canvas.put(at, color);
        }
    }
}

fn draw_trails(canvas: &mut Canvas<'_>, flight: &Flight, p: &Projection, pal: &Palette) {
    for (i, jet) in flight.jets[..flight.count].iter().enumerate() {
        if i == 0 && p.jet_alpha < 0.5 {
            continue;
        }
        for wing in &jet.trails {
            let mut last: Option<[f32; 4]> = None;
            for n in 0..jet.trail_len {
                let point = wing[(jet.trail_head + TRAIL_CAPACITY - 1 - n) % TRAIL_CAPACITY];
                let age = (flight.time - point.born) / (2.6 * (0.3 + 0.7 * jet.load));
                if age >= 1.0 {
                    break;
                }
                let Some(next) = p.project(point.position) else {
                    last = None;
                    continue;
                };
                if let Some(prev) = last {
                    let span = (next[0] - prev[0]).abs().max((next[1] - prev[1]).abs());
                    if span < 600.0 {
                        let steps = span.ceil().max(1.0) as usize;
                        let alpha = if age < 0.5 { 1.0 } else { (1.0 - age) * 2.0 };
                        let color = pal.trails[if age < 0.25 {
                            0
                        } else if age < 0.6 {
                            1
                        } else {
                            2
                        }];
                        for k in 0..steps {
                            let x = (prev[0] + (next[0] - prev[0]) * k as f32 / steps as f32)
                                .round() as i32;
                            let y = (prev[1] + (next[1] - prev[1]) * k as f32 / steps as f32)
                                .round() as i32;
                            for yy in y..y + if next[2] > 2.4 { 2 } else { 1 } {
                                for xx in x..x + if next[2] > 2.4 { 2 } else { 1 } {
                                    if xx < 0
                                        || yy < 0
                                        || xx >= canvas.width as i32
                                        || yy >= canvas.height as i32
                                    {
                                        continue;
                                    }
                                    let at = yy as usize * canvas.width + xx as usize;
                                    if canvas.depth[at] >= next[3] - 1.0
                                        && alpha * 16.0
                                            > f32::from(BAYER[((yy & 3) * 4 + (xx & 3)) as usize])
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
        if smoke.life <= 0.0 || age < 0.0 || age >= smoke.life {
            continue;
        }
        let t = age / smoke.life;
        let push = (1.0 - (-age * 5.0).exp()) / 5.0;
        let Some(point) = p.project([
            smoke.position[0] + smoke.push[0] * push,
            smoke.position[1] + smoke.push[1] * push,
            smoke.position[2] + age * 2.5,
        ]) else {
            continue;
        };
        let radius = (smoke.size * (1.0 + 2.4 * t) * point[2]).max(0.8);
        if radius > 60.0 {
            continue;
        }
        let alpha = smoke.strength
            * (1.0 - t).powi(2)
            * if t < 0.08 { t / 0.08 } else { 1.0 }
            * if day == Daylight::Night { 0.55 } else { 1.0 };
        let shade = 26.0 + 70.0 * t;
        let color = rgba(shade, shade + 1.0, shade + 7.0);
        for y in ((point[1] - radius).floor() as i32).max(0)
            ..=((point[1] + radius).ceil() as i32).min(canvas.height as i32 - 1)
        {
            for x in ((point[0] - radius).floor() as i32).max(0)
                ..=((point[0] + radius).ceil() as i32).min(canvas.width as i32 - 1)
            {
                let d = ((x as f32 - point[0]).powi(2) + (y as f32 - point[1]).powi(2))
                    / (radius * radius);
                let at = y as usize * canvas.width + x as usize;
                if d <= 1.0 && canvas.depth[at] >= point[3] - 1.0 {
                    canvas.put(at, blend(canvas.get(at), color, alpha * (1.0 - d * 0.7)));
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
    slices: &[Vec<Voxel>; jet::LAYERS],
) {
    let jet = &flight.jets[index];
    let fade = if index == 0 { p.jet_alpha } else { 1.0 };
    if fade < 0.004 {
        return;
    }
    let Some(center) = p.project(jet.position) else {
        return;
    };
    if center[2] <= 0.0 {
        return;
    }
    let reach = jet::LENGTH as f32 * flight.settings.jet_scale * center[2] + 40.0;
    if center[0] < -reach
        || center[1] < -reach - canvas.height as f32
        || center[0] > canvas.width as f32 + reach
        || center[1] > canvas.height as f32 + reach
    {
        return;
    }
    canvas.mask.fill(0);
    let axes = jet.axes();
    let (sn, cs) = p.yaw.sin_cos();
    let sc = flight.settings.jet_scale;
    let project_axis = |a: [f32; 3]| {
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
    let plane = if p.perspective < 1e-6 {
        if p.slope * up[2] - up[1] >= 0.0 {
            1e9
        } else {
            -1e9
        }
    } else {
        (-cu * up[0] + (-1.0 / p.perspective - cv) * up[1] + (p.slope / p.perspective - cw) * up[2])
            / (sc * sc)
    };
    let mut order: [usize; jet::LAYERS] = core::array::from_fn(|i| i);
    order.sort_unstable_by(|a, b| {
        (((*b as f32 - jet::PIVOT[1]) * flight.spread - plane).abs())
            .total_cmp(&((*a as f32 - jet::PIVOT[1]) * flight.spread - plane).abs())
    });
    let light = |side: f32| {
        let n = core::array::from_fn::<_, 3, _>(|i| axes[2][i] + side * 0.4 * axes[0][i]);
        let dot = -n[0] * pal.sun[0] - n[1] * pal.sun[1] + n[2] * 1.1;
        let norm = ((pal.sun[0] * pal.sun[0] + pal.sun[1] * pal.sun[1] + 1.21)
            * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]))
            .sqrt();
        let v = dot / norm;
        if v > if pal.day == Daylight::Night {
            0.74
        } else {
            0.9
        } {
            1
        } else if v < if pal.day == Daylight::Night {
            0.4
        } else {
            0.66
        } {
            2
        } else {
            0
        }
    };
    let lights = [light(-1.0), light(1.0)];
    let big = sc * p.scale
        / (1.0 + p.perspective * cv - p.perspective * jet::LENGTH as f32 * 0.6 * sc).max(0.2);
    let samples = (big * 2.0 - 0.24).ceil().clamp(2.0, 8.0) as usize;
    let mut offsets = [[0.0; 2]; 64];
    for i in 0..samples {
        for k in 0..samples {
            let a = (i as f32 + 0.5) / samples as f32 - 0.5;
            let b = (k as f32 + 0.5) / samples as f32 - 0.5;
            offsets[i * samples + k] = [
                center[2] * (a * right[0] + b * forward[0]),
                -center[2]
                    * (a * (right[2] + p.slope * right[1])
                        + b * (forward[2] + p.slope * forward[1])),
            ];
        }
    }
    let mut bounds = [canvas.width as i32, canvas.height as i32, -1, -1];
    for layer in order {
        let intro = flight.intro - index as f32 * 0.25;
        let t = (intro - layer as f32 * 0.07) / 0.36;
        if t <= 0.0 {
            continue;
        }
        let drop = if t < 1.0 {
            -(1.0 - t * t) * (canvas.height as f32 / 2.0 + 44.0)
        } else {
            0.0
        };
        let height = (layer as f32 - jet::PIVOT[1]) * flight.spread;
        let bu = cu + height * up[0];
        let bv = cv + height * up[1];
        let bw = cw + height * up[2];
        for voxel in &slices[layer] {
            let depth = bv + voxel.a * right[1] + voxel.b * forward[1];
            let den = 1.0 + p.perspective * depth;
            if den < 0.2 {
                continue;
            }
            let m = p.scale / den;
            let color = pal.jets[lights[usize::from(voxel.a >= 0.0)]][usize::from(voxel.color)];
            let x = p.center[0] + m * (bu + voxel.a * right[0] + voxel.b * forward[0]);
            let y = p.center[1]
                - m * (bw + voxel.a * right[2] + voxel.b * forward[2] + p.slope * depth)
                + drop;
            for offset in &offsets[..samples * samples] {
                let xx = (x + offset[0] * m / center[2]).round() as i32;
                let yy = (y + offset[1] * m / center[2]).round() as i32;
                if xx < 0 || yy < 0 || xx >= canvas.width as i32 || yy >= canvas.height as i32 {
                    continue;
                }
                let at = yy as usize * canvas.width + xx as usize;
                if canvas.depth[at] < depth - 1.0 {
                    continue;
                }
                if fade < 1.0 && canvas.mask[at] == 0 {
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
                if fade < 1.0 {
                    canvas.put(at, blend(canvas.backdrop[at], canvas.get(at), fade));
                }
            } else if fade >= 0.5
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
    seen: &mut [[f32; 3]; 6],
) {
    let jet: &Aircraft = &flight.jets[index];
    let alpha = if index == 0 { p.jet_alpha } else { 1.0 } * (1.0 - p.cockpit_in());
    let flick =
        0.82 + 0.18 * (flight.time * 47.0 + index as f32 * 2.0).sin() * (flight.time * 31.0).sin();
    let length = flight.flame_length() * (0.94 + 0.3 * (flick - 0.82)) * 1.3;
    if length <= 0.6 || alpha < 0.01 || flight.spread >= 2.6 {
        return;
    }
    let width = 1.63 * (1.0 + 0.12 * flight.boost) * 1.5;
    for side in [-1.0, 1.0] {
        if *count == out.len() {
            break;
        }
        let mut projected = [[0.0; 4]; 3];
        let mut visible = true;
        for (n, target) in projected.iter_mut().enumerate() {
            let curl = if n == 2 { length } else { 0.0 };
            let local = [
                side * 1.96 + flight.flame_bend() * curl,
                jet::PIVOT[0] - 43.6 - n as f32 * 0.5 * length,
                (4.1 - 0.5 - jet::PIVOT[1]) * flight.spread + flight.climb_tilt * 0.35 * curl,
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
        let slot = index * 2 + usize::from(side > 0.0);
        let mut visibility = [0.0; 3];
        for n in 0..3 {
            let point = if n == 1 {
                core::array::from_fn(|k| {
                    0.25 * projected[0][k] + 0.5 * projected[1][k] + 0.25 * projected[2][k]
                })
            } else {
                projected[n]
            };
            let mut open = 0;
            for (dx, dy) in [
                (0.0, 0.0),
                (-radius, 0.0),
                (radius, 0.0),
                (0.0, -radius),
                (0.0, radius),
            ] {
                let x = (point[0] + dx).round() as i32;
                let y = (point[1] + dy).round() as i32;
                if x < 0
                    || y < 0
                    || x >= canvas.width as i32
                    || y >= canvas.height as i32
                    || canvas.depth[y as usize * canvas.width + x as usize] >= point[3] - 1.0
                {
                    open += 1;
                }
            }
            seen[slot][n] += (open as f32 / 5.0 - seen[slot][n]) * 0.3;
            visibility[n] = seen[slot][n];
        }
        out[*count] = Flame {
            points: projected.map(|v| [v[0], v[1]]),
            radius,
            tip_scale: projected[2][2] / projected[0][2],
            power: flight.boost,
            alpha,
            visibility,
            seed: slot as f32,
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
        let world = World::new(7, 0.33);
        let mut flight = Flight::new(&world, Settings::default());
        let mut raster = Raster::new(240, 150).unwrap();
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            for _ in 0..240 {
                flight.step(&world, 0.016, 240, 150);
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
            canvas.fill_depth(3, 2, 5, rgba(10.0, 20.0, 30.0), 1.0);
            canvas.fill_depth(3, 0, 8, rgba(200.0, 20.0, 30.0), 9.0);
            assert_eq!(canvas.get(3 * 8 + 3), rgba(10.0, 20.0, 30.0));
            assert_eq!(canvas.depth[3 * 8 + 3], 1.0);
        });
    }
}
