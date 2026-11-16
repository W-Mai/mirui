#![doc = " Height-field scan conversion into reusable ordinary Texture storage."]
mod compact;
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
}
struct Workspace {
    depth: Vec<Fixed>,
    mask: Vec<u8>,
    backdrop: Vec<u32>,
    columns: Vec<i32>,
}
#[doc = " A short mutable borrow. No texture or allocator borrow survives the call."]
pub struct Canvas<'a> {
    pub bytes: &'a mut [u8],
    pub width: usize,
    pub height: usize,
    pub bounds: [i32; 4],
    pub(super) clip: [i32; 4],
    pitch: usize,
    origin: [i32; 2],
    extent: [usize; 2],
}
struct SceneCanvas<'a, 'b> {
    color: &'a mut Canvas<'b>,
    depth: &'a mut [Fixed],
    mask: &'a mut [u8],
    backdrop: &'a mut [u32],
    columns: &'a mut [i32],
}
impl Frame {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        Self::with_format(width, height, ColorFormat::RGBA8888)
    }
    fn with_format(width: usize, height: usize, format: ColorFormat) -> Result<Self, RenderError> {
        Ok(Self {
            texture: Texture::owned(width as u16, height as u16, format)?.with_cache_revision(0),
            width,
            height,
        })
    }
    pub fn with_canvas<R>(&mut self, f: impl FnOnce(&mut Canvas<'_>) -> R) -> R {
        self.with_region(
            [self.width, self.height],
            [0, 0],
            [0, 0, self.width as i32, self.height as i32],
            f,
        )
    }
    pub(super) fn with_region<R>(
        &mut self,
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        f: impl FnOnce(&mut Canvas<'_>) -> R,
    ) -> R {
        assert_eq!(self.texture.format, ColorFormat::RGBA8888);
        let mut pixels = self.texture.pixels_mut().expect("owned flight frame");
        let mut canvas = Canvas {
            bytes: pixels.bytes_mut(),
            width: virtual_size[0],
            height: virtual_size[1],
            bounds,
            clip: bounds,
            pitch: self.width,
            origin,
            extent: [self.width, self.height],
        };
        canvas.set_bounds(bounds);
        f(&mut canvas)
    }
    pub fn bytes(&self) -> usize {
        self.width * self.height * self.texture.format.bytes_per_pixel()
    }
}
impl Workspace {
    fn new(width: usize, height: usize) -> Self {
        let count = width * height;
        Self {
            depth: vec![Fixed::MAX; count],
            mask: vec![0; count],
            backdrop: vec![0; count],
            columns: vec![height as i32; width],
        }
    }
    fn bytes(&self) -> usize {
        self.depth.capacity() * 4
            + self.mask.capacity()
            + self.backdrop.capacity() * 4
            + self.columns.capacity() * 4
    }
    fn with_canvas<R>(
        &mut self,
        frame: &mut Frame,
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        self.with_region(
            frame,
            [frame.width, frame.height],
            [0, 0],
            [0, 0, frame.width as i32, frame.height as i32],
            draw,
        )
    }
    fn with_region<R>(
        &mut self,
        frame: &mut Frame,
        virtual_size: [usize; 2],
        origin: [i32; 2],
        bounds: [i32; 4],
        draw: impl FnOnce(&mut SceneCanvas<'_, '_>) -> R,
    ) -> R {
        let count = frame.width * frame.height;
        let width = frame.width;
        frame.with_region(virtual_size, origin, bounds, |color| {
            draw(&mut SceneCanvas {
                color,
                depth: &mut self.depth[..count],
                mask: &mut self.mask[..count],
                backdrop: &mut self.backdrop[..count],
                columns: &mut self.columns[..width],
            })
        })
    }
}
impl Canvas<'_> {
    pub(super) fn index(&self, x: i32, y: i32) -> usize {
        ((y - self.origin[1]) * self.pitch as i32 + x - self.origin[0]) as usize
    }
    pub(super) fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.clip[0] && y >= self.clip[1] && x < self.clip[2] && y < self.clip[3]
    }
    fn set_bounds(&mut self, bounds: [i32; 4]) {
        self.bounds = bounds;
        self.clip = [
            bounds[0].max(self.origin[0]).max(0),
            bounds[1].max(self.origin[1]).max(0),
            bounds[2]
                .min(self.origin[0] + self.extent[0] as i32)
                .min(self.width as i32),
            bounds[3]
                .min(self.origin[1] + self.extent[1] as i32)
                .min(self.height as i32),
        ];
    }
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
        if self.contains(x, y) {
            self.put(self.index(x, y), color);
        }
    }
}
impl SceneCanvas<'_, '_> {
    fn get(&self, index: usize) -> u32 {
        self.color.get(index)
    }
    fn put(&mut self, index: usize, color: u32) {
        self.color.put(index, color);
    }
    fn sample_depth(&self, x: i32, y: i32) -> Option<Fixed> {
        let [left, top, right, bottom] = self.color.bounds;
        if x < left || y < top || x >= right || y >= bottom {
            Some(Fixed::MAX)
        } else if !self.color.contains(x, y) {
            None
        } else {
            Some(self.depth[self.color.index(x, y)])
        }
    }
    fn fill_depth(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed) {
        for y in y0.max(self.color.clip[1])..y1.min(self.color.clip[3]) {
            let at = self.color.index(x as i32, y);
            if self.get(at) == 0 {
                self.put(at, color);
                self.depth[at] = depth;
            }
        }
    }
    fn cloud_span(&mut self, x: usize, y0: i32, y1: i32, color: u32, depth: Fixed, thin: bool) {
        for y in y0.max(self.color.clip[1])..y1.min(self.color.clip[3]) {
            if thin && (x + y as usize) & 1 == 0 {
                continue;
            }
            let at = self.color.index(x as i32, y);
            if self.get(at) == 0 {
                self.put(at, color);
                self.depth[at] = depth;
            }
        }
    }
    fn clear(&mut self) {
        if self.color.clip[0] >= self.color.clip[2] || self.color.clip[1] >= self.color.clip[3] {
            return;
        }
        for y in self.color.clip[1]..self.color.clip[3] {
            let start = self.color.index(self.color.clip[0], y);
            let end = start + (self.color.clip[2] - self.color.clip[0]) as usize;
            self.color.bytes[start * 4..end * 4].fill(0);
            self.depth[start..end].fill(Fixed::MAX);
        }
        self.columns.fill(self.color.bounds[3]);
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
    workspace: Workspace,
    output_tile: Option<Frame>,
    pub palette: Palette,
    pub flames: [Flame; 6],
    pub flame_count: usize,
    slices: [&'static [jet::Voxel]; jet::LAYERS],
    pub(super) seen: [[Fixed; 3]; 6],
}
struct Scene<'a> {
    world: &'a World,
    flight: &'a Flight,
    palette: &'a Palette,
    slices: &'a [&'static [jet::Voxel]; jet::LAYERS],
    frame_height: usize,
}
impl Scene<'_> {
    fn draw(&self, canvas: &mut SceneCanvas<'_, '_>, projection: Projection) -> [usize; 3] {
        let order = self.order(projection);
        self.draw_ordered(canvas, projection, &order);
        order
    }
    fn order(&self, projection: Projection) -> [usize; 3] {
        let mut order = [0, 1, 2];
        let (sn, cs) = projection.yaw.sin_cos();
        order[..self.flight.count].sort_unstable_by(|a, b| {
            let depth = |i: usize| {
                (self.flight.jets[i].position[0] - projection.position[0]) * sn
                    + (self.flight.jets[i].position[1] - projection.position[1]) * cs
            };
            depth(*a).cmp(&depth(*b))
        });
        order
    }
    fn draw_ordered(
        &self,
        canvas: &mut SceneCanvas<'_, '_>,
        projection: Projection,
        order: &[usize; 3],
    ) {
        draw_world(canvas, self.world, self.flight, &projection, self.palette);
        draw_sky(canvas, &projection, self.palette, self.frame_height);
        draw_trails(canvas, self.flight, &projection, self.palette);
        draw_smoke(canvas, self.flight, &projection, self.palette.day);
        for i in &order[..self.flight.count] {
            draw_jet(
                canvas,
                self.flight,
                *i,
                &projection,
                self.palette,
                self.slices,
            );
        }
    }
}
fn sheet_side(width: usize, height: usize) -> usize {
    let pivot = (Fixed::from_int(height as i32) * Fixed::from_ratio(34, 100)).round();
    let tall = pivot.max(Fixed::from_int(height as i32) - pivot);
    (((Fixed::from_int(width as i32) / 2).powi(2) + tall * tall)
        .sqrt()
        .ceil()
        .to_int() as usize
        + 2)
        * 2
}
struct Mapping {
    projection: Projection,
    size: [usize; 2],
    bounds: [i32; 4],
    rolled: bool,
    half: Fixed,
    cx: Fixed,
    pivot: Fixed,
    sine: Fixed,
    cosine: Fixed,
}
impl Mapping {
    fn new(mut projection: Projection, width: usize, height: usize) -> Self {
        let rolled = projection.cockpit > Fixed::ZERO || projection.roll != Fixed::ZERO;
        let cx = Fixed::from_int(width as i32) / 2;
        let pivot = (Fixed::from_int(height as i32) * Fixed::from_ratio(34, 100)).round();
        let (sine, cosine) = projection.roll.sin_cos();
        let mut mapping = Self {
            projection,
            size: [width, height],
            bounds: [0, 0, width as i32, height as i32],
            rolled,
            half: Fixed::ZERO,
            cx,
            pivot,
            sine,
            cosine,
        };
        if rolled {
            let side = sheet_side(width, height);
            let half = Fixed::from_int((side / 2) as i32);
            let tall = pivot.max(Fixed::from_int(height as i32) - pivot);
            let x = (cosine.abs() * Fixed::from_int(width as i32) / 2 + sine.abs() * tall).ceil()
                + Fixed::from_int(2);
            let (above, below) = if cosine < Fixed::ZERO {
                (Fixed::from_int(height as i32) - pivot, pivot)
            } else {
                (pivot, Fixed::from_int(height as i32) - pivot)
            };
            let top = (cosine.abs() * above + sine.abs() * Fixed::from_int(width as i32) / 2)
                .ceil()
                + Fixed::from_int(2);
            let bottom = (cosine.abs() * below + sine.abs() * Fixed::from_int(width as i32) / 2)
                .ceil()
                + Fixed::from_int(2);
            projection.center = [half, half + (projection.center[1] - pivot)];
            mapping.projection = projection;
            mapping.size = [side, side];
            mapping.half = half;
            mapping.bounds = [
                (half - x).max(Fixed::ZERO).to_int(),
                (half - top).max(Fixed::ZERO).to_int(),
                (half + x).min(Fixed::from_int(side as i32)).to_int(),
                (half + bottom).min(Fixed::from_int(side as i32)).to_int(),
            ];
        }
        mapping
    }
    fn source_at(&self, x: i32, y: i32) -> [i32; 2] {
        if !self.rolled {
            return [x, y];
        }
        let dx = Fixed::from_int(x) - self.cx;
        let dy = Fixed::from_int(y) - self.pivot;
        [
            (self.half + self.cosine * dx - self.sine * dy + Fixed::from_ratio(1, 2)).to_int(),
            (self.half + self.sine * dx + self.cosine * dy + Fixed::from_ratio(1, 2)).to_int(),
        ]
    }
    fn unroll(&self, point: [Fixed; 2]) -> [Fixed; 2] {
        if !self.rolled {
            return point;
        }
        let dx = point[0] - self.half;
        let dy = point[1] - self.half;
        [
            self.cx + self.cosine * dx + self.sine * dy,
            self.pivot - self.sine * dx + self.cosine * dy,
        ]
    }
}
#[doc = " Integral scaling preserves hard scene pixels and caps CPU work independently of window size."]
pub fn scene_size(width: u16, height: u16) -> (usize, usize, usize) {
    super::Profile::Desktop.scene_size(width, height)
}
impl Raster {
    pub fn new(width: usize, height: usize) -> Result<Self, RenderError> {
        Self::allocate(width, height, false)
    }
    pub(super) fn new_compact(width: usize, height: usize) -> Result<Self, RenderError> {
        assert!(width <= 128 && height <= 128);
        Self::allocate(width, height, true)
    }
    fn allocate(width: usize, height: usize, compact: bool) -> Result<Self, RenderError> {
        let width = width.clamp(8, MAX_WIDTH);
        let height = height.clamp(8, MAX_HEIGHT);
        let side = if compact {
            compact::SOURCE_SIDE
        } else {
            sheet_side(width, height)
        };
        let slices = jet::slices();
        Ok(Self {
            frame: Frame::with_format(
                width,
                height,
                if compact {
                    ColorFormat::RGB565
                } else {
                    ColorFormat::RGBA8888
                },
            )?,
            sheet: Frame::new(side, side)?,
            workspace: Workspace::new(side, side),
            output_tile: if compact {
                Some(Frame::new(compact::TILE_SIDE, compact::TILE_SIDE)?)
            } else {
                None
            },
            palette: Palette::new(Daylight::Day),
            flames: [Flame::default(); 6],
            flame_count: 0,
            slices,
            seen: [[Fixed::ONE; 3]; 6],
        })
    }
    pub fn bytes(&self) -> usize {
        self.frame.bytes()
            + self.sheet.bytes()
            + self.workspace.bytes()
            + self.output_tile.as_ref().map_or(0, Frame::bytes)
            + core::mem::size_of::<Self>()
    }
    pub fn render(&mut self, world: &World, flight: &Flight) {
        assert!(
            self.output_tile.is_none(),
            "compact rendering requires its cockpit pass"
        );
        let p = flight.projection(self.frame.width, self.frame.height);
        self.flame_count = 0;
        let Self {
            frame,
            sheet,
            workspace,
            palette,
            flames,
            flame_count,
            slices,
            seen,
            output_tile: _,
        } = self;
        let frame_height = frame.height;
        let scene = Scene {
            world,
            flight,
            palette,
            slices,
            frame_height,
        };
        let mut draw = |canvas: &mut SceneCanvas<'_, '_>, projection: Projection| {
            let order = scene.draw(canvas, projection);
            for i in &order[..flight.count] {
                record_flames(canvas, flight, *i, &projection, flames, flame_count, seen);
            }
        };
        let mapping = Mapping::new(p, frame.width, frame.height);
        if mapping.rolled {
            workspace.with_region(sheet, mapping.size, [0, 0], mapping.bounds, |canvas| {
                draw(canvas, mapping.projection)
            });
            let pixels = sheet.texture.read_pixels().expect("flight roll sheet");
            frame.with_canvas(|canvas| {
                for y in 0..canvas.height {
                    for x in 0..canvas.width {
                        let [sx, sy] = mapping.source_at(x as i32, y as i32);
                        let at = (sy as usize * sheet.width + sx as usize) * 4;
                        canvas.bytes[(y * canvas.width + x) * 4..(y * canvas.width + x + 1) * 4]
                            .copy_from_slice(&pixels.bytes()[at..at + 4]);
                    }
                }
            });
            for flame in &mut flames[..*flame_count] {
                for point in &mut flame.points {
                    *point = mapping.unroll(*point);
                }
            }
        } else {
            workspace.with_canvas(frame, |canvas| draw(canvas, p));
        }
    }
    pub(super) fn render_with_cockpit(
        &mut self,
        world: &World,
        flight: &Flight,
        cockpit: &super::cockpit::Cockpit,
    ) {
        if self.output_tile.is_some() {
            self.render_compact(world, flight, cockpit);
        } else {
            self.render(world, flight);
            self.frame
                .with_canvas(|canvas| cockpit.draw(canvas, world, flight, &self.palette));
        }
    }
}
fn draw_world(
    canvas: &mut SceneCanvas<'_, '_>,
    world: &World,
    flight: &Flight,
    p: &Projection,
    pal: &Palette,
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
    let below = Fixed::from_int(canvas.color.bounds[3]) - cy;
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
    let above = cy - Fixed::from_int(canvas.color.bounds[1]) + Fixed::from_int(2);
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
        let mut wx = bx - cx * ax - v * sn + ax * Fixed::from_int(canvas.color.clip[0]);
        let mut wy = by - cx * ay - v * cs + ay * Fixed::from_int(canvas.color.clip[0]);
        let fog = if q > Fixed64::ZERO && v > fog_start {
            (((v - fog_start) / (limit - fog_start)).sqrt() * Fixed::from_int(10))
                .round()
                .min(Fixed::from_int(10))
                / Fixed::from_int(10)
        } else {
            Fixed::ZERO
        };
        for px in canvas.color.clip[0]..canvas.color.clip[2] {
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
            let mut column = canvas.columns[(x as i32 - canvas.color.origin[0]) as usize];
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
            canvas.columns[(x as i32 - canvas.color.origin[0]) as usize] = column;
        }
    }
}
fn draw_sky(canvas: &mut SceneCanvas<'_, '_>, p: &Projection, pal: &Palette, frame_height: usize) {
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
    for y in canvas.color.clip[1]..canvas.color.clip[3] {
        for x in canvas.color.clip[0]..canvas.color.clip[2] {
            let at = canvas.color.index(x, y);
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
                        let radius = Fixed::from_int((canvas.color.width) as i32)
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
fn draw_trails(canvas: &mut SceneCanvas<'_, '_>, flight: &Flight, p: &Projection, pal: &Palette) {
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
                last = Some(next);
            }
        }
    }
}
fn draw_smoke(canvas: &mut SceneCanvas<'_, '_>, flight: &Flight, p: &Projection, day: Daylight) {
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
fn draw_jet(
    canvas: &mut SceneCanvas<'_, '_>,
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
        || center[1] < -reach - Fixed::from_int((canvas.color.height) as i32)
        || center[0] > Fixed::from_int((canvas.color.width) as i32) + reach
        || center[1] > Fixed::from_int((canvas.color.height) as i32) + reach
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
    let mut bounds = [
        canvas.color.width as i32,
        canvas.color.height as i32,
        -1,
        -1,
    ];
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
                * (Fixed::from_int((canvas.color.height) as i32) / Fixed::from_int(2)
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
                if !canvas.color.contains(xx, yy) {
                    continue;
                }
                let at = canvas.color.index(xx, yy);
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
    for y in (bounds[1] - 1).max(canvas.color.clip[1] + 1)
        ..=(bounds[3] + 1).min(canvas.color.clip[3] - 2)
    {
        for x in (bounds[0] - 1).max(canvas.color.clip[0] + 1)
            ..=(bounds[2] + 1).min(canvas.color.clip[2] - 2)
        {
            let at = canvas.color.index(x, y);
            if canvas.mask[at] != 0 {
                if fade < Fixed::ONE {
                    canvas.put(at, blend(canvas.backdrop[at], canvas.get(at), fade));
                }
            } else if fade >= Fixed::from_ratio(5, 10)
                && (canvas.mask[at - 1] != 0
                    || canvas.mask[at + 1] != 0
                    || canvas.mask[at - canvas.color.pitch] != 0
                    || canvas.mask[at + canvas.color.pitch] != 0)
            {
                canvas.put(at, pal.outline);
            }
        }
    }
}
#[derive(Clone, Copy, Default)]
struct PendingFlame {
    flame: Flame,
    projected: [[Fixed; 4]; 3],
    slot: usize,
    resolved: u16,
    open: u16,
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
    fn resolve(&mut self, canvas: &SceneCanvas<'_, '_>) {
        for index in 0..15 {
            let bit = 1 << index;
            if self.resolved & bit != 0 {
                continue;
            }
            let (x, y, depth) = self.query(index);
            if let Some(value) = canvas.sample_depth(x, y) {
                self.resolved |= bit;
                if value >= depth - Fixed::ONE {
                    self.open |= bit;
                }
            }
        }
    }
    fn missing(&self) -> Option<[i32; 2]> {
        (0..15)
            .find(|index| self.resolved & (1 << index) == 0)
            .map(|index| {
                let (x, y, _) = self.query(index);
                [x, y]
            })
    }
    fn finish(mut self, seen: &mut [[Fixed; 3]; 6]) -> Flame {
        assert_eq!(self.resolved, 0x7fff, "unresolved flame visibility");
        for (n, (value, target)) in seen[self.slot]
            .iter_mut()
            .zip(self.flame.visibility.iter_mut())
            .enumerate()
        {
            let open = ((self.open >> (n * 5)) & 31).count_ones() as i32;
            *value +=
                (Fixed::from_int(open) / Fixed::from_int(5) - *value) * Fixed::from_ratio(3, 10);
            *target = *value;
        }
        self.flame
    }
}
fn record_flames(
    canvas: &SceneCanvas<'_, '_>,
    flight: &Flight,
    index: usize,
    p: &Projection,
    out: &mut [Flame; 6],
    count: &mut usize,
    seen: &mut [[Fixed; 3]; 6],
) {
    let mut pending = [PendingFlame::default(); 2];
    let mut length = 0;
    project_flames(flight, index, p, &mut pending, &mut length);
    for mut value in pending[..length].iter().copied() {
        value.resolve(canvas);
        assert!(value.missing().is_none());
        out[*count] = value.finish(seen);
        *count += 1;
    }
}
fn project_flames(
    flight: &Flight,
    index: usize,
    p: &Projection,
    out: &mut [PendingFlame],
    count: &mut usize,
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
            resolved: 0,
            open: 0,
        };
        *count += 1;
    }
}
#[cfg(test)]
mod tests {
    use super::super::flight::{Settings, View};
    use super::*;
    #[test]
    fn region_writes_preserve_global_scene_samples_and_depth() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        let raster = Raster::new(128, 128).unwrap();
        let mut full = Frame::new(128, 128).unwrap();
        let mut full_workspace = Workspace::new(128, 128);
        let mut region = Frame::new(40, 40).unwrap();
        let mut region_workspace = Workspace::new(40, 40);
        let bounds = [3, 2, 126, 124];
        for view in [View::Overhead, View::Chase, View::Cockpit] {
            flight.view = view;
            flight.roll();
            for _ in 0..300 {
                flight.step(&world, 16, 128, 128);
            }
            let scene = Scene {
                world: &world,
                flight: &flight,
                palette: &raster.palette,
                slices: &raster.slices,
                frame_height: 128,
            };
            let projection = flight.projection(128, 128);
            full_workspace.with_region(&mut full, [128, 128], [0, 0], bounds, |canvas| {
                scene.draw(canvas, projection);
            });
            for origin in [[-2, -2], [0, 0], [17, 37], [44, 20], [104, 90]] {
                region_workspace.with_region(&mut region, [128, 128], origin, bounds, |canvas| {
                    assert_eq!(
                        canvas.sample_depth(80, 80),
                        if canvas.color.contains(80, 80) {
                            Some(canvas.depth[canvas.color.index(80, 80)])
                        } else {
                            None
                        }
                    );
                    scene.draw(canvas, projection);
                });
                let reference = full.texture.read_pixels().unwrap();
                let local = region.texture.read_pixels().unwrap();
                for y in (origin[1] + 1).max(bounds[1])..(origin[1] + 39).min(bounds[3]) {
                    for x in (origin[0] + 1).max(bounds[0])..(origin[0] + 39).min(bounds[2]) {
                        let a = y as usize * 128 + x as usize;
                        let b = ((y - origin[1]) * 40 + x - origin[0]) as usize;
                        assert_eq!(
                            &reference.bytes()[a * 4..a * 4 + 4],
                            &local.bytes()[b * 4..b * 4 + 4],
                            "view {view:?} origin {origin:?} at {x},{y}"
                        );
                        assert_eq!(full_workspace.depth[a], region_workspace.depth[b]);
                    }
                }
            }
        }
    }
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
        let mut workspace = Workspace::new(8, 8);
        assert_eq!(frame.bytes(), 8 * 8 * 4);
        assert_eq!(workspace.bytes(), 8 * 8 * 9 + 8 * 4);
        workspace.with_canvas(&mut frame, |canvas| {
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
            canvas.color.bounds = [0, 0, 2, 2];
            assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::MAX));
            canvas.color.bounds = [0, 0, 8, 8];
            assert_eq!(canvas.sample_depth(3, 3), Some(Fixed::ONE));
        });
    }
}
