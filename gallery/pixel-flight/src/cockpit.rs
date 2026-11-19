#![doc = " Resize-time metalwork layers and live flight instruments."]
use super::math::FixedMath as _;
use super::math::{PI, SQRT_2, TAU};
use super::{
    flight::{Flight, Projection},
    jet,
    raster::{BAYER, Canvas, Palette, blend, rgba},
    world::{World, hash8},
};
use alloc::{vec, vec::Vec};
use mirui::types::Fixed;
const GREEN: [u32; 6] = [
    0xff0d1403, 0xff162205, 0xff28420a, 0xff4e9616, 0xff7ee246, 0xffd2ffba,
];
const WHITE: u32 = 0xffff_ffff;
const BLACK: u32 = 0xff00_0000;
const HUD: u32 = 0xffa4_ff78;
const GLASS: u32 = 0xffff_e2aa;
const GLINT: u32 = 0xffff_f6e8;
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 2, 2],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

fn scaled_pixel(value: i32, scale: Fixed) -> i32 {
    let result = (Fixed::from_int(value) * scale).round().to_int();
    if result == 0 { value.signum() } else { result }
}
#[derive(Clone, Copy, Default)]
struct Box2 {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
#[derive(Clone, Copy, Default)]
struct Dial {
    x: i32,
    y: i32,
    r: i32,
}
#[doc = " Only procedural instrument artwork uses these small pixel primitives."]
trait Ink {
    fn clip(&self) -> [i32; 4];
    fn put(&mut self, x: i32, y: i32, color: u32);
    fn get(&self, x: i32, y: i32) -> u32;
    fn tint(&mut self, x: i32, y: i32, color: u32, alpha: Fixed) {
        let old = self.get(x, y);
        if old != 0 {
            self.put(x, y, blend(old, color, alpha));
        }
    }
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        let [left, top, right, bottom] = self.clip();
        for y in y0.max(top)..y1.min(bottom) {
            for x in x0.max(left)..x1.min(right) {
                self.put(x, y, color);
            }
        }
    }
    fn line(&mut self, a: [Fixed; 2], b: [Fixed; 2], color: u32) {
        let [left, top, right, bottom] = self.clip();
        if a[0].max(b[0]).ceil().to_int() < left
            || a[1].max(b[1]).ceil().to_int() < top
            || a[0].min(b[0]).floor().to_int() >= right
            || a[1].min(b[1]).floor().to_int() >= bottom
        {
            return;
        }
        let n = ((b[0] - a[0])
            .abs()
            .max((b[1] - a[1]).abs())
            .ceil()
            .max(Fixed::ONE))
        .trunc_to_int();
        for i in 0..=n {
            let t = Fixed::from_int(i) / Fixed::from_int(n);
            self.put(
                ((a[0] + (b[0] - a[0]) * t).round()).trunc_to_int(),
                ((a[1] + (b[1] - a[1]) * t).round()).trunc_to_int(),
                color,
            );
        }
    }
    fn disc(&mut self, x: i32, y: i32, r: i32, color: u32) {
        let [left, top, right, bottom] = self.clip();
        for dy in (-r).max(top - y)..=r.min(bottom - y - 1) {
            for dx in (-r).max(left - x)..=r.min(right - x - 1) {
                if dx * dx + dy * dy <= r * r {
                    self.put(x + dx, y + dy, color);
                }
            }
        }
    }
    fn plate(&mut self, r: Box2, colors: [u32; 3]) {
        self.rect(r.x, r.y, r.x + r.w, r.y + r.h, colors[0]);
        self.rect(r.x, r.y, r.x + r.w, r.y + 1, colors[1]);
        self.rect(r.x, r.y, r.x + 1, r.y + r.h, colors[1]);
        self.rect(r.x, r.y + r.h - 1, r.x + r.w, r.y + r.h, colors[2]);
        self.rect(r.x + r.w - 1, r.y, r.x + r.w, r.y + r.h, colors[2]);
    }
    fn well(&mut self, r: Box2, colors: [u32; 3]) {
        self.plate(r, [colors[0], colors[2], colors[1]]);
        self.rect(r.x, r.y, r.x + r.w, r.y + 2, colors[2]);
        self.rect(r.x, r.y, r.x + 2, r.y + r.h, colors[2]);
    }
    fn bolt(&mut self, x: i32, y: i32, lit: u32, dark: u32) {
        self.rect(x, y, x + 2, y + 2, dark);
        self.put(x, y, lit);
    }
    fn number(&mut self, x: i32, y: i32, value: u32, digits: usize, color: u32) {
        let [left, top, right, bottom] = self.clip();
        if x >= right || y >= bottom || x + digits as i32 * 4 - 1 <= left || y + 5 <= top {
            return;
        }
        let mut divisor = 10u32.pow((digits - 1) as u32);
        for i in 0..digits {
            let n = (value / divisor % 10) as usize;
            for (row, bits) in DIGITS[n].iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) != 0 {
                        self.put(x + i as i32 * 4 + col, y + row as i32, color);
                    }
                }
            }
            divisor = (divisor / 10).max(1);
        }
    }
}
struct Layer {
    pixels: LayerPixels,
    width: i32,
    height: i32,
    pad: i32,
}
const ROW_PIXELS: usize = 160;
enum LayerPixels {
    Rgba(Vec<u32>),
    Indexed {
        data: Vec<u8>,
        colors: [u32; 32],
        used: usize,
    },
    Encoded {
        data: Vec<u8>,
        rows: Vec<u32>,
        colors: [u32; 32],
    },
}
enum RowPixels<'a> {
    Rgba(&'a [u32]),
    Indexed(&'a [u8], &'a [u32; 32]),
    Empty,
}
#[derive(Clone, Copy)]
pub(super) struct PreparedCockpit {
    projection: Projection,
    offsets: [[Fixed; 3]; 7],
    light: Light,
    heading: [Fixed; 2],
}
struct RadarCell {
    bearing: Fixed,
    flags: u8,
}
impl RadarCell {
    const RING: u8 = 1;
    const INSIDE: u8 = 2;

    fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        let dx = x - width / 2;
        let dy = y - height / 2;
        let distance = Fixed::from_int(dx * dx + dy * dy).sqrt();
        let radius = Fixed::from_int(width.min(height)) * Fixed::from_ratio(47, 100);
        let ring = [
            Fixed::ONE,
            Fixed::from_ratio(66, 100),
            Fixed::from_ratio(33, 100),
        ]
        .into_iter()
        .any(|ring| (distance / radius - ring).abs() < Fixed::from_ratio(5, 10) / radius);
        let inside = distance <= radius;
        Self {
            bearing: if inside {
                Fixed::from_int(dx).atan2(Fixed::from_int(-dy))
            } else {
                Fixed::ZERO
            },
            flags: (u8::from(ring) * Self::RING) | (u8::from(inside) * Self::INSIDE),
        }
    }
}
impl RowPixels<'_> {
    fn color(&self, x: usize) -> u32 {
        match self {
            Self::Rgba(row) => row[x],
            Self::Indexed(row, colors) => colors[usize::from(row[x])],
            Self::Empty => 0,
        }
    }
    fn sample(&self, above: Option<&Self>, x: usize, fraction: Fixed) -> (u32, Fixed) {
        let mut color = self.color(x);
        let mut cover = Fixed::ONE;
        if let Some(above) = above {
            let above = above.color(x);
            if above != color {
                if above == 0 {
                    cover = Fixed::ONE - fraction;
                } else if color == 0 {
                    color = above;
                    cover = fraction;
                } else {
                    color = blend(color, above, fraction);
                }
            }
        }
        (color, cover)
    }
}
impl Layer {
    fn new(width: i32, height: i32, pad: i32, compact: bool) -> Self {
        Self {
            pixels: if compact {
                assert!((width + pad * 2) as usize <= ROW_PIXELS);
                LayerPixels::Indexed {
                    data: Vec::new(),
                    colors: [0; 32],
                    used: 1,
                }
            } else {
                LayerPixels::Rgba(Vec::new())
            },
            width: width + pad * 2,
            height: height + pad * 2,
            pad,
        }
    }
    fn mark_coverage(
        &self,
        clip: [i32; 4],
        offset: [Fixed; 3],
        mask: &mut [u8],
        row_scratch: &mut [u8; ROW_PIXELS],
        above_scratch: &mut [u8; ROW_PIXELS],
    ) -> usize {
        let [left, top, right, bottom] = clip;
        let width = (right - left) as usize;
        let dx = offset[0].trunc_to_int() - self.pad;
        let dy = offset[1].trunc_to_int() - self.pad;
        let fraction = offset[2];
        let mut marked = 0;
        for y in top.max(dy)..bottom.min(dy + self.height) {
            let sy = (y - dy) as usize;
            let row = self.row(sy, row_scratch);
            let above = (fraction > Fixed::from_ratio(4, 1000) && sy > 0)
                .then(|| self.row(sy - 1, above_scratch));
            if matches!(row, RowPixels::Empty)
                && above
                    .as_ref()
                    .is_none_or(|row| matches!(row, RowPixels::Empty))
            {
                continue;
            }
            for x in left.max(dx)..right.min(dx + self.width) {
                let at = (y - top) as usize * width + (x - left) as usize;
                if mask[at] != 0 {
                    continue;
                }
                let (color, cover) = row.sample(above.as_ref(), (x - dx) as usize, fraction);
                if color >> 24 == 255 && cover == Fixed::ONE {
                    mask[at] = 1;
                    marked += 1;
                }
            }
        }
        marked
    }
    fn bytes(&self) -> usize {
        match &self.pixels {
            LayerPixels::Rgba(data) => data.capacity() * 4,
            LayerPixels::Indexed { data, .. } => data.capacity(),
            LayerPixels::Encoded { data, rows, .. } => data.capacity() + rows.capacity() * 4,
        }
    }
    fn color_at(&self, at: usize) -> u32 {
        match &self.pixels {
            LayerPixels::Rgba(data) => data.get(at).copied().unwrap_or(0),
            LayerPixels::Indexed { data, colors, .. } => {
                data.get(at).map_or(0, |index| colors[usize::from(*index)])
            }
            LayerPixels::Encoded { .. } => {
                let mut scratch = [0; ROW_PIXELS];
                self.row(at / self.width as usize, &mut scratch)
                    .color(at % self.width as usize)
            }
        }
    }
    fn set_color_at(&mut self, at: usize, color: u32) {
        let count = (self.width * self.height) as usize;
        match &mut self.pixels {
            LayerPixels::Rgba(data) => {
                if data.is_empty() {
                    data.resize(count, 0);
                }
                data[at] = color;
            }
            LayerPixels::Indexed { data, colors, used } => {
                let index = colors[..*used]
                    .iter()
                    .position(|value| *value == color)
                    .unwrap_or_else(|| {
                        assert!(*used < colors.len(), "procedural cockpit palette overflow");
                        let index = *used;
                        colors[index] = color;
                        *used += 1;
                        index
                    });
                if data.is_empty() {
                    data.resize(count, 0);
                }
                data[at] = index as u8;
            }
            LayerPixels::Encoded { .. } => panic!("completed cockpit layer is immutable"),
        }
    }
    fn seal(&mut self) {
        let LayerPixels::Indexed { data, colors, .. } = &self.pixels else {
            return;
        };
        if data.is_empty() {
            return;
        }
        let codec = mirx::coding::Rle::new();
        let width = self.width as usize;
        let length = data
            .chunks_exact(width)
            .map(|row| {
                if row.iter().all(|v| *v == 0) {
                    0
                } else {
                    codec.encoded_len(row).expect("cockpit row size")
                }
            })
            .sum();
        let mut encoded = vec![0; length];
        let mut rows = Vec::with_capacity(self.height as usize + 1);
        let mut offset = 0;
        rows.push(0);
        for row in data.chunks_exact(width) {
            if row.iter().any(|v| *v != 0) {
                offset += codec
                    .encode_into(row, &mut encoded[offset..])
                    .expect("cockpit row encoding");
            }
            rows.push(u32::try_from(offset).expect("bounded cockpit layer"));
        }
        self.pixels = LayerPixels::Encoded {
            data: encoded,
            rows,
            colors: *colors,
        };
    }
    fn row<'a>(&'a self, y: usize, scratch: &'a mut [u8; ROW_PIXELS]) -> RowPixels<'a> {
        let width = self.width as usize;
        if let LayerPixels::Encoded { data, rows, .. } = &self.pixels {
            let encoded = &data[rows[y] as usize..rows[y + 1] as usize];
            if !encoded.is_empty() {
                mirx::coding::Rle::new()
                    .plan(encoded, width)
                    .expect("prepared cockpit row")
                    .decode_into(&mut scratch[..width])
                    .expect("cockpit row workspace");
            }
        }
        self.row_view(y, scratch)
    }
    // Encoded scratch must already contain this row; transparent rows do not read it.
    fn row_view<'a>(&'a self, y: usize, scratch: &'a [u8; ROW_PIXELS]) -> RowPixels<'a> {
        let width = self.width as usize;
        match &self.pixels {
            LayerPixels::Rgba(data) if !data.is_empty() => {
                RowPixels::Rgba(&data[y * width..(y + 1) * width])
            }
            LayerPixels::Indexed { data, colors, .. } if !data.is_empty() => {
                RowPixels::Indexed(&data[y * width..(y + 1) * width], colors)
            }
            LayerPixels::Encoded { rows, colors, .. } if rows[y] != rows[y + 1] => {
                RowPixels::Indexed(&scratch[..width], colors)
            }
            _ => RowPixels::Empty,
        }
    }
}
impl Ink for Layer {
    fn clip(&self) -> [i32; 4] {
        [
            -self.pad,
            -self.pad,
            self.width - self.pad,
            self.height - self.pad,
        ]
    }
    fn put(&mut self, x: i32, y: i32, color: u32) {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.set_color_at((y * self.width + x) as usize, color);
        }
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.color_at((y * self.width + x) as usize)
        } else {
            0
        }
    }
}
struct Live<'a, 'b> {
    canvas: &'a mut Canvas<'b>,
    offset: [i32; 2],
}
impl Ink for Live<'_, '_> {
    fn clip(&self) -> [i32; 4] {
        let [left, top, right, bottom] = self.canvas.clip;
        [
            left - self.offset[0],
            top - self.offset[1],
            right - self.offset[0],
            bottom - self.offset[1],
        ]
    }
    fn put(&mut self, x: i32, y: i32, color: u32) {
        self.canvas
            .point(x + self.offset[0], y + self.offset[1], color);
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.offset[0];
        let y = y + self.offset[1];
        if self.canvas.contains(x, y) {
            self.canvas.get(self.canvas.index(x, y))
        } else {
            0
        }
    }
}
pub struct Cockpit {
    width: i32,
    height: i32,
    scale: Fixed,
    pad: i32,
    layers: [Layer; 4],
    body: [u32; 9],
    hot: [u32; 6],
    panel_top: i32,
    deck_top: i32,
    rear_bottom: i32,
    screens: [Box2; 2],
    ball: Dial,
    dials: [Dial; 4],
    lamps: [Box2; 5],
    panel_keys: Vec<Box2>,
    deck_keys: Vec<Box2>,
    radar: Vec<RadarCell>,
}
impl Cockpit {
    pub fn bytes(&self) -> usize {
        self.layers.iter().map(Layer::bytes).sum::<usize>()
            + (self.panel_keys.capacity() + self.deck_keys.capacity())
                * core::mem::size_of::<Box2>()
            + self.radar.capacity() * core::mem::size_of::<RadarCell>()
            + core::mem::size_of::<Self>()
    }
    pub fn new(width: usize, height: usize, palette: &Palette) -> Self {
        Self::with_storage(width, height, palette, false)
    }
    pub(super) fn new_compact(width: usize, height: usize, palette: &Palette) -> Self {
        assert!(width <= 128 && height <= 128);
        Self::with_storage(width, height, palette, true)
    }
    fn with_storage(width: usize, height: usize, palette: &Palette, compact: bool) -> Self {
        let w = width as i32;
        let h = height as i32;
        let s = Fixed::from_int((height) as i32) / Fixed::from_int(300);
        let cx = w / 2;
        let r = |v: i32| scaled_pixel(v, s);
        let x = |u: i32| cx + r(u);
        let pad = ((Fixed::from_int(14) * s).ceil()).trunc_to_int() + 3;
        let b = core::array::from_fn(|i| palette.jets[0][i + 1]);
        let a = core::array::from_fn(|i| palette.jets[0][i + 17]);
        let mut art = Self {
            width: w,
            height: h,
            scale: s,
            pad,
            layers: core::array::from_fn(|_| Layer::new(w, h, pad, compact)),
            body: b,
            hot: a,
            panel_top: h,
            deck_top: h,
            rear_bottom: 0,
            screens: [Box2::default(); 2],
            ball: Dial {
                x: cx,
                y: r(228),
                r: r(25),
            },
            dials: [Dial::default(); 4],
            lamps: [Box2::default(); 5],
            panel_keys: Vec::with_capacity(8),
            deck_keys: Vec::with_capacity(8),
            radar: Vec::new(),
        };
        let inside = |dx: Fixed, dy: Fixed, ax: Fixed, ay: Fixed| {
            (dx.abs() / ax).pow_fraction(Fixed::from_ratio(25, 10))
                + (dy.abs() / ay).pow_fraction(Fixed::from_ratio(25, 10))
        };
        for yy in -pad..h + pad {
            let dy = Fixed::from_int(252) * s - Fixed::from_int(yy);
            if dy <= Fixed::ZERO {
                continue;
            }
            for xx in -pad..w + pad {
                let v = inside(
                    Fixed::from_int(xx - cx),
                    dy,
                    Fixed::from_int(280) * s,
                    Fixed::from_int(276) * s,
                );
                if v <= Fixed::ONE {
                    continue;
                }
                let f = (v.pow_fraction(Fixed::from_ratio(4, 10)) - Fixed::ONE)
                    * Fixed::from_int(280)
                    * s;
                let n = hash8((xx * 71 + yy * 409) as u32);
                let color = if f < Fixed::from_ratio(12, 10) {
                    b[5]
                } else if f < Fixed::from_ratio(24, 10) + s {
                    b[3]
                } else if f < Fixed::from_ratio(24, 10) + Fixed::from_int(6) * s {
                    if n < 20 { b[1] } else { b[2] }
                } else if n > 248 {
                    b[2]
                } else {
                    b[1]
                };
                art.layers[3].put(xx, yy, color);
                art.rear_bottom = art.rear_bottom.max(yy);
            }
        }
        for i in 0..40 {
            let angle = Fixed::from_int(i) / Fixed::from_int(40) * PI;
            let (sn, cs) = angle.sin_cos();
            let m = Fixed::ONE + Fixed::from_int(9) / Fixed::from_int(280);
            let xx = ((Fixed::from_int(cx)
                + cs.signum()
                    * cs.abs().pow_fraction(Fixed::from_ratio(8, 10))
                    * Fixed::from_int(280)
                    * s
                    * m)
                .round())
            .trunc_to_int();
            let yy = ((Fixed::from_int(252) * s
                - sn.abs().pow_fraction(Fixed::from_ratio(8, 10)) * Fixed::from_int(276) * s * m)
                .round())
            .trunc_to_int();
            if art.layers[3].get(xx, yy) != 0 {
                art.layers[3].bolt(xx, yy, b[5], b[0]);
            }
        }
        art.layers[3].seal();
        let bar = (Fixed::from_int(13) * s).max(Fixed::from_int(6));
        let rx = Fixed::from_int(226) * s;
        let ry = Fixed::from_int(228) * s;
        let oy = Fixed::from_int(236) * s;
        for yy in -pad..h + pad {
            let dy = oy - Fixed::from_int(yy);
            if dy <= Fixed::ZERO {
                continue;
            }
            for xx in -pad..w + pad {
                let dx = Fixed::from_int(xx - cx);
                let inner = inside(dx, dy, rx, ry);
                if inner <= Fixed::ONE || inside(dx, dy, rx + bar, ry + bar) > Fixed::ONE {
                    continue;
                }
                let f = ((inner.pow_fraction(Fixed::from_ratio(4, 10)) - Fixed::ONE)
                    / ((rx + bar) / rx - Fixed::ONE))
                    .clamp(Fixed::ZERO, Fixed::ONE);
                let part = (dy / ry).atan2(dx / rx) / PI * Fixed::from_int(15);
                let section = (part.floor()).trunc_to_int();
                let along = part.fract();
                let n = hash8((xx * 131 + yy * 977) as u32);
                let seam =
                    !(Fixed::from_ratio(35, 1000)..=Fixed::from_ratio(965, 1000)).contains(&along);
                let mut color = if seam {
                    b[0]
                } else if f < Fixed::from_ratio(8, 100) {
                    b[2]
                } else if f < Fixed::from_ratio(2, 10) {
                    b[6]
                } else if f < Fixed::from_ratio(34, 100) {
                    b[5]
                } else if f < Fixed::from_ratio(66, 100) {
                    if n < 14 { b[3] } else { b[4] }
                } else if f < Fixed::from_ratio(86, 100) {
                    b[2]
                } else {
                    b[0]
                };
                if !seam
                    && [1, 4, 7, 10, 13].contains(&section)
                    && f > Fixed::from_ratio(14, 100)
                    && f < Fixed::from_ratio(86, 100)
                {
                    let edge = along.min(Fixed::ONE - along) * Fixed::from_int(9)
                        + (f - Fixed::from_ratio(14, 100)).min(Fixed::from_ratio(86, 100) - f)
                            * Fixed::from_int(5);
                    if Fixed::from_int((n) as i32)
                        > Fixed::from_int(70) - edge * Fixed::from_int(150)
                    {
                        color = if f < Fixed::from_ratio(3, 10) {
                            a[5]
                        } else if f < Fixed::from_ratio(42, 100) {
                            a[4]
                        } else if f < Fixed::from_ratio(68, 100) {
                            a[3]
                        } else {
                            a[2]
                        };
                    }
                } else if !seam
                    && f > Fixed::from_ratio(24, 100)
                    && f < Fixed::from_ratio(8, 10)
                    && n > 249
                {
                    color = a[2];
                }
                art.layers[0].put(xx, yy, color);
            }
        }
        for i in 1..15 {
            for off in [-Fixed::from_ratio(9, 100), Fixed::from_ratio(9, 100)] {
                let angle = (Fixed::from_int(i) + off) / Fixed::from_int(15) * PI;
                let m = Fixed::ONE + bar / (Fixed::from_int(2) * rx);
                let cosine = angle.cos();
                let xx = ((Fixed::from_int(cx)
                    + cosine.signum()
                        * cosine.abs().pow_fraction(Fixed::from_ratio(8, 10))
                        * rx
                        * m)
                    .round())
                .trunc_to_int();
                let yy = ((oy - angle.sin().abs().pow_fraction(Fixed::from_ratio(8, 10)) * ry * m)
                    .round())
                .trunc_to_int();
                if art.layers[0].get(xx, yy) != 0 {
                    art.layers[0].bolt(xx - 1, yy - 1, b[7], b[0]);
                }
            }
        }
        let ty = ((oy - ry - bar).round()).trunc_to_int();
        for yy in -pad..r(21) {
            let inset = ((Fixed::from_int(yy) * Fixed::from_ratio(45, 100))
                .max(Fixed::ZERO)
                .round())
            .trunc_to_int();
            art.layers[0].rect(
                cx - r(26) + inset,
                ty + yy,
                cx + r(26) - inset,
                ty + yy + 1,
                if yy < 2 {
                    b[5]
                } else if yy > r(21) - 3 {
                    b[1]
                } else {
                    b[3]
                },
            );
        }
        art.layers[0].rect(x(-14), ty + r(7), x(14), ty + r(13), b[0]);
        art.layers[0].rect(x(-12), ty + r(8), x(12), ty + r(12), a[4]);
        for side in [-1, 1] {
            let ly = r(118);
            let half = rx
                * (Fixed::ONE
                    - ((oy - Fixed::from_int(ly)) / ry).pow_fraction(Fixed::from_ratio(25, 10)))
                .max(Fixed::ZERO)
                .pow_fraction(Fixed::from_ratio(4, 10));
            let lx = ((Fixed::from_int(cx)
                + Fixed::from_int(side) * (half + bar * Fixed::from_ratio(35, 100)))
            .round())
            .trunc_to_int();
            art.layers[0].rect(lx - r(5), ly - r(9), lx + r(5), ly + r(9), b[0]);
            art.layers[0].rect(lx - r(4), ly - r(8), lx + r(4), ly + r(8), a[3]);
            art.layers[0].bolt(lx - 1, ly - 1, b[7], b[0]);
            for (u0, v0, u1, v1, wide) in [
                (-152_i32, 66, -124, 38, 3),
                (-141, 72, -127, 58, 2),
                (-190, 40, -170, 20, 3),
                (-197, 54, -184, 41, 2),
                (-178, 44, -172, 38, 1),
            ] {
                let n = r((u1 - u0).abs()).max(1);
                for i in 0..=n {
                    for k in 0..r(wide).max(1) {
                        let xx = cx
                            + ((Fixed::from_int(side)
                                * (Fixed::from_int(u0)
                                    + Fixed::from_int(u1 - u0) * Fixed::from_int(i)
                                        / Fixed::from_int(n))
                                * s)
                                .round())
                            .trunc_to_int()
                            + k * side;
                        let yy = (((Fixed::from_int(v0)
                            + Fixed::from_int(v1 - v0) * Fixed::from_int(i) / Fixed::from_int(n))
                            * s)
                            .round())
                        .trunc_to_int()
                            + k;
                        if art.layers[0].get(xx, yy) == 0
                            && ((k == 0 && i.min(n - i) >= 2) || (xx + yy) & 1 == 0)
                        {
                            art.layers[0].put(xx, yy, GLINT);
                        }
                    }
                }
            }
        }
        art.layers[0].seal();
        let top_of = |u: Fixed| {
            let u = u.abs();
            if u <= Fixed::from_int(58) {
                Fixed::from_int(168)
            } else if u <= Fixed::from_int(74) {
                Fixed::from_int(168)
                    + (u - Fixed::from_int(58)) / Fixed::from_int(16) * Fixed::from_int(11)
            } else if u <= Fixed::from_int(92) {
                Fixed::from_int(179)
            } else if u <= Fixed::from_int(104) {
                Fixed::from_int(179)
                    - (u - Fixed::from_int(92)) / Fixed::from_int(12) * Fixed::from_int(9)
            } else if u <= Fixed::from_int(206) {
                Fixed::from_int(170) + (u - Fixed::from_int(104)) * Fixed::from_ratio(2, 100)
            } else {
                Fixed::from_int(172)
                    + (u - Fixed::from_int(206)) / Fixed::from_int(16) * Fixed::from_int(10)
            }
        };
        let g = r(11).max(r(5).max(1));
        for xx in -pad..w + pad {
            let u = Fixed::from_int(xx - cx) / s;
            if u.abs() > Fixed::from_int(232) {
                continue;
            }
            let top = ((top_of(u) * s).round()).trunc_to_int();
            art.panel_top = art.panel_top.min(top);
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 53 + yy * 311) as u32);
                let mut color = if d < g {
                    let near = Fixed::from_int(d) / Fixed::from_int(g);
                    let ray = u.abs() / (Fixed::from_int(15) + near * Fixed::from_int(9));
                    if d == 0 {
                        b[6]
                    } else if d == 1 {
                        b[4]
                    } else if ray.fract() < Fixed::from_ratio(7, 100) {
                        b[0]
                    } else if near < Fixed::from_ratio(45, 100) {
                        if n > 236 { b[3] } else { b[2] }
                    } else {
                        b[1]
                    }
                } else if d < g + 2 {
                    if d == g { b[5] } else { b[3] }
                } else if d < g + 4 {
                    b[0]
                } else {
                    let below = d - g - 4;
                    let v = Fixed::from_int(yy) / s;
                    if below < r(4)
                        || (below < r(8) && (xx + yy) & 1 == 0)
                        || v > Fixed::from_int(262)
                    {
                        b[0]
                    } else if v > Fixed::from_int(250) {
                        if (xx + yy) & 1 == 0 { b[0] } else { b[1] }
                    } else if v > Fixed::from_int(222) || (xx + yy) & 3 == 0 {
                        b[1]
                    } else {
                        b[2]
                    }
                };
                if d > 0 && d < g / 2 && hash8(((xx >> 3) * 7 + 5) as u32) > 208 && n > 120 {
                    color = if d < 3 { a[3] } else { a[2] };
                }
                art.layers[1].put(xx, yy, color);
            }
        }
        for side in [-1, 1] {
            for i in 0..5 {
                art.layers[1].rect(
                    x(side * (150 + i * 5)),
                    r(173),
                    x(side * (150 + i * 5)) + r(2).max(1),
                    r(179),
                    b[0],
                );
            }
        }
        art.layers[1].well(
            Box2 {
                x: x(-34),
                y: r(183),
                w: r(68),
                h: r(8),
            },
            [b[0], b[4], b[0]],
        );
        for i in 0..5 {
            let lamp = Box2 {
                x: cx
                    + (((-Fixed::from_int(30)
                        + Fixed::from_int((i) as i32) * Fixed::from_ratio(124, 10))
                        * s)
                        .round())
                    .trunc_to_int(),
                y: r(185),
                w: r(10).max(r(4).max(1)),
                h: r(5).max(r(3).max(1)),
            };
            art.lamps[i] = lamp;
            art.layers[1].rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[1]);
        }
        for (i, u) in [-206, 106].into_iter().enumerate() {
            let x0 = x(u);
            let x1 = x(u + 100);
            let y0 = r(193);
            let y1 = r(261);
            let panel = &mut art.layers[1];
            panel.rect(x0 + 2, y0 + 2, x1 + 3, y1 + 3, b[0]);
            panel.plate(
                Box2 {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                },
                [b[3], b[6], b[1]],
            );
            let margin = r(7).max(r(4).max(1));
            let screen = Box2 {
                x: x0 + margin,
                y: y0 + margin - 1,
                w: x1 - x0 - margin * 2,
                h: y1 - y0 - margin * 2 - r(3),
            };
            panel.well(
                Box2 {
                    x: screen.x - 3,
                    y: screen.y - 3,
                    w: screen.w + 6,
                    h: screen.h + 6,
                },
                [b[1], b[5], b[0]],
            );
            panel.rect(
                screen.x,
                screen.y,
                screen.x + screen.w,
                screen.y + screen.h,
                GREEN[0],
            );
            art.screens[i] = screen;
            for (bx, by) in [
                (x0 + 3, y0 + 3),
                (x1 - 5, y0 + 3),
                (x0 + 3, y1 - 5),
                (x1 - 5, y1 - 5),
            ] {
                panel.bolt(bx, by, b[7], b[0]);
            }
            for k in 0..6 {
                let key = Box2 {
                    x: screen.x
                        + r(4)
                        + k * ((Fixed::from_int(screen.w) / Fixed::from_ratio(64, 10)).round())
                            .trunc_to_int(),
                    y: y1 - r(7),
                    w: r(7).max(r(3).max(1)),
                    h: r(3).max(r(2).max(1)),
                };
                panel.plate(key, [b[5], b[7], b[0]]);
                if k == 1 || k == 4 {
                    art.panel_keys.push(key);
                }
            }
        }
        art.layers[1].plate(
            Box2 {
                x: x(-100),
                y: r(193),
                w: r(200),
                h: r(69),
            },
            [b[2], b[5], b[0]],
        );
        let ball = art.ball;
        let outer = ball.r + r(8).max(r(4).max(1));
        for dy in -outer - 3..=outer + 3 {
            for dx in -outer - 3..=outer + 3 {
                let d = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                if d > Fixed::from_int((outer + 3) as i32) {
                    continue;
                }
                let color = if d > Fixed::from_int((outer) as i32) + Fixed::ONE {
                    if (Fixed::from_int(dx + dy))
                        < (Fixed::from_int((outer) as i32) * Fixed::from_ratio(4, 10))
                    {
                        continue;
                    }
                    b[0]
                } else if d > Fixed::from_int((outer) as i32) {
                    b[0]
                } else if d > Fixed::from_int((ball.r) as i32) + Fixed::from_int(2) {
                    let light =
                        Fixed::from_int(-(dx + dy)) / (d * SQRT_2 + Fixed::from_ratio(1, 1000));
                    if light > Fixed::from_ratio(55, 100) {
                        b[7]
                    } else if light > Fixed::from_ratio(1, 10) {
                        b[5]
                    } else if light > -Fixed::from_ratio(45, 100) {
                        b[4]
                    } else {
                        b[2]
                    }
                } else {
                    b[0]
                };
                art.layers[1].put(ball.x + dx, ball.y + dy, color);
            }
        }
        let brass = [
            rgba(
                Fixed::from_int(120),
                Fixed::from_int(86),
                Fixed::from_int(26),
            ),
            rgba(
                Fixed::from_int(206),
                Fixed::from_int(158),
                Fixed::from_int(54),
            ),
            rgba(
                Fixed::from_int(255),
                Fixed::from_int(222),
                Fixed::from_int(120),
            ),
        ];
        let rb = ball.r + r(5).max(r(3).max(1));
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                let xx = ((Fixed::from_int((ball.x) as i32)
                    + Fixed::from_int(sx) * Fixed::from_int((rb) as i32) / SQRT_2)
                    .round())
                .trunc_to_int() as i32;
                let yy = ((Fixed::from_int((ball.y) as i32)
                    + Fixed::from_int(sy) * Fixed::from_int((rb) as i32) / SQRT_2)
                    .round())
                .trunc_to_int() as i32;
                art.layers[1].put(xx, yy, brass[2]);
                art.layers[1].put(xx + 1, yy, brass[1]);
                art.layers[1].put(xx, yy + 1, brass[1]);
                art.layers[1].put(xx + 1, yy + 1, brass[0]);
            }
        }
        let ph = r(5).max(r(3).max(1));
        for i in 0..ph {
            art.layers[1].rect(
                ball.x - i,
                ball.y - outer + ph - i,
                ball.x + i + 1,
                ball.y - outer + ph - i + 1,
                if i == 0 { brass[2] } else { brass[1] },
            );
        }
        let knob = &mut art.layers[1];
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(2),
            ball.x + r(6) + 2,
            ball.y + outer + r(4),
            b[0],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(3),
            ball.x + r(6) + 1,
            ball.y + outer + r(3),
            brass[1],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(3),
            ball.x + r(6) + 1,
            ball.y + outer - r(2),
            brass[2],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer + r(2),
            ball.x + r(6) + 1,
            ball.y + outer + r(3),
            brass[0],
        );
        for i in (-r(5)..=r(5)).step_by(2) {
            knob.put(ball.x + i, ball.y + outer, brass[0]);
        }
        for (i, (u, v)) in [(-72, 211), (-72, 246), (72, 211), (72, 246)]
            .into_iter()
            .enumerate()
        {
            let dial = Dial {
                x: x(u),
                y: r(v),
                r: r(12).max(r(6).max(1)),
            };
            art.dials[i] = dial;
            for dy in -dial.r - 3..=dial.r + 4 {
                for dx in -dial.r - 3..=dial.r + 4 {
                    let d = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                    if d > Fixed::from_int(dial.r) + Fixed::from_ratio(35, 10) {
                        continue;
                    }
                    let color = if d > Fixed::from_int(dial.r + 2) {
                        if (Fixed::from_int(dx + dy))
                            < (Fixed::from_int(dial.r) * Fixed::from_ratio(5, 10))
                        {
                            continue;
                        }
                        b[0]
                    } else if d > Fixed::from_int(dial.r) {
                        if Fixed::from_int(-(dx + dy))
                            > Fixed::from_int(dial.r) * Fixed::from_ratio(4, 10)
                        {
                            b[7]
                        } else if Fixed::from_int(-(dx + dy))
                            > Fixed::from_int(-dial.r) * Fixed::from_ratio(5, 10)
                        {
                            b[5]
                        } else {
                            b[2]
                        }
                    } else if d > Fixed::from_int(dial.r) - Fixed::from_ratio(15, 10)
                        || (d > Fixed::from_int(dial.r - 4)
                            && Fixed::from_int(-(dx + dy))
                                > Fixed::from_int(dial.r) * Fixed::from_ratio(3, 10))
                    {
                        b[0]
                    } else {
                        b[1]
                    };
                    art.layers[1].put(dial.x + dx, dial.y + dy, color);
                }
            }
            for mark in 0..10 {
                let angle = Fixed::from_int(-225 + mark * 30) * PI / Fixed::from_int(180);
                art.layers[1].put(
                    dial.x + ((angle.cos() * Fixed::from_int(dial.r - 3)).round()).trunc_to_int(),
                    dial.y + ((angle.sin() * Fixed::from_int(dial.r - 3)).round()).trunc_to_int(),
                    if mark > 7 { a[4] } else { b[7] },
                );
            }
        }
        for side in [-1, 1] {
            for j in 0..2 {
                for i in 0..2 {
                    let key = Box2 {
                        x: x(side * 72) - r(8) + i * r(9),
                        y: r(224) + j * r(6),
                        w: r(7).max(r(4).max(1)),
                        h: r(5).max(r(3).max(1)),
                    };
                    let lit = (i + j) % 2 == 0;
                    art.layers[1].plate(
                        key,
                        [
                            if lit { a[3] } else { b[4] },
                            if lit { a[5] } else { b[6] },
                            b[0],
                        ],
                    );
                    if lit && j == 0 {
                        art.panel_keys.push(key);
                    }
                }
            }
        }
        for xx in -pad..w + pad {
            let u = Fixed::from_int(xx - cx) / s;
            let abs = u.abs();
            let top = ((if abs <= Fixed::from_int(212) {
                Fixed::from_int(263)
            } else if abs <= Fixed::from_int(222) {
                Fixed::from_int(263)
                    - (abs - Fixed::from_int(212)) / Fixed::from_int(10) * Fixed::from_int(77)
            } else {
                (Fixed::from_int(186) - (abs - Fixed::from_int(222)) * Fixed::from_ratio(42, 100))
                    .max(Fixed::from_int(110))
            } * s)
                .round())
            .trunc_to_int();
            if abs <= Fixed::from_int(212) {
                art.deck_top = art.deck_top.min(top);
            }
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 59 + yy * 211) as u32);
                let v = Fixed::from_int(yy) / s;
                let color = if abs > Fixed::from_int(212) {
                    let ray = (v - Fixed::from_int(112)) / abs.max(Fixed::ONE) * Fixed::from_int(7);
                    let rib = ray.fract() < Fixed::from_ratio(6, 100);
                    if d == 0 {
                        b[6]
                    } else if d < r(3) {
                        b[4]
                    } else if d < r(5) {
                        b[2]
                    } else if d < r(7) {
                        b[0]
                    } else if abs < Fixed::from_int(222) {
                        if abs < Fixed::from_int(215) {
                            b[5]
                        } else if abs < Fixed::from_int(218) {
                            b[3]
                        } else {
                            b[1]
                        }
                    } else if rib {
                        b[0]
                    } else if (ray.floor()).trunc_to_int() % 2 == 0 {
                        if n > 250 { b[3] } else { b[2] }
                    } else {
                        b[1]
                    }
                } else if d == 0 {
                    b[6]
                } else if d == 1 {
                    b[4]
                } else if v > Fixed::from_int(290) {
                    if (xx + yy) & 1 == 0 { b[1] } else { b[2] }
                } else if n > 250 {
                    b[4]
                } else {
                    b[3]
                };
                art.layers[2].put(xx, yy, color);
            }
        }
        let key_colors = [
            a[3],
            a[3],
            b[4],
            rgba(
                Fixed::from_int(255),
                Fixed::from_int(206),
                Fixed::from_int(84),
            ),
            rgba(
                Fixed::from_int(255),
                Fixed::from_int(206),
                Fixed::from_int(84),
            ),
            b[4],
            b[4],
            a[3],
        ];
        for j in 0..3 {
            for i in 0..3 {
                for side in [-1, 1] {
                    let key = Box2 {
                        x: x(if side < 0 {
                            -209 + i * 12
                        } else {
                            180 + i * 12
                        }),
                        y: r(269 + j * 9),
                        w: r(10).max(r(5).max(1)),
                        h: r(6).max(r(3).max(1)),
                    };
                    let color = key_colors[((if side < 0 {
                        i + j * 3
                    } else {
                        i * 2 + j * 3 + 2
                    }) % 8) as usize];
                    art.layers[2]
                        .plate(key, [color, if color == b[4] { b[6] } else { a[5] }, b[0]]);
                    if (side < 0 && i == 0) || (side > 0 && j == 1) {
                        art.deck_keys.push(key);
                    }
                }
            }
        }
        art.layers[2].well(
            Box2 {
                x: x(-173),
                y: r(267),
                w: r(64),
                h: h + pad - r(267),
            },
            [b[1], b[5], b[0]],
        );
        for u in [-158, -141, -124] {
            art.layers[2].rect(x(u), r(270), x(u) + r(3).max(r(2).max(1)), h + pad, b[0]);
            for yy in (r(272)..h + pad).step_by(r(4).max(r(2).max(1)) as usize) {
                art.layers[2].put(x(u) + r(3).max(r(2).max(1)) + 1, yy, b[5]);
            }
        }
        for i in 0..4 {
            let tx = x(112 + i * 11);
            let up = i % 2 == 0;
            art.layers[2].rect(tx - 1, r(281), tx + r(6).max(r(4).max(1)) + 1, r(288), b[0]);
            art.layers[2].rect(tx, r(282), tx + r(5).max(r(3).max(1)), r(286), b[5]);
            art.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 284 }),
                tx + r(3).max(r(2).max(1)),
                r(if up { 283 } else { 293 }),
                b[7],
            );
            art.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 290 }),
                tx + r(3).max(r(2).max(1)),
                r(if up { 275 } else { 293 }),
                WHITE,
            );
        }
        for (u, v, radius, color) in [
            (167, 282, 8, b[0]),
            (166, 280, 8, b[1]),
            (166, 280, 7, b[5]),
            (166, 279, 5, a[3]),
            (165, 278, 2, a[5]),
        ] {
            art.layers[2].disc(x(u), r(v), r(radius).max(1), color);
        }
        for dy in -r(10)..=r(10) {
            for dx in -r(32)..=r(32) {
                let d = Fixed::from_int(dx) * Fixed::from_int(dx)
                    / (Fixed::from_int(r(32).max(1))).powi(2)
                    + Fixed::from_int(dy) * Fixed::from_int(dy)
                        / (Fixed::from_int(r(10).max(1))).powi(2);
                if d <= Fixed::ONE {
                    art.layers[2].put(
                        cx + dx,
                        r(296) + dy,
                        if d > Fixed::from_ratio(8, 10) {
                            if dy < 0 { b[1] } else { b[5] }
                        } else if d > Fixed::from_ratio(62, 100) {
                            b[0]
                        } else {
                            b[1]
                        },
                    );
                }
            }
        }
        for side in [-1, 1] {
            for j in 0..4 {
                let key = Box2 {
                    x: x(side * 233) - if side < 0 { r(8) } else { 0 },
                    y: r(212 + j * 15),
                    w: r(8).max(r(4).max(1)),
                    h: r(6).max(r(3).max(1)),
                };
                art.layers[2].plate(
                    key,
                    [
                        if j == 1 {
                            a[3]
                        } else if j == 2 {
                            key_colors[3]
                        } else {
                            b[4]
                        },
                        b[6],
                        b[0],
                    ],
                );
            }
        }
        for (layer, top, salt) in [
            (1, art.panel_top + g + r(10), 3),
            (2, art.deck_top + r(4), 11),
        ] {
            let pixels = &mut art.layers[layer];
            let mut worn = Vec::new();
            let plate = |c| [b[1], b[2], b[3]].contains(&c);
            let edge = |c| [b[0], b[4], b[5], b[6]].contains(&c);
            for yy in top + pad..pixels.height - 1 {
                for xx in 1..pixels.width - 1 {
                    let at = (yy * pixels.width + xx) as usize;
                    if !plate(pixels.color_at(at))
                        || hash8(((xx >> 3) * 31 + (yy >> 2) * 17 + salt) as u32) < 234
                    {
                        continue;
                    }
                    if [
                        at - 1,
                        at + 1,
                        at - pixels.width as usize,
                        at + pixels.width as usize,
                    ]
                    .into_iter()
                    .any(|i| edge(pixels.color_at(i)))
                        && hash8((xx * 7 + yy * 13) as u32) > 96
                    {
                        worn.push(at);
                    }
                }
            }
            for at in worn {
                let n = hash8(at as u32);
                pixels.set_color_at(
                    at,
                    if n > 170 {
                        a[3]
                    } else if n > 70 {
                        a[2]
                    } else {
                        a[1]
                    },
                );
            }
            pixels.seal();
        }
        let radar = art.screens[0];
        if radar.w > 0 && radar.h > 0 {
            art.radar = Vec::with_capacity(radar.w as usize * radar.h as usize);
            for y in 0..radar.h {
                for x in 0..radar.w {
                    art.radar.push(RadarCell::new(x, y, radar.w, radar.h));
                }
            }
        }
        art
    }
    pub(super) fn prepare(&self, flight: &Flight, palette: &Palette) -> Option<PreparedCockpit> {
        let p = flight.projection(self.width as usize, self.height as usize);
        let amount = p.cockpit_in();
        if amount <= Fixed::ZERO {
            return None;
        }
        let s = self.scale;
        let r = |v: i32| scaled_pixel(v, s);
        let cx = self.width / 2;
        let j = &flight.jets[0];
        let heading = [j.yaw.cos(), j.yaw.sin()];
        let head = [
            flight.head[0].position * s,
            (flight.head[1].position
                + if flight.boost > Fixed::from_ratio(2, 10) {
                    Fixed::from_int((flight.time * Fixed::from_int(30)).trunc_to_int() & 1)
                        * Fixed::from_ratio(15, 10)
                        * flight.boost
                        - Fixed::from_ratio(75, 100) * flight.boost
                } else {
                    Fixed::ZERO
                })
                * s,
        ];
        let away = Fixed::ONE - amount;
        let deck_slide =
            away * Fixed::from_int(self.height - self.deck_top + 8) * Fixed::from_ratio(15, 10);
        let offset = |near: Fixed, slide: Fixed| {
            let whole = slide.floor();
            [
                -(head[0] * near)
                    .round()
                    .clamp(Fixed::from_int(-self.pad), Fixed::from_int(self.pad)),
                -(head[1] * near)
                    .round()
                    .clamp(Fixed::from_int(-self.pad), Fixed::from_int(self.pad))
                    + whole,
                slide - whole,
            ]
        };
        let canopy = offset(
            Fixed::from_ratio(3, 10),
            -away * Fixed::from_int(self.height) * Fixed::from_ratio(62, 100),
        );
        let glass = offset(
            Fixed::from_ratio(5, 10),
            away * Fixed::from_int(self.height - self.panel_top + 8),
        );
        let panel = offset(
            Fixed::from_ratio(72, 100),
            away * Fixed::from_int(self.height - self.panel_top + 8),
        );
        let deck = offset(Fixed::ONE, deck_slide);
        let lever = offset(Fixed::from_ratio(112, 100), deck_slide);
        let stick = offset(Fixed::from_ratio(132, 100), deck_slide);
        let rear = offset(
            Fixed::from_ratio(165, 100),
            -away * Fixed::from_int(self.height) * Fixed::from_ratio(9, 10),
        );
        let light_length = (palette.sun[0].powi(2) + palette.sun[1].powi(2)).sqrt();
        let sun_side = (-palette.sun[0] * heading[0] - palette.sun[1] * heading[1]) / light_length;
        let sun_front = (-palette.sun[0] * heading[1] + palette.sun[1] * heading[0]) / light_length;
        let sun_power = match palette.day {
            super::raster::Daylight::Day => Fixed::ONE,
            super::raster::Daylight::Sunset => Fixed::from_ratio(11, 10),
            super::raster::Daylight::Night => Fixed::from_ratio(45, 100),
        };
        let light = Light {
            x: Fixed::from_int(cx) - sun_side * Fixed::from_int(r(176)),
            slope: (p.roll.tan() * Fixed::from_ratio(9, 10) - sun_side * Fixed::from_ratio(6, 10))
                .clamp(-Fixed::from_ratio(24, 10), Fixed::from_ratio(24, 10)),
            half: Fixed::from_int(r(78)),
            alpha: Fixed::from_int(58)
                * sun_power
                * (Fixed::from_ratio(72, 100) - Fixed::from_ratio(28, 100) * sun_front)
                / Fixed::from_int(256),
            color: match palette.day {
                super::raster::Daylight::Day => rgba(
                    Fixed::from_int(255),
                    Fixed::from_int(246),
                    Fixed::from_int(222),
                ),
                super::raster::Daylight::Sunset => rgba(
                    Fixed::from_int(255),
                    Fixed::from_int(170),
                    Fixed::from_int(92),
                ),
                super::raster::Daylight::Night => rgba(
                    Fixed::from_int(150),
                    Fixed::from_int(190),
                    Fixed::from_int(255),
                ),
            },
            top: self.panel_top,
        };
        Some(PreparedCockpit {
            projection: p,
            offsets: [canopy, glass, panel, deck, lever, stick, rear],
            light,
            heading,
        })
    }
    pub(super) fn covers(
        &self,
        clip: [i32; 4],
        prepared: &PreparedCockpit,
        mask: &mut [u8],
    ) -> bool {
        mirui::trace_span!("flight.cockpit.coverage");
        let mut row_scratch = [0; ROW_PIXELS];
        let mut above_scratch = [0; ROW_PIXELS];
        mirui_sprite_stack::raster::fully_covered(clip, mask, |y, row_mask| {
            let mut covered = 0;
            for (layer, offset) in self.layers.iter().zip([0, 2, 3, 6]) {
                covered += layer.mark_coverage(
                    [clip[0], y, clip[2], y + 1],
                    prepared.offsets[offset],
                    row_mask,
                    &mut row_scratch,
                    &mut above_scratch,
                );
                if covered == row_mask.len() {
                    break;
                }
            }
            covered
        })
    }
    pub fn draw(&self, canvas: &mut Canvas<'_>, world: &World, flight: &Flight, palette: &Palette) {
        if let Some(prepared) = self.prepare(flight, palette) {
            self.draw_prepared(canvas, world, flight, &prepared);
        }
    }
    pub(super) fn draw_prepared(
        &self,
        canvas: &mut Canvas<'_>,
        world: &World,
        flight: &Flight,
        prepared: &PreparedCockpit,
    ) {
        mirui::trace_span!("flight.cockpit");
        let p = prepared.projection;
        let [canopy, glass, panel, deck, lever, stick, rear] = prepared.offsets;
        let light = prepared.light;
        let s = self.scale;
        let r = |v: i32| scaled_pixel(v, s);
        let cx = self.width / 2;
        let j = &flight.jets[0];
        let b = self.body;
        let a = self.hot;
        self.lay(
            canvas,
            0,
            canopy,
            Some(Light {
                alpha: light.alpha * Fixed::from_ratio(5, 10),
                ..light
            }),
        );
        self.hud(canvas, flight, &p, glass);
        self.lay(canvas, 1, panel, Some(light));
        {
            let mut live = Live {
                canvas,
                offset: [(panel[0]).trunc_to_int(), (panel[1]).trunc_to_int()],
            };
            let clip = live.clip();
            for (i, lamp) in self.lamps.iter().enumerate() {
                if flight.boost > Fixed::from_ratio(25, 100)
                    || i < 2
                    || (i == 2 && (flight.time * Fixed::from_int(2)).trunc_to_int() % 2 == 0)
                    || (i == 4
                        && hash8((flight.time * Fixed::from_int(3)).trunc_to_int() as u32 * 7)
                            > 200)
                {
                    live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[4]);
                    live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + 1, a[5]);
                }
            }
            for (i, key) in self.panel_keys.iter().enumerate() {
                if hash8(
                    (i * 53) as u32
                        + (flight.time
                            * (Fixed::from_ratio(7, 10)
                                + Fixed::from_int((i) as i32) * Fixed::from_ratio(23, 100)))
                        .trunc_to_int() as u32,
                ) <= 150
                {
                    live.rect(key.x, key.y, key.x + key.w, key.y + key.h, b[3]);
                }
            }
            let radar = self.screens[0];
            let zoom = Fixed::from_int(9) / s;
            let sweep = (flight.time * Fixed::from_ratio(16, 10)).rem_euclid(TAU);
            for yy in 0.max(clip[1] - radar.y)..radar.h.min(clip[3] - radar.y) {
                for xx in 0.max(clip[0] - radar.x)..radar.w.min(clip[2] - radar.x) {
                    let dx = xx - radar.w / 2;
                    let dy = yy - radar.h / 2;
                    let cell = &self.radar[(yy * radar.w + xx) as usize];
                    let right = Fixed::from_int(dx) * zoom;
                    let ahead = Fixed::from_int(-dy) * zoom;
                    let mat = world.material(
                        (j.position[0] + right * prepared.heading[0] + ahead * prepared.heading[1])
                            .trunc_to_int(),
                        (j.position[1] + right * prepared.heading[1] - ahead * prepared.heading[0])
                            .trunc_to_int(),
                    );
                    let mut tone = if mat >= 12 {
                        4
                    } else if mat >= 4 {
                        3
                    } else if mat >= 2 || yy & 1 != 0 {
                        1
                    } else {
                        0
                    };
                    if mat < 4 && (dx == 0 || dy == 0) {
                        tone = 2;
                    }
                    if mat < 12 && cell.flags & RadarCell::RING != 0 {
                        tone = tone.max(if mat >= 4 { 4 } else { 2 });
                    }
                    if cell.flags & RadarCell::INSIDE != 0 {
                        let angle = (sweep - cell.bearing).rem_euclid(TAU);
                        if angle < Fixed::from_ratio(12, 100) {
                            tone = 5;
                        } else if angle < Fixed::from_ratio(7, 10) && tone < 5 {
                            tone += 1;
                        }
                    }
                    live.put(radar.x + xx, radar.y + yy, GREEN[tone]);
                }
            }
            for other in &flight.jets[1..flight.count] {
                let ex = other.position[0] - j.position[0];
                let ey = other.position[1] - j.position[1];
                let x = radar.x
                    + radar.w / 2
                    + (((ex * prepared.heading[0] + ey * prepared.heading[1]) / zoom).round())
                        .trunc_to_int();
                let y = radar.y + radar.h / 2
                    - (((ex * prepared.heading[1] - ey * prepared.heading[0]) / zoom).round())
                        .trunc_to_int();
                if x > radar.x + 1
                    && x < radar.x + radar.w - 2
                    && y > radar.y + 1
                    && y < radar.y + radar.h - 2
                {
                    live.rect(x, y, x + 2, y + 2, GREEN[5]);
                }
            }
            for (dx, dy) in [(0, -2), (0, -1), (-1, 0), (0, 0), (1, 0), (-2, 1), (2, 1)] {
                live.put(
                    radar.x + radar.w / 2 + dx,
                    radar.y + radar.h / 2 + dy,
                    GREEN[5],
                );
            }
            for i in 0..3 {
                live.rect(
                    radar.x + 3,
                    radar.y + 3 + i * 3,
                    radar.x + 3 + if i == 1 { 9 } else { 13 },
                    radar.y + 5 + i * 3,
                    GREEN[3],
                );
                live.rect(
                    radar.x + radar.w - 15,
                    radar.y + radar.h - 11 + i * 3,
                    radar.x + radar.w - 3,
                    radar.y + radar.h - 9 + i * 3,
                    GREEN[3],
                );
            }
            instrument_glass(&mut live, radar);
            let status = self.screens[1];
            for y in 0.max(clip[1] - status.y)..status.h.min(clip[3] - status.y) {
                live.rect(
                    status.x,
                    status.y + y,
                    status.x + status.w,
                    status.y + y + 1,
                    GREEN[(y & 1) as usize],
                );
            }
            let fit = (Fixed::from_int(status.h - 8) / Fixed::from_int((jet::LENGTH) as i32))
                .min(
                    Fixed::from_int(status.w) * Fixed::from_ratio(5, 10)
                        / Fixed::from_int((jet::WIDTH) as i32),
                )
                .max(Fixed::from_ratio(1, 100));
            let jw = ((Fixed::from_int((jet::WIDTH) as i32) * fit).round()).trunc_to_int();
            let jh = ((Fixed::from_int((jet::LENGTH) as i32) * fit).round()).trunc_to_int();
            let jx = status.x
                + ((Fixed::from_int(status.w) * Fixed::from_ratio(31, 100)).round()).trunc_to_int()
                - jw / 2;
            let jy = status.y + (status.h - jh) / 2;
            let at = |xx: i32, yy: i32| {
                if xx < 0 || yy < 0 || xx >= jw || yy >= jh {
                    0
                } else {
                    jet::PROJECTIONS.top[((Fixed::from_int(yy) / fit).trunc_to_int() as usize)
                        .min(jet::LENGTH - 1)
                        * jet::WIDTH
                        + ((Fixed::from_int(xx) / fit).trunc_to_int() as usize).min(jet::WIDTH - 1)]
                }
            };
            for yy in 0.max(clip[1] - jy)..jh.min(clip[3] - jy) {
                for xx in 0.max(clip[0] - jx)..jw.min(clip[2] - jx) {
                    let c = at(xx, yy);
                    if c == 0 {
                        continue;
                    }
                    let edge = at(xx - 1, yy) == 0
                        || at(xx + 1, yy) == 0
                        || at(xx, yy - 1) == 0
                        || at(xx, yy + 1) == 0;
                    live.put(
                        jx + xx,
                        jy + yy,
                        if c == 2 {
                            if edge { a[2] } else { a[3] }
                        } else if c == 3 {
                            GREEN[5]
                        } else if edge {
                            GREEN[2]
                        } else {
                            GREEN[3]
                        },
                    );
                }
            }
            if hash8((flight.time * Fixed::from_int(12)).trunc_to_int() as u32) > 60 {
                for side in [-1, 1] {
                    let xx = jx + jw / 2 + side * 2;
                    live.rect(
                        xx - 1,
                        jy + jh,
                        xx + 1,
                        jy + jh + 1 + ((flight.boost * Fixed::from_int(3)).round()).trunc_to_int(),
                        a[5],
                    );
                }
            }
            let bh = status.h - 16;
            for engine in 0..2 {
                let bx = status.x
                    + ((Fixed::from_int(status.w) * Fixed::from_ratio(66, 100)).round())
                        .trunc_to_int()
                    + engine * r(12).max(r(8).max(1));
                let by = status.y + 7;
                let bw = r(8).max(r(5).max(1));
                live.rect(bx - 1, by - 1, bx + bw + 1, by + bh + 1, GREEN[3]);
                live.rect(bx, by, bx + bw, by + bh, GREEN[0]);
                let level = (Fixed::from_ratio(5, 10)
                    + Fixed::from_ratio(45, 100) * flight.boost
                    + (Fixed::from_int(
                        (hash8(
                            (flight.time * Fixed::from_int(9)).trunc_to_int() as u32
                                + engine as u32 * 31,
                        )) as i32,
                    ) - Fixed::from_int(128))
                        / Fixed::from_int(2600))
                .clamp(Fixed::ZERO, Fixed::ONE);
                for yy in 0..((Fixed::from_int(bh) * level).round()).trunc_to_int() {
                    if yy % 3 != 2 {
                        live.rect(
                            bx + 1,
                            by + bh - 1 - yy,
                            bx + bw - 1,
                            by + bh - yy,
                            if Fixed::from_int(yy) > Fixed::from_int(bh) * Fixed::from_ratio(8, 10)
                            {
                                a[4]
                            } else {
                                GREEN[4]
                            },
                        );
                    }
                }
            }
            live.number(
                status.x + 3,
                status.y + 3,
                ((j.position[2] * Fixed::from_int(120)).round()).trunc_to_int() as u32,
                5,
                GREEN[4],
            );
            live.number(
                status.x + 3,
                status.y + status.h - 7,
                ((flight.speed * Fixed::from_ratio(42, 10)).round()).trunc_to_int() as u32,
                3,
                GREEN[4],
            );
            instrument_glass(&mut live, status);
            let ball = self.ball;
            let (sn, cs) = p.roll.sin_cos();
            let per = Fixed::from_int(ball.r) / Fixed::from_int(34);
            let pitch = (j.pitch.to_degrees() * per * Fixed::from_ratio(9, 10)).clamp(
                Fixed::from_int(-ball.r) * Fixed::from_ratio(85, 100),
                Fixed::from_int(ball.r) * Fixed::from_ratio(85, 100),
            );
            let rung = (Fixed::from_int(5) * per * Fixed::from_int(2))
                .round()
                .max(Fixed::from_int(6))
                / Fixed::from_int(2);
            for dy in (-ball.r).max(clip[1] - ball.y)..=ball.r.min(clip[3] - ball.y - 1) {
                for dx in (-ball.r).max(clip[0] - ball.x)..=ball.r.min(clip[2] - ball.x - 1) {
                    let d = Fixed::from_int(dx * dx + dy * dy);
                    if d > Fixed::from_int(ball.r * ball.r) {
                        continue;
                    }
                    let bx = cs * Fixed::from_int(dx) - sn * Fixed::from_int(dy);
                    let by = sn * Fixed::from_int(dx) + cs * Fixed::from_int(dy) - pitch;
                    let mut color = if by > Fixed::ZERO {
                        if by > Fixed::from_int(ball.r) * Fixed::from_ratio(55, 100) {
                            rgba(
                                Fixed::from_int(120),
                                Fixed::from_int(70),
                                Fixed::from_int(30),
                            )
                        } else {
                            rgba(
                                Fixed::from_int(150),
                                Fixed::from_int(92),
                                Fixed::from_int(40),
                            )
                        }
                    } else if by < Fixed::from_int(-ball.r) * Fixed::from_ratio(55, 100) {
                        rgba(
                            Fixed::from_int(70),
                            Fixed::from_int(140),
                            Fixed::from_int(224),
                        )
                    } else {
                        rgba(
                            Fixed::from_int(110),
                            Fixed::from_int(176),
                            Fixed::from_int(240),
                        )
                    };
                    let k = (by / rung).round();
                    if by.abs() < Fixed::from_ratio(6, 10)
                        || (k != Fixed::ZERO
                            && (by - k * rung).abs() < Fixed::from_ratio(55, 100)
                            && bx.abs()
                                < Fixed::from_int(ball.r)
                                    * if (k).trunc_to_int() % 2 == 0 {
                                        Fixed::from_ratio(34, 100)
                                    } else {
                                        Fixed::from_ratio(18, 100)
                                    })
                        || (bx.abs() < Fixed::from_ratio(6, 10)
                            && by.abs() < Fixed::from_int(ball.r) * Fixed::from_ratio(7, 10))
                    {
                        color = WHITE;
                    }
                    let rim = d / Fixed::from_int((ball.r * ball.r).max(1));
                    let fall = if rim > Fixed::from_ratio(9, 10) {
                        Fixed::from_int(130)
                    } else if rim > Fixed::from_ratio(74, 100) {
                        Fixed::from_int(80)
                    } else if rim > Fixed::from_ratio(55, 100) {
                        Fixed::from_int(34)
                    } else {
                        Fixed::ZERO
                    };
                    color = blend(
                        color,
                        BLACK,
                        (fall
                            + (Fixed::from_int(dx + dy) / (Fixed::from_int(ball.r) * SQRT_2)
                                * Fixed::from_int(46))
                            .round())
                        .clamp(Fixed::ZERO, Fixed::from_int(200))
                            / Fixed::from_int(256),
                    );
                    live.put(ball.x + dx, ball.y + dy, color);
                }
            }
            let amber = rgba(
                Fixed::from_int(255),
                Fixed::from_int(206),
                Fixed::from_int(84),
            );
            for dy in (-ball.r).max(clip[1] - ball.y)..0.min(clip[3] - ball.y) {
                for dx in (-ball.r).max(clip[0] - ball.x)..0.min(clip[2] - ball.x) {
                    let d = (Fixed::from_int(dx * dx + dy * dy)).sqrt()
                        / Fixed::from_int(ball.r.max(1));
                    if d > Fixed::from_ratio(5, 10)
                        && d < Fixed::from_ratio(72, 100)
                        && (Fixed::from_int(dx))
                            < Fixed::from_int(dy) * Fixed::from_ratio(4, 10)
                                + Fixed::from_int(ball.r) * Fixed::from_ratio(1, 10)
                        && (Fixed::from_int(dx)) > Fixed::from_int(dy) * Fixed::from_ratio(24, 10)
                    {
                        live.tint(
                            ball.x + dx,
                            ball.y + dy,
                            WHITE,
                            Fixed::from_int(54) / Fixed::from_int(256),
                        );
                    }
                }
            }
            for side in [-1, 1] {
                let start = ball.x
                    + side
                        * ((Fixed::from_int(ball.r) * Fixed::from_ratio(2, 10)).round())
                            .trunc_to_int();
                let end = ball.x
                    + side
                        * ((Fixed::from_int(ball.r) * Fixed::from_ratio(62, 100)).round())
                            .trunc_to_int();
                live.line(
                    [Fixed::from_int(start), Fixed::from_int(ball.y)],
                    [Fixed::from_int(end), Fixed::from_int(ball.y)],
                    amber,
                );
            }
            live.rect(ball.x - 1, ball.y - 1, ball.x + 2, ball.y + 2, amber);
            live.put(
                ball.x,
                ball.y,
                rgba(
                    Fixed::from_int(255),
                    Fixed::from_int(244),
                    Fixed::from_int(190),
                ),
            );
            let needles = [
                -Fixed::from_int(225)
                    + (flight.speed
                        / (flight.settings.speed
                            * flight.settings.boost
                            * Fixed::from_ratio(108, 100))
                        .max(Fixed::ONE))
                    .clamp(Fixed::ZERO, Fixed::ONE)
                        * Fixed::from_int(270),
                -Fixed::from_int(225)
                    + (j.position[2] / flight.settings.ceiling.max(flight.settings.altitude))
                        .clamp(Fixed::ZERO, Fixed::ONE)
                        * Fixed::from_int(270),
                -Fixed::from_int(90)
                    + (flight.climb_tilt / Fixed::from_ratio(5, 10)).clamp(-Fixed::ONE, Fixed::ONE)
                        * Fixed::from_int(120),
                -Fixed::from_int(90)
                    + (j.yaw_rate / flight.settings.agility.max(Fixed::from_ratio(1, 10)))
                        .clamp(-Fixed::ONE, Fixed::ONE)
                        * Fixed::from_int(120),
            ];
            for (dial, degrees) in self.dials.iter().zip(needles) {
                let (sn, cs) = degrees.to_radians().sin_cos();
                let end = [
                    Fixed::from_int(dial.x) + cs * Fixed::from_int(dial.r - 3),
                    Fixed::from_int(dial.y) + sn * Fixed::from_int(dial.r - 3),
                ];
                live.line(
                    [
                        Fixed::from_int(dial.x) + Fixed::ONE,
                        Fixed::from_int(dial.y) + Fixed::ONE,
                    ],
                    [end[0] + Fixed::ONE, end[1] + Fixed::ONE],
                    BLACK,
                );
                live.line(
                    [Fixed::from_int(dial.x), Fixed::from_int(dial.y)],
                    end,
                    a[4],
                );
                live.put(dial.x, dial.y, b[7]);
                for k in 0..4 {
                    let angle = Fixed::from_int(-150 + k * 12) * PI / Fixed::from_int(180);
                    live.tint(
                        dial.x
                            + ((angle.cos() * Fixed::from_int(dial.r - 5)).round()).trunc_to_int(),
                        dial.y
                            + ((angle.sin() * Fixed::from_int(dial.r - 5)).round()).trunc_to_int(),
                        WHITE,
                        Fixed::from_int(110) / Fixed::from_int(256),
                    );
                }
            }
        }
        self.lay(canvas, 2, deck, Some(light));
        {
            let mut live = Live {
                canvas,
                offset: [(deck[0]).trunc_to_int(), (deck[1]).trunc_to_int()],
            };
            for (i, key) in self.deck_keys.iter().enumerate() {
                if hash8(
                    (i * 91) as u32
                        + (flight.time
                            * (Fixed::from_ratio(9, 10)
                                + Fixed::from_int((i) as i32) * Fixed::from_ratio(31, 100)))
                        .trunc_to_int() as u32,
                ) <= 150
                {
                    live.rect(key.x, key.y, key.x + key.w, key.y + key.h, b[3]);
                }
            }
            live.offset = [(lever[0]).trunc_to_int(), (lever[1]).trunc_to_int()];
            let throttle = cx + r(-150);
            let yy = r(290)
                - ((Fixed::from_int(r(290) - r(270))
                    * (Fixed::from_ratio(18, 100) + Fixed::from_ratio(82, 100) * flight.boost)
                        .clamp(Fixed::ZERO, Fixed::ONE))
                .round())
                .trunc_to_int();
            for u in [-7, 10] {
                live.rect(
                    throttle + r(u),
                    yy + r(5),
                    throttle + r(u + 3),
                    self.height + 12,
                    b[5],
                );
                live.rect(
                    throttle + r(u + 3),
                    yy + r(5),
                    throttle + r(u + 4),
                    self.height + 12,
                    b[1],
                );
            }
            live.rect(
                throttle - r(11) - 1,
                yy - 1,
                throttle + r(28) + 1,
                yy + r(9) + 1,
                b[0],
            );
            live.rect(throttle - r(11), yy, throttle + r(28), yy + r(9), a[3]);
            live.rect(
                throttle - r(11),
                yy,
                throttle + r(28),
                yy + r(3).max(r(2).max(1)),
                a[5],
            );
            live.rect(
                throttle - r(11),
                yy + r(6),
                throttle + r(28),
                yy + r(9),
                a[2],
            );
            live.offset = [(stick[0]).trunc_to_int(), (stick[1]).trunc_to_int()];
            let top = r(253)
                - (((flight.climb_tilt / Fixed::from_ratio(5, 10)).clamp(-Fixed::ONE, Fixed::ONE)
                    * Fixed::from_int(r(3)))
                .round())
                .trunc_to_int();
            let lean = (j.yaw_rate / flight.settings.agility.max(Fixed::from_ratio(1, 10)))
                .clamp(-Fixed::ONE, Fixed::ONE)
                * Fixed::from_int(r(9));
            let reach = self.height + r(40) - top;
            let head_height = r(14).max(r(7).max(1));
            let grip_height = r(17).max(r(8).max(1));
            let fold = r(5).max(r(3).max(1));
            for y in top + r(4)..self.height + self.pad {
                let d = y - top;
                let xx = ((Fixed::from_int(cx)
                    + lean * (Fixed::ONE - Fixed::from_int(d) / Fixed::from_int(reach)))
                .round())
                .trunc_to_int()
                    + r(5);
                let half = if d < head_height {
                    r(12)
                } else if d < head_height + grip_height {
                    r(8)
                } else {
                    r(12) + (d - head_height - grip_height) / fold * r(3)
                };
                for i in -half..=half {
                    if (xx + i + y) & 1 == 0 {
                        live.tint(
                            xx + i,
                            y + r(3),
                            BLACK,
                            Fixed::from_int(150) / Fixed::from_int(256),
                        );
                    }
                }
            }
            for y in top..self.height + self.pad {
                let d = y - top;
                let xx = ((Fixed::from_int(cx)
                    + lean * (Fixed::ONE - Fixed::from_int(d) / Fixed::from_int(reach)))
                .round())
                .trunc_to_int();
                let half = if d < head_height {
                    r(12)
                        - if d < 2 {
                            2 - d
                        } else if d > head_height - 2 {
                            1
                        } else {
                            0
                        }
                } else if d < head_height + grip_height {
                    r(8)
                } else {
                    r(12) + (d - head_height - grip_height) / fold * r(3)
                        - i32::from((d - head_height - grip_height) % fold == 0)
                };
                for i in -half..=half {
                    let f = Fixed::from_int(i) / Fixed::from_int(half.max(1));
                    let color = if d < head_height {
                        if d == 0 {
                            b[5]
                        } else if d < r(3).max(r(2).max(1)) {
                            b[6]
                        } else if d > head_height - r(3).max(r(2).max(1)) {
                            b[1]
                        } else if f < -Fixed::from_ratio(8, 10) {
                            b[5]
                        } else if f > Fixed::from_ratio(78, 100) {
                            b[1]
                        } else if f > Fixed::from_ratio(3, 10) {
                            b[2]
                        } else {
                            b[3]
                        }
                    } else if d < head_height + grip_height {
                        let groove =
                            (d - head_height) % r(4).max(r(3).max(1)) == r(3).max(r(2).max(1));
                        if groove {
                            if f < -Fixed::from_ratio(5, 100) {
                                b[2]
                            } else {
                                b[0]
                            }
                        } else if f < -Fixed::from_ratio(8, 10) {
                            b[2]
                        } else if f < -Fixed::from_ratio(45, 100) {
                            b[5]
                        } else if f < -Fixed::from_ratio(5, 100) {
                            b[4]
                        } else if f < Fixed::from_ratio(45, 100) {
                            b[3]
                        } else if f < Fixed::from_ratio(8, 10) {
                            b[1]
                        } else {
                            b[0]
                        }
                    } else {
                        let within = (d - head_height - grip_height) % fold;
                        if within == 0 {
                            if f < Fixed::from_ratio(2, 10) {
                                b[4]
                            } else {
                                b[2]
                            }
                        } else if within == fold - 1 || f > Fixed::from_ratio(5, 10) {
                            b[0]
                        } else if f < -Fixed::from_ratio(55, 100) {
                            b[2]
                        } else {
                            b[1]
                        }
                    };
                    live.put(xx + i, y, color);
                }
                live.put(xx - half - 1, y, b[0]);
                live.put(xx + half + 1, y, b[0]);
            }
            let xx = ((Fixed::from_int(cx) + lean).round()).trunc_to_int();
            for side in [-1, 1] {
                let x = xx + side * r(6);
                live.disc(x, top + r(7) + 1, r(4).max(r(2).max(1)), b[0]);
                live.disc(
                    x,
                    top + r(7),
                    r(3).max(r(2).max(1)),
                    if side > 0 {
                        if flight.boost > Fixed::from_ratio(3, 10) {
                            a[5]
                        } else {
                            a[3]
                        }
                    } else {
                        b[2]
                    },
                );
                live.put(x - 1, top + r(7) - 1, if side > 0 { a[5] } else { b[6] });
            }
        }
        self.lay(canvas, 3, rear, None);
    }
    fn lay(&self, canvas: &mut Canvas<'_>, layer: usize, offset: [Fixed; 3], light: Option<Light>) {
        let source = &self.layers[layer];
        let dx = (offset[0]).trunc_to_int();
        let dy = (offset[1]).trunc_to_int();
        let fraction = offset[2];
        let mut row_scratch = [0; ROW_PIXELS];
        let mut above_scratch = [0; ROW_PIXELS];
        let mut row_bytes = &mut row_scratch;
        let mut above_bytes = &mut above_scratch;
        let mut previous_y = None;
        let fractional = fraction > Fixed::from_ratio(4, 1000);
        for y in canvas.clip[1]..canvas.clip[3] {
            let sy = y - dy + self.pad;
            if sy < 0 || sy >= source.height {
                continue;
            }
            let reuse_above = fractional && previous_y == Some(sy - 1);
            if reuse_above {
                core::mem::swap(&mut row_bytes, &mut above_bytes);
            }
            let row = source.row(sy as usize, row_bytes);
            let above = (fractional && sy > 0).then(|| {
                if reuse_above {
                    source.row_view(sy as usize - 1, above_bytes)
                } else {
                    source.row(sy as usize - 1, above_bytes)
                }
            });
            previous_y = Some(sy);
            if matches!(row, RowPixels::Empty)
                && above
                    .as_ref()
                    .is_none_or(|row| matches!(row, RowPixels::Empty))
            {
                continue;
            }
            for x in canvas.clip[0]..canvas.clip[2] {
                let sx = x - dx + self.pad;
                if sx < 0 || sx >= source.width {
                    continue;
                }
                let (mut color, cover) = row.sample(above.as_ref(), sx as usize, fraction);
                if color == 0 {
                    continue;
                }
                if let Some(light) = light {
                    let mid = light.x + Fixed::from_int(y - light.top) * light.slope;
                    let xx = Fixed::from_int(x);
                    if xx >= mid - light.half
                        && xx < mid + light.half
                        && !(xx >= mid - light.half * Fixed::from_ratio(22, 100)
                            && xx < mid + light.half * Fixed::from_ratio(2, 100))
                    {
                        let edge = (xx - mid + light.half).min(mid + light.half - xx);
                        if edge >= Fixed::from_int(4)
                            || Fixed::from_int((BAYER[((y & 3) * 4 + (x & 3)) as usize]) as i32)
                                <= edge * Fixed::from_int(4)
                        {
                            color = blend(color, light.color, light.alpha);
                        }
                    }
                }
                let out = canvas.index(x, y);
                canvas.put(
                    out,
                    if cover == Fixed::ONE {
                        color
                    } else {
                        blend(canvas.get(out), color, cover)
                    },
                );
            }
        }
    }
    fn hud(&self, canvas: &mut Canvas<'_>, flight: &Flight, p: &Projection, offset: [Fixed; 3]) {
        let alpha = super::flight::smooth(
            (p.cockpit - Fixed::from_ratio(72, 100)) / Fixed::from_ratio(28, 100),
        );
        if alpha <= Fixed::ZERO {
            return;
        }
        let r = |v: i32| scaled_pixel(v, self.scale);
        let cx = self.width / 2;
        let pivot =
            ((Fixed::from_int(self.height) * Fixed::from_ratio(34, 100)).round()).trunc_to_int();
        let gx = (offset[0]).trunc_to_int();
        let gy = (offset[1]).trunc_to_int();
        let top = pivot - r(57) + gy;
        let bottom = r(170) + gy;
        let edge = |y: i32| {
            Fixed::from_int(r(76))
                + Fixed::from_int(r(86) - r(76)) * Fixed::from_int(bottom - y)
                    / Fixed::from_int((bottom - top).max(1))
        };
        for y in top.max(canvas.clip[1])..bottom.min(canvas.clip[3]) {
            let half = (edge(y).round()).trunc_to_int();
            let left = (cx + gx - half).max(0);
            let right = (cx + gx + half).min(self.width - 1);
            for x in left.max(canvas.clip[0])..=right.min(canvas.clip[2] - 1) {
                let at = canvas.index(x, y);
                let sheen = (Fixed::from_int(x - left))
                    < Fixed::from_int(r(10))
                        + Fixed::from_int(y - top) * Fixed::from_ratio(12, 100)
                    && Fixed::from_int(x - left)
                        > Fixed::from_int(r(4))
                            + Fixed::from_int(y - top) * Fixed::from_ratio(12, 100);
                canvas.put(
                    at,
                    blend(
                        canvas.get(at),
                        GLASS,
                        if sheen {
                            Fixed::from_int(30)
                        } else {
                            Fixed::from_int(10)
                        } * alpha
                            / Fixed::from_int(256),
                    ),
                );
            }
            if left > 0 && canvas.contains(left, y) {
                let at = canvas.index(left, y);
                canvas.put(
                    at,
                    blend(
                        canvas.get(at),
                        GLINT,
                        Fixed::from_int(150) / Fixed::from_int(256) * alpha,
                    ),
                );
            }
            if right < self.width - 1 && canvas.contains(right, y) {
                let at = canvas.index(right, y);
                canvas.put(
                    at,
                    blend(
                        canvas.get(at),
                        self.body[5],
                        Fixed::from_int(120) / Fixed::from_int(256) * alpha,
                    ),
                );
            }
        }
        for side in [-1, 1] {
            for y in bottom - r(14)..bottom + 2 {
                let x = cx + gx + side * ((edge(y).round()).trunc_to_int() + 1);
                for i in 0..r(3).max(r(2).max(1)) {
                    let xx = x + side * i;
                    if canvas.contains(xx, y) {
                        let at = canvas.index(xx, y);
                        let color = self.body[if i == 0 { 6 } else { 2 }];
                        canvas.put(at, blend(canvas.get(at), color, alpha));
                    }
                }
            }
        }
        let mut hud = Hud {
            canvas,
            top,
            bottom,
            cx: cx + gx,
            top_half: r(86),
            bottom_half: r(76),
            alpha,
        };
        hud.line(
            [Fixed::from_int(cx + gx - r(86)), Fixed::from_int(top)],
            [Fixed::from_int(cx + gx + r(86)), Fixed::from_int(top)],
            GLINT,
        );
        let q = r(9);
        let l = r(4).max(r(2).max(1));
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                hud.line(
                    [
                        Fixed::from_int(cx + sx * q),
                        Fixed::from_int(pivot + sy * q),
                    ],
                    [
                        Fixed::from_int(cx + sx * (q - l)),
                        Fixed::from_int(pivot + sy * q),
                    ],
                    HUD,
                );
                hud.line(
                    [
                        Fixed::from_int(cx + sx * q),
                        Fixed::from_int(pivot + sy * q),
                    ],
                    [
                        Fixed::from_int(cx + sx * q),
                        Fixed::from_int(pivot + sy * (q - l)),
                    ],
                    HUD,
                );
            }
        }
        hud.line(
            [Fixed::from_int(cx - r(3)), Fixed::from_int(pivot)],
            [Fixed::from_int(cx + r(3)), Fixed::from_int(pivot)],
            HUD,
        );
        hud.line(
            [Fixed::from_int(cx), Fixed::from_int(pivot - r(3))],
            [Fixed::from_int(cx), Fixed::from_int(pivot + r(3))],
            HUD,
        );
        let (sn, cs) = p.roll.sin_cos();
        let focal = p.divide_perspective(p.scale);
        for degrees in (-20_i32..=20).step_by(10) {
            let dy = p.center[1]
                - Fixed::from_int(pivot)
                - focal
                    * (Fixed::from_int(degrees) * PI / Fixed::from_int(180)
                        * Fixed::from_ratio(8, 10))
                    .tan();
            for side in [-1, 1] {
                let xa = Fixed::from_int(side * r(if degrees == 0 { 16 } else { 8 }));
                let xb = Fixed::from_int(side * r(if degrees == 0 { 54 } else { 20 }));
                let a = [
                    Fixed::from_int(cx) + cs * xa + sn * dy,
                    Fixed::from_int(pivot) - sn * xa + cs * dy,
                ];
                let b = [
                    Fixed::from_int(cx) + cs * xb + sn * dy,
                    Fixed::from_int(pivot) - sn * xb + cs * dy,
                ];
                hud.line(a, b, HUD);
                if degrees != 0 {
                    let tick = dy + Fixed::from_int(degrees.signum()) * Fixed::from_int(r(3));
                    hud.line(
                        b,
                        [
                            Fixed::from_int(cx) + cs * xb + sn * tick,
                            Fixed::from_int(pivot) - sn * xb + cs * tick,
                        ],
                        HUD,
                    );
                }
            }
        }
        let half = r(30);
        let step = r(5).max(r(3).max(1));
        for side in [-1, 1] {
            let x = cx + side * r(62);
            hud.line(
                [Fixed::from_int(x), Fixed::from_int(pivot - half)],
                [Fixed::from_int(x), Fixed::from_int(pivot + half)],
                HUD,
            );
            let value = if side < 0 {
                flight.speed * Fixed::from_ratio(6, 10)
            } else {
                flight.jets[0].position[2] * Fixed::from_ratio(8, 10)
            };
            let off = value.rem_euclid(Fixed::from_int(step * 4));
            for yy in (-half - step * 4..=half).step_by(step as usize) {
                let y =
                    ((Fixed::from_int(pivot) + Fixed::from_int(yy) + off).round()).trunc_to_int();
                if y < pivot - half || y > pivot + half {
                    continue;
                }
                let long = (yy + half + step * 4) / step % 4 == 0;
                hud.line(
                    [Fixed::from_int(x), Fixed::from_int(y)],
                    [
                        Fixed::from_int(x - side * r(if long { 6 } else { 3 })),
                        Fixed::from_int(y),
                    ],
                    HUD,
                );
            }
            let px = x + side * r(4);
            hud.line(
                [Fixed::from_int(px), Fixed::from_int(pivot)],
                [
                    Fixed::from_int(px + side * r(4)),
                    Fixed::from_int(pivot - r(3)),
                ],
                HUD,
            );
            hud.line(
                [Fixed::from_int(px), Fixed::from_int(pivot)],
                [
                    Fixed::from_int(px + side * r(4)),
                    Fixed::from_int(pivot + r(3)),
                ],
                HUD,
            );
        }
        hud.number(
            cx - r(62) - 6,
            pivot - half - 8,
            ((flight.speed * Fixed::from_ratio(42, 10)).round()).trunc_to_int() as u32,
            3,
            HUD,
        );
        hud.number(
            cx + r(62) - 10,
            pivot - half - 8,
            ((flight.jets[0].position[2] * Fixed::from_int(120)).round()).trunc_to_int() as u32,
            5,
            HUD,
        );
        for i in 0..3 {
            let y = pivot - r(22) + i * r(4);
            let w = r(if i == 1 { 6 } else { 9 });
            if flight.boost > Fixed::from_int(2 - i) * Fixed::from_ratio(33, 100) {
                hud.line(
                    [Fixed::from_int(cx - w), Fixed::from_int(y)],
                    [Fixed::from_int(cx + w), Fixed::from_int(y)],
                    HUD,
                );
            } else {
                hud.put(cx - w, y, HUD);
                hud.put(cx + w, y, HUD);
            }
        }
        let by = pivot + r(40);
        let span = r(34);
        hud.line(
            [Fixed::from_int(cx - span), Fixed::from_int(by)],
            [Fixed::from_int(cx + span), Fixed::from_int(by)],
            HUD,
        );
        let heading = flight.jets[0]
            .yaw
            .to_degrees()
            .rem_euclid(Fixed::from_int(360));
        let per = Fixed::from_int(r(4).max(r(2).max(1))) / Fixed::from_int(5);
        for degrees in ((((heading - Fixed::from_int(60)) / Fixed::from_int(5)).floor())
            .trunc_to_int()
            * 5..=(heading).trunc_to_int() + 60)
            .step_by(5)
        {
            let x = ((Fixed::from_int(cx) + (Fixed::from_int(degrees) - heading) * per).round())
                .trunc_to_int();
            if (x - cx).abs() <= span {
                hud.line(
                    [Fixed::from_int(x), Fixed::from_int(by)],
                    [
                        Fixed::from_int(x),
                        Fixed::from_int(by - r(if degrees.rem_euclid(15) == 0 { 4 } else { 2 })),
                    ],
                    HUD,
                );
            }
        }
        hud.number(
            cx - 5,
            by - r(4) - 8,
            (heading.round()).trunc_to_int() as u32,
            3,
            HUD,
        );
    }
}
#[derive(Clone, Copy)]
struct Light {
    x: Fixed,
    slope: Fixed,
    half: Fixed,
    alpha: Fixed,
    color: u32,
    top: i32,
}
fn instrument_glass(ink: &mut impl Ink, r: Box2) {
    let [left, top, right, bottom] = ink.clip();
    for y in 0.max(top - r.y)..r.h.min(bottom - r.y) {
        for x in 0.max(left - r.x)..r.w.min(right - r.x) {
            if x < 2 || y < 2 {
                ink.tint(
                    r.x + x,
                    r.y + y,
                    BLACK,
                    if x < 1 || y < 1 {
                        Fixed::from_int(120)
                    } else {
                        Fixed::from_int(60)
                    } / Fixed::from_int(256),
                );
            } else {
                let d = Fixed::from_int(x) + Fixed::from_int(y) * Fixed::from_ratio(9, 10)
                    - Fixed::from_int(r.w) * Fixed::from_ratio(34, 100);
                if d > Fixed::ZERO && d < Fixed::from_int(r.w) * Fixed::from_ratio(16, 100) {
                    ink.tint(
                        r.x + x,
                        r.y + y,
                        WHITE,
                        Fixed::from_int(22) / Fixed::from_int(256),
                    );
                } else if d > Fixed::from_int(r.w) * Fixed::from_ratio(24, 100)
                    && d < Fixed::from_int(r.w) * Fixed::from_ratio(28, 100)
                {
                    ink.tint(
                        r.x + x,
                        r.y + y,
                        WHITE,
                        Fixed::from_int(16) / Fixed::from_int(256),
                    );
                }
            }
        }
    }
}
struct Hud<'a, 'b> {
    canvas: &'a mut Canvas<'b>,
    top: i32,
    bottom: i32,
    cx: i32,
    top_half: i32,
    bottom_half: i32,
    alpha: Fixed,
}
impl Ink for Hud<'_, '_> {
    fn clip(&self) -> [i32; 4] {
        let [left, top, right, bottom] = self.canvas.clip;
        [left, top.max(self.top), right, bottom.min(self.bottom - 2)]
    }
    fn put(&mut self, x: i32, y: i32, color: u32) {
        if y < self.top || y >= self.bottom - 2 || !self.canvas.contains(x, y) {
            return;
        }
        let edge = Fixed::from_int(self.bottom_half)
            + Fixed::from_int(self.top_half - self.bottom_half) * Fixed::from_int(self.bottom - y)
                / Fixed::from_int((self.bottom - self.top).max(1));
        if Fixed::from_int((x - self.cx).abs()) > edge - Fixed::from_int(2) {
            return;
        }
        let at = self.canvas.index(x, y);
        self.canvas
            .put(at, blend(self.canvas.get(at), color, self.alpha));
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        if self.canvas.contains(x, y) {
            self.canvas.get(self.canvas.index(x, y))
        } else {
            0
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::{
        flight::{Settings, View},
        raster::Raster,
    };
    use super::*;
    #[test]
    fn digit_group_clipping_preserves_partial_glyphs_and_skips_empty_groups() {
        struct Probe {
            clip: [i32; 4],
            pixels: [u32; 256],
            visits: usize,
        }
        impl Ink for Probe {
            fn clip(&self) -> [i32; 4] {
                self.clip
            }
            fn get(&self, _: i32, _: i32) -> u32 {
                0
            }
            fn put(&mut self, x: i32, y: i32, color: u32) {
                self.visits += 1;
                let [l, t, r, b] = self.clip;
                if x >= l && x < r && y >= t && y < b {
                    self.pixels[(y * 16 + x) as usize] = color;
                }
            }
        }
        for x in (-16..19).step_by(3) {
            for y in (-8..19).step_by(2) {
                for digits in [1, 4] {
                    let mut full = Probe {
                        clip: [0, 0, 16, 16],
                        pixels: [0; 256],
                        visits: 0,
                    };
                    let mut part = Probe {
                        clip: [3, 2, 11, 13],
                        pixels: [0; 256],
                        visits: 0,
                    };
                    full.number(x, y, 1234, digits, WHITE);
                    part.number(x, y, 1234, digits, WHITE);
                    for yy in 0..16 {
                        for xx in 0..16 {
                            let at = (yy * 16 + xx) as usize;
                            let expected = if (3..11).contains(&xx) && (2..13).contains(&yy) {
                                full.pixels[at]
                            } else {
                                0
                            };
                            assert_eq!(part.pixels[at], expected);
                        }
                    }
                    if x >= 11 || y >= 13 || x + digits as i32 * 4 - 1 <= 3 || y + 5 <= 2 {
                        assert_eq!(part.visits, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn radar_geometry_preserves_ring_and_bearing_at_all_native_cells() {
        for (width, height) in [(1, 1), (2, 3), (37, 22), (86, 51)] {
            let radius = Fixed::from_int(width.min(height)) * Fixed::from_ratio(47, 100);
            for y in 0..height {
                for x in 0..width {
                    let cell = RadarCell::new(x, y, width, height);
                    let dx = x - width / 2;
                    let dy = y - height / 2;
                    let distance = Fixed::from_int(dx * dx + dy * dy).sqrt();
                    let ring = [
                        Fixed::ONE,
                        Fixed::from_ratio(66, 100),
                        Fixed::from_ratio(33, 100),
                    ]
                    .into_iter()
                    .any(|ring| {
                        (distance / radius - ring).abs() < Fixed::from_ratio(5, 10) / radius
                    });
                    assert_eq!(cell.flags & RadarCell::RING != 0, ring);
                    assert_eq!(cell.flags & RadarCell::INSIDE != 0, distance <= radius);
                    if distance <= radius {
                        assert_eq!(
                            cell.bearing,
                            Fixed::from_int(dx).atan2(Fixed::from_int(-dy))
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn transparent_encoded_rows_do_not_read_or_clear_the_workspace() {
        let mut layer = Layer::new(8, 4, 0, true);
        layer.put(3, 1, WHITE);
        layer.seal();
        let LayerPixels::Encoded { rows, .. } = &layer.pixels else {
            panic!("sealed indexed rows");
        };
        assert_eq!(rows[0], rows[1]);
        assert!(rows[1] < rows[2]);
        assert_eq!(rows[2], rows[3]);
        assert_eq!(rows[3], rows[4]);
        let mut scratch = [0xa5; ROW_PIXELS];
        assert!(matches!(layer.row(0, &mut scratch), RowPixels::Empty));
        assert!(scratch.iter().all(|v| *v == 0xa5));
        assert_eq!(layer.row(1, &mut scratch).color(3), WHITE);
        let saved = scratch;
        assert!(matches!(layer.row(2, &mut scratch), RowPixels::Empty));
        assert_eq!(scratch, saved);
    }
    #[test]
    fn opaque_coverage_requires_full_fractional_sample_coverage() {
        let row = RowPixels::Rgba(&[WHITE, 0, WHITE, 0]);
        let above = RowPixels::Rgba(&[0, BLACK, BLACK, 0]);
        let fraction = Fixed::from_ratio(1, 4);
        let samples = core::array::from_fn::<_, 4, _>(|x| row.sample(Some(&above), x, fraction));
        assert_eq!(samples[0], (WHITE, Fixed::from_ratio(3, 4)));
        assert_eq!(samples[1], (BLACK, fraction));
        assert_eq!(samples[2].1, Fixed::ONE);
        assert_eq!(samples[2].0 >> 24, 255);
        assert_eq!(samples[3].0, 0);
    }

    #[test]
    fn covered_regions_do_not_depend_on_background_or_retained_mask() {
        use super::super::raster::{Daylight, Frame};
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.view = View::Cockpit;
        let mut a = Frame::new(32, 32).unwrap();
        let mut b = Frame::new(32, 32).unwrap();
        let mut mask = [255; 1024];
        let mut accepted = 0;
        for day in [Daylight::Day, Daylight::Sunset, Daylight::Night] {
            let palette = Palette::new(day);
            let art = Cockpit::new_compact(128, 128, &palette);
            for steps in [1, 12, 60, 240] {
                for _ in 0..steps {
                    flight.step(&world, 16, 128, 128);
                }
                flight.head[0].position = Fixed::from_ratio(steps % 5 - 2, 4);
                flight.head[1].position = -flight.head[0].position;
                let Some(prepared) = art.prepare(&flight, &palette) else {
                    continue;
                };
                for y in (0..128).step_by(32) {
                    for x in (0..128).step_by(32) {
                        for [w, h] in [[32, 32], [17, 23]] {
                            let clip = [x, y, x + w, y + h];
                            mask.fill(255);
                            let covered = art.covers(clip, &prepared, &mut mask);
                            if !covered {
                                continue;
                            }
                            accepted += 1;
                            assert!(mask[..(w * h) as usize].iter().all(|value| *value == 1));
                            for (frame, background) in [(&mut a, 0xff000000), (&mut b, 0xffffffff)]
                            {
                                frame.with_region([128, 128], [x, y], clip, |canvas| {
                                    for yy in y..y + h {
                                        for xx in x..x + w {
                                            let at = canvas.index(xx, yy);
                                            canvas.put(at, background);
                                        }
                                    }
                                    art.draw_prepared(canvas, &world, &flight, &prepared);
                                });
                            }
                            let ap = a.texture.read_pixels().unwrap();
                            let bp = b.texture.read_pixels().unwrap();
                            for yy in 0..h {
                                let start = yy as usize * 32 * 4;
                                let end = start + w as usize * 4;
                                assert_eq!(
                                    &ap.bytes()[start..end],
                                    &bp.bytes()[start..end],
                                    "{day:?}, steps {steps}, clip {clip:?}"
                                );
                            }
                        }
                    }
                }
                assert!(!art.covers([0, 0, 0, 1], &prepared, &mut mask));
            }
        }
        assert!(
            accepted > 0,
            "settled cockpit must hide some background regions"
        );
    }

    #[test]
    fn cockpit_regions_match_full_resolution_composition() {
        use super::super::raster::{Daylight, Frame};
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.view = View::Cockpit;
        for _ in 0..360 {
            flight.step(&world, 16, 128, 128);
        }
        flight.head[0].position = Fixed::from_ratio(1, 2);
        flight.head[1].position = Fixed::from_ratio(-3, 4);
        let palette = Palette::new(Daylight::Day);
        let art = Cockpit::new_compact(128, 128, &palette);
        let mut full = Frame::new(128, 128).unwrap();
        full.with_canvas(|canvas| {
            for at in 0..128 * 128 {
                canvas.put(at, 0xff603010);
            }
            art.draw(canvas, &world, &flight, &palette);
        });
        let reference = full.texture.read_pixels().unwrap();
        let mut tile = Frame::new(24, 24).unwrap();
        for y in (0..128).step_by(24) {
            for x in (0..128).step_by(24) {
                tile.with_region([128, 128], [x, y], [0, 0, 128, 128], |canvas| {
                    for at in 0..24 * 24 {
                        canvas.put(at, 0xff603010);
                    }
                    art.draw(canvas, &world, &flight, &palette);
                });
                let pixels = tile.texture.read_pixels().unwrap();
                for yy in y..(y + 24).min(128) {
                    for xx in x..(x + 24).min(128) {
                        let a = (yy * 128 + xx) as usize;
                        let b = ((yy - y) * 24 + xx - x) as usize;
                        assert_eq!(
                            &reference.bytes()[a * 4..a * 4 + 4],
                            &pixels.bytes()[b * 4..b * 4 + 4],
                            "cockpit region at {xx},{yy}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn compressed_layers_preserve_pixels_and_fractional_live_composition() {
        let world = World::with_size(7, Fixed::from_ratio(33, 100), 64, 32, 16);
        let mut flight = Flight::new(&world, Settings::default());
        flight.view = View::Cockpit;
        for _ in 0..360 {
            flight.step(&world, 16, 128, 128);
        }
        for day in [
            super::super::raster::Daylight::Day,
            super::super::raster::Daylight::Sunset,
            super::super::raster::Daylight::Night,
        ] {
            let palette = Palette::new(day);
            for (width, height) in [(128, 128), (128, 64), (64, 128), (26, 26)] {
                let dense = Cockpit::new(width, height, &palette);
                let compact = Cockpit::new_compact(width, height, &palette);
                assert!(compact.bytes() < 20 * 1024);
                for (plain, encoded) in dense.layers.iter().zip(&compact.layers) {
                    let mut a = [0; ROW_PIXELS];
                    let mut b = [0; ROW_PIXELS];
                    for y in 0..plain.height as usize {
                        let left = plain.row(y, &mut a);
                        let right = encoded.row(y, &mut b);
                        for x in 0..plain.width as usize {
                            assert_eq!(left.color(x), right.color(x));
                        }
                    }
                }
                if (width, height) != (128, 128) {
                    continue;
                }
                let mut a = super::super::raster::Frame::new(width, height).unwrap();
                let mut b = super::super::raster::Frame::new(width, height).unwrap();
                for offset in [
                    Fixed::ZERO,
                    Fixed::from_ratio(1, 2),
                    Fixed::from_ratio(-3, 4),
                ] {
                    flight.head[0].position = offset;
                    flight.head[1].position = offset;
                    let allocations = crate::test_support::tracked_allocations(|| {
                        for (frame, art) in [(&mut a, &dense), (&mut b, &compact)] {
                            frame.with_canvas(|canvas| {
                                for at in 0..width * height {
                                    canvas.put(at, 0xff60_3010);
                                }
                                art.draw(canvas, &world, &flight, &palette);
                            });
                        }
                    });
                    assert_eq!(allocations, 0);
                    assert_eq!(
                        a.texture.read_pixels().unwrap().bytes(),
                        b.texture.read_pixels().unwrap().bytes()
                    );
                }
            }
        }
    }
    #[test]
    fn layered_cockpit_has_bounded_storage_and_live_instruments_allocate_nothing() {
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let mut flight = Flight::new(&world, Settings::default());
        flight.view = View::Cockpit;
        for _ in 0..300 {
            flight.step(&world, 16, 480, 300);
        }
        let mut raster = Raster::new(480, 300).unwrap();
        let art = Cockpit::new(480, 300, &raster.palette);
        assert!(art.bytes() < 3 * 1024 * 1024);
        assert!(art.layers.iter().all(|layer| {
            (0..layer.width * layer.height).any(|at| layer.color_at(at as usize) != 0)
        }));
        raster.render(
            &world,
            &flight,
            &crate::raster::PlacedAircraft::from_flight(&flight),
        );
        let allocations = crate::test_support::tracked_allocations(|| {
            raster
                .frame
                .with_canvas(|canvas| art.draw(canvas, &world, &flight, &raster.palette))
        });
        assert_eq!(allocations, 0);
        let pixels = raster.frame.texture.read_pixels().unwrap();
        assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
    }
}
