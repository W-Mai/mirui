#![doc = " Resize-time metalwork layers and live flight instruments."]
use super::math::FixedMath as _;
use super::math::{PI, SQRT_2, TAU};
use super::{
    flight::{Flight, Projection, V3, cockpit_center},
    jet,
    raster::{BAYER, Canvas, Palette, blend, rgba},
    world::{Ground, World, hash8},
};
use alloc::{vec, vec::Vec};

mod artwork;
mod instruments;
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
// Baked pixels use the aircraft palette's body and hot-metal slots. The remaining
// slots are fixed instrument colors. A lighting change only updates this table.
const BODY_INKS: [u32; 9] = [1, 2, 3, 4, 5, 6, 7, 8, 9];
const HOT_INKS: [u32; 6] = [17, 18, 19, 20, 21, 22];
const GLINT_INK: u32 = 23;
const SCREEN_INK: u32 = 24;
const BRASS_INKS: [u32; 3] = [25, 26, 27];
const KEY_INK: u32 = 28;
const WHITE_INK: u32 = 29;
type Inks = [u32; 30];
const INSTRUMENT_INKS: Inks = {
    let mut inks = [0; 30];
    inks[GLINT_INK as usize] = GLINT;
    inks[SCREEN_INK as usize] = GREEN[0];
    inks[BRASS_INKS[0] as usize] = u32::from_le_bytes([120, 86, 26, 255]);
    inks[BRASS_INKS[1] as usize] = u32::from_le_bytes([206, 158, 54, 255]);
    inks[BRASS_INKS[2] as usize] = u32::from_le_bytes([255, 222, 120, 255]);
    inks[KEY_INK as usize] = u32::from_le_bytes([255, 206, 84, 255]);
    inks[WHITE_INK as usize] = WHITE;
    inks
};

enum LayerPixels {
    Indexed(Vec<u8>),
    Encoded { data: Vec<u8>, rows: Vec<u32> },
}
enum RowPixels<'a> {
    Indexed(&'a [u8], &'a Inks),
    Empty,
}
#[derive(Clone, Copy)]
pub(super) struct PreparedCockpit {
    roll: (Fixed, Fixed),
    needle_ends: [[Fixed; 2]; 4],
    projection: Projection,
    offsets: [[Fixed; 3]; 7],
    light: Light,
    heading: [Fixed; 2],
    instruments: Instruments,
}

#[derive(Clone, Copy)]
struct Instruments {
    time: Fixed,
    speed: Fixed,
    boost: Fixed,
    position: V3,
    yaw: Fixed,
    pitch: Fixed,
    climb: Fixed,
    rudder: Fixed,
    wingmen: [Option<[Fixed; 2]>; 2],
}

impl Instruments {
    fn new(flight: &Flight) -> Self {
        let jet = &flight.jets[0];
        Self {
            time: flight.time,
            speed: flight.speed,
            boost: flight.boost,
            position: jet.position,
            yaw: jet.yaw,
            pitch: jet.pitch,
            climb: (flight.climb_tilt / Fixed::from_ratio(5, 10)).clamp(-Fixed::ONE, Fixed::ONE),
            rudder: (jet.yaw_rate / flight.settings.agility.max(Fixed::from_ratio(1, 10)))
                .clamp(-Fixed::ONE, Fixed::ONE),
            wingmen: core::array::from_fn(|i| {
                (i + 1 < flight.count).then(|| {
                    let [x, y, _] = flight.jets[i + 1].position;
                    [x, y]
                })
            }),
        }
    }
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
        if compact {
            assert!((width + pad * 2) as usize <= ROW_PIXELS);
        }
        Self {
            pixels: LayerPixels::Indexed(Vec::new()),
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
        inks: &Inks,
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
            let row = self.row(sy, row_scratch, inks);
            let above = (fraction > Fixed::from_ratio(4, 1000) && sy > 0)
                .then(|| self.row(sy - 1, above_scratch, inks));
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
            LayerPixels::Indexed(data) => data.capacity(),
            LayerPixels::Encoded { data, rows } => data.capacity() + rows.capacity() * 4,
        }
    }
    fn ink_at(&self, at: usize) -> u32 {
        match &self.pixels {
            LayerPixels::Indexed(data) => u32::from(data.get(at).copied().unwrap_or(0)),
            LayerPixels::Encoded { .. } => panic!("completed cockpit layer is immutable"),
        }
    }
    fn set_ink_at(&mut self, at: usize, ink: u32) {
        let LayerPixels::Indexed(data) = &mut self.pixels else {
            panic!("completed cockpit layer is immutable")
        };
        if data.is_empty() {
            data.resize((self.width * self.height) as usize, 0);
        }
        assert!(ink < INSTRUMENT_INKS.len() as u32, "cockpit material index");
        data[at] = ink as u8;
    }
    fn seal(&mut self, compact: bool) {
        let LayerPixels::Indexed(data) = &self.pixels else {
            return;
        };
        if !compact || data.is_empty() {
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
        };
    }
    fn row<'a>(
        &'a self,
        y: usize,
        scratch: &'a mut [u8; ROW_PIXELS],
        inks: &'a Inks,
    ) -> RowPixels<'a> {
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
        self.row_view(y, scratch, inks)
    }
    // Encoded scratch must already contain this row; transparent rows do not read it.
    fn row_view<'a>(
        &'a self,
        y: usize,
        scratch: &'a [u8; ROW_PIXELS],
        inks: &'a Inks,
    ) -> RowPixels<'a> {
        let width = self.width as usize;
        match &self.pixels {
            LayerPixels::Indexed(data) if !data.is_empty() => {
                RowPixels::Indexed(&data[y * width..(y + 1) * width], inks)
            }
            LayerPixels::Encoded { rows, .. } if rows[y] != rows[y + 1] => {
                RowPixels::Indexed(&scratch[..width], inks)
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
            self.set_ink_at((y * self.width + x) as usize, color);
        }
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.ink_at((y * self.width + x) as usize)
        } else {
            0
        }
    }
}
struct Live<'a, 'b> {
    canvas: &'a mut Canvas<'b>,
    offset: [i32; 2],
}
impl<'a, 'b> Live<'a, 'b> {
    fn new(canvas: &'a mut Canvas<'b>, [x, y, _]: [Fixed; 3]) -> Self {
        Self {
            canvas,
            offset: [x.trunc_to_int(), y.trunc_to_int()],
        }
    }
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
    inks: Inks,
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
        let mut cockpit = Self::with_storage(width, height, false);
        cockpit.set_palette(palette);
        cockpit
    }
    #[cfg(test)]
    pub(super) fn new_compact(width: usize, height: usize, palette: &Palette) -> Self {
        let mut cockpit = Self::with_storage(width, height, true);
        cockpit.set_palette(palette);
        cockpit
    }
    pub(super) fn size(&self) -> [usize; 2] {
        [self.width as usize, self.height as usize]
    }
    pub(super) fn set_palette(&mut self, palette: &Palette) {
        self.inks[1..=9].copy_from_slice(&palette.jets[0][1..=9]);
        self.inks[17..=22].copy_from_slice(&palette.jets[0][17..=22]);
    }
    pub(super) fn prepare(
        &self,
        flight: &Flight,
        p: Projection,
        palette: &Palette,
    ) -> Option<PreparedCockpit> {
        self.prepare_lit(flight, p, palette.day, palette.sun)
    }

    pub(super) fn prepare_lit(
        &self,
        flight: &Flight,
        p: Projection,
        day: crate::raster::Daylight,
        sun: [Fixed; 2],
    ) -> Option<PreparedCockpit> {
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
        let light_length = (sun[0].powi(2) + sun[1].powi(2)).sqrt();
        let sun_side = (-sun[0] * heading[0] - sun[1] * heading[1]) / light_length;
        let sun_front = (-sun[0] * heading[1] + sun[1] * heading[0]) / light_length;
        let sun_power = match day {
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
            color: match day {
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
            roll: p.roll.sin_cos(),
            needle_ends: self.needle_ends(flight),
            projection: p,
            offsets: [canopy, glass, panel, deck, lever, stick, rear],
            light,
            heading,
            instruments: Instruments::new(flight),
        })
    }
    fn needle_ends(&self, flight: &Flight) -> [[Fixed; 2]; 4] {
        let j = &flight.jets[0];
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
        core::array::from_fn(|index| {
            let dial = self.dials[index];
            let (sn, cs) = needles[index].to_radians().sin_cos();
            [
                Fixed::from_int(dial.x) + cs * Fixed::from_int(dial.r - 3),
                Fixed::from_int(dial.y) + sn * Fixed::from_int(dial.r - 3),
            ]
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
                    &self.inks,
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
        let projection = flight.projection(self.width as usize, self.height as usize);
        if let Some(prepared) = self.prepare(flight, projection, palette) {
            self.draw_prepared(canvas, world.at(Some(Default::default())), &prepared);
        }
    }
    pub(super) fn draw_prepared(
        &self,
        canvas: &mut Canvas<'_>,
        world: Ground<'_>,
        prepared: &PreparedCockpit,
    ) {
        mirui::trace_span!("flight.cockpit");
        let [canopy, glass, panel, deck, lever, stick, rear] = prepared.offsets;
        let light = prepared.light;
        self.lay(
            canvas,
            0,
            canopy,
            Some(Light {
                alpha: light.alpha * Fixed::HALF,
                ..light
            }),
        );
        let instruments = &prepared.instruments;
        self.hud(canvas, instruments, &prepared.projection, glass);
        self.lay(canvas, 1, panel, Some(light));
        let mut panel = Live::new(canvas, panel);
        self.paint_indicators(&mut panel, instruments);
        self.paint_radar(&mut panel, world, prepared);
        self.paint_status(&mut panel, instruments);
        self.paint_attitude(&mut panel, prepared);
        self.paint_dials(&mut panel, prepared);
        self.lay(canvas, 2, deck, Some(light));
        self.paint_controls(canvas, instruments, [deck, lever, stick]);
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
            let row = source.row(sy as usize, row_bytes, &self.inks);
            let above = (fractional && sy > 0).then(|| {
                if reuse_above {
                    source.row_view(sy as usize - 1, above_bytes, &self.inks)
                } else {
                    source.row(sy as usize - 1, above_bytes, &self.inks)
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
    fn hud(
        &self,
        canvas: &mut Canvas<'_>,
        instruments: &Instruments,
        p: &Projection,
        offset: [Fixed; 3],
    ) {
        let alpha = super::flight::smooth(
            (p.cockpit - Fixed::from_ratio(72, 100)) / Fixed::from_ratio(28, 100),
        );
        if alpha <= Fixed::ZERO {
            return;
        }
        let body = &self.inks[1..=9];
        let r = |v: i32| scaled_pixel(v, self.scale);
        let cx = self.width / 2;
        let pivot = cockpit_center(self.height as usize).trunc_to_int();
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
                        body[5],
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
                        let color = body[if i == 0 { 6 } else { 2 }];
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
        let focal = p.camera.divide_perspective(p.camera.scale);
        for degrees in (-20_i32..=20).step_by(10) {
            let dy = p.camera.center[1]
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
                instruments.speed * Fixed::from_ratio(6, 10)
            } else {
                instruments.position[2] * Fixed::from_ratio(8, 10)
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
            ((instruments.speed * Fixed::from_ratio(42, 10)).round()).trunc_to_int() as u32,
            3,
            HUD,
        );
        hud.number(
            cx + r(62) - 10,
            pivot - half - 8,
            ((instruments.position[2] * Fixed::from_int(120)).round()).trunc_to_int() as u32,
            5,
            HUD,
        );
        for i in 0..3 {
            let y = pivot - r(22) + i * r(4);
            let w = r(if i == 1 { 6 } else { 9 });
            if instruments.boost > Fixed::from_int(2 - i) * Fixed::from_ratio(33, 100) {
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
        let heading = instruments
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
        let inks = INSTRUMENT_INKS;
        layer.put(3, 1, WHITE_INK);
        layer.seal(true);
        let LayerPixels::Encoded { rows, .. } = &layer.pixels else {
            panic!("sealed indexed rows");
        };
        assert_eq!(rows[0], rows[1]);
        assert!(rows[1] < rows[2]);
        assert_eq!(rows[2], rows[3]);
        assert_eq!(rows[3], rows[4]);
        let mut scratch = [0xa5; ROW_PIXELS];
        assert!(matches!(
            layer.row(0, &mut scratch, &inks),
            RowPixels::Empty
        ));
        assert!(scratch.iter().all(|v| *v == 0xa5));
        assert_eq!(layer.row(1, &mut scratch, &inks).color(3), WHITE);
        let saved = scratch;
        assert!(matches!(
            layer.row(2, &mut scratch, &inks),
            RowPixels::Empty
        ));
        assert_eq!(scratch, saved);
    }
    #[test]
    fn opaque_coverage_requires_full_fractional_sample_coverage() {
        let mut inks = [0; 30];
        inks[1] = WHITE;
        inks[2] = BLACK;
        let row = RowPixels::Indexed(&[1, 0, 1, 0], &inks);
        let above = RowPixels::Indexed(&[0, 2, 2, 0], &inks);
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
                let Some(prepared) = art.prepare(
                    &flight,
                    flight.projection(art.width as usize, art.height as usize),
                    &palette,
                ) else {
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
                                    art.draw_prepared(
                                        canvas,
                                        world.at(Some(Default::default())),
                                        &prepared,
                                    );
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
                        let left = plain.row(y, &mut a, &dense.inks);
                        let right = encoded.row(y, &mut b, &compact.inks);
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
        let mut raster = Raster::new([480, 300], false).unwrap();
        let mut frame = crate::raster::Frame::new(480, 300).unwrap();
        let art = Cockpit::new(480, 300, &raster.palette);
        assert!(art.bytes() < 3 * 1024 * 1024);
        assert!(art.layers.iter().all(|layer| {
            (0..layer.width * layer.height).any(|at| layer.ink_at(at as usize) != 0)
        }));
        raster.render(
            &mut frame,
            &world,
            &flight,
            &crate::raster::PlacedAircraft::from_flight(&flight),
        );
        let allocations = crate::test_support::tracked_allocations(|| {
            frame.with_canvas(|canvas| art.draw(canvas, &world, &flight, &raster.palette))
        });
        assert_eq!(allocations, 0);
        let pixels = frame.texture.read_pixels().unwrap();
        assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
    }
}
