#![doc = " Seeded, wrapping terrain and cloud fields. Map generation is a cold path."]
pub(crate) mod coordinates;

use super::math::FixedMath as _;
pub use crate::palette::terrain::{TOP, WALL};
use alloc::{vec, vec::Vec};
use mirui::core::cache::Handle;
use mirui::types::{Color, Fixed};
use mirui_sprite_stack::{HeightField, Pose, TerrainGrid};
#[derive(Clone, Copy)]
enum GroundPose {
    Original,
    Placed {
        origin: [Fixed; 3],
        rotation: [Fixed; 2],
    },
    Absent,
}

#[derive(Clone, Copy)]
pub(super) struct Ground<'a> {
    pub(super) world: &'a World,
    placement: GroundPose,
}

impl Ground<'_> {
    pub(super) fn is_original(&self) -> bool {
        matches!(self.placement, GroundPose::Original)
    }

    pub(super) fn elevation(&self) -> Fixed {
        match self.placement {
            GroundPose::Placed { origin, .. } => origin[1],
            _ => Fixed::ZERO,
        }
    }
    fn local_address(
        &self,
        x: Fixed,
        y: Fixed,
        origin: [Fixed; 3],
        [sin, cos]: [Fixed; 2],
    ) -> usize {
        let dx = x - origin[0];
        let dz = -y - origin[2];
        self.world
            .ground
            .address_at(dx * cos - dz * sin, dx * sin + dz * cos)
            .expect("flight ground repeats across the map")
    }

    pub(super) fn placed_sample(&self, [x, y]: [Fixed; 2]) -> Option<(Fixed, usize)> {
        let GroundPose::Placed { origin, rotation } = self.placement else {
            return None;
        };
        let i = self.local_address(x, y, origin, rotation);
        Some((
            Fixed::from_int(i32::from(self.world.ground.levels()[i])) + origin[1],
            i,
        ))
    }
    pub(super) fn height(&self, x: i32, y: i32) -> Fixed {
        if self.is_original() {
            return Fixed::from_int(i32::from(self.world.height(x, y)));
        }
        self.placed_sample([x, y].map(Fixed::from_int))
            .map_or(Fixed::ZERO, |(h, _)| h)
    }
    pub(super) fn material(&self, x: i32, y: i32) -> u8 {
        match self.placement {
            GroundPose::Original => self.world.material(x, y),
            GroundPose::Placed { origin, rotation } => self.world.ground.materials()
                [self.local_address(Fixed::from_int(x), Fixed::from_int(y), origin, rotation)],
            GroundPose::Absent => 0,
        }
    }
}

pub const SIDE: usize = 1024;
pub const CLOUD_SIDE: usize = 512;
pub const CLOUD_ALT: Fixed = Fixed::from_int(40);
pub const CLOUD_LAYERS: usize = 8;
pub const MAX_HEIGHT: usize = 48;
const TERRAIN_COLORS: [Color; TOP.len()] = {
    let mut colors = [Color::rgb(0, 0, 0); TOP.len()];
    let mut index = 0;
    while index < colors.len() {
        let [r, g, b] = TOP[index];
        colors[index] = Color::rgb(r, g, b);
        index += 1;
    }
    colors
};

pub fn hash8(value: u32) -> u8 {
    let mut h = (value ^ (value >> 13)).wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    (h >> 24) as u8
}
struct Random(u32);
impl Random {
    fn next(&mut self) -> Fixed {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        Fixed::from_ratio(((t ^ (t >> 14)) >> 16) as i32, 65536)
    }
}
#[doc = " A unit is one ground pixel. Reducing the map changes repetition distance."]
pub struct World {
    ground: Handle<HeightField>,
    pub clouds: Vec<u8>,
    pub noise: Vec<u8>,
    pub spawn: [Fixed; 2],
    side: usize,
    cloud_side: usize,
    noise_side: usize,
    sample_shift: u32,
}
// Flight samples integer map coordinates and points its nose toward decreasing map Y.
fn flight_map_coordinates([x, z]: [Fixed; 2]) -> [Fixed; 2] {
    let y = (-z.trunc_to_int()).rem_euclid(SIDE as i32);
    [Fixed::from_int(x.trunc_to_int()), Fixed::from_int(y)]
}
fn index(x: i32, y: i32, side: usize) -> usize {
    (y as usize & (side - 1)) * side + (x as usize & (side - 1))
}
fn octave(
    field: &mut [Fixed],
    cell: usize,
    amplitude: Fixed,
    random: &mut Random,
    lattice: &mut [Fixed],
    blend: &mut [Fixed],
) {
    let side = blend.len();
    let count = side / cell;
    for value in &mut lattice[..count * count] {
        *value = random.next();
    }
    for (x, value) in blend.iter_mut().enumerate() {
        let f = Fixed::from_int((x % cell) as i32) / Fixed::from_int((cell) as i32);
        *value = f * f * (Fixed::from_int(3) - Fixed::from_int(2) * f);
    }
    for y in 0..side {
        let a = y / cell * count;
        let b = ((y / cell + 1) % count) * count;
        for x in 0..side {
            let i = x / cell;
            let j = (i + 1) % count;
            let va = lattice[a + i] + (lattice[a + j] - lattice[a + i]) * blend[x];
            let vb = lattice[b + i] + (lattice[b + j] - lattice[b + i]) * blend[x];
            field[y * side + x] += amplitude * (va + (vb - va) * blend[y]);
        }
    }
}
fn generate_noise(random: &mut Random, noise_side: usize) -> Vec<u8> {
    let mut noise = vec![0; noise_side * noise_side];
    let stride = 256 / noise_side;
    for i in 0..256 * 256 {
        let value = (random.next() * Fixed::from_int(256)).trunc_to_int() as u8;
        let (x, y) = (i % 256, i / 256);
        if x % stride == 0 && y % stride == 0 {
            noise[(y / stride) * noise_side + x / stride] = value;
        }
    }
    noise
}

fn generate_elevation(random: &mut Random, side: usize) -> Vec<Fixed> {
    let mut elevation = vec![Fixed::ZERO; side * side];
    let mut lattice = vec![Fixed::ZERO; 64 * 64];
    let mut blend = vec![Fixed::ZERO; side];
    for (cell, amplitude) in [
        (512, Fixed::from_ratio(55, 100)),
        (256, Fixed::ONE),
        (128, Fixed::from_ratio(5, 10)),
        (64, Fixed::from_ratio(25, 100)),
        (32, Fixed::from_ratio(11, 100)),
        (16, Fixed::from_ratio(5, 100)),
    ] {
        octave(
            &mut elevation,
            (cell * side / SIDE).max(1),
            amplitude,
            random,
            &mut lattice,
            &mut blend,
        );
    }
    elevation
}

fn find_sea_level(elevation: &mut [Fixed], land: Fixed) -> Fixed {
    let min = elevation.iter().copied().fold(Fixed::MAX, Fixed::min);
    let max = elevation.iter().copied().fold(Fixed::MIN, Fixed::max);
    let range = (max - min).max(Fixed::from_ratio(1, 256));
    let mut bins = [0usize; 1024];
    for value in elevation.iter_mut() {
        *value = (*value - min) / range;
        bins[(*value * Fixed::from_int(1023)).trunc_to_int() as usize] += 1;
    }
    let target = (Fixed::ONE - land.clamp(Fixed::from_ratio(2, 100), Fixed::from_ratio(9, 10)))
        * Fixed::from_int((elevation.len()) as i32);
    let mut acc = 0;
    let mut sea = Fixed::ONE;
    for (i, n) in bins.into_iter().enumerate() {
        acc += n;
        if Fixed::from_int((acc) as i32) >= target {
            sea = Fixed::from_int((i + 1) as i32) / Fixed::from_int(1023);
            break;
        }
    }
    sea
}

fn fill_land(
    elevation: &[Fixed],
    heights: &mut [u8],
    materials: &mut [u8],
    side: usize,
    sea: Fixed,
) {
    let span = (Fixed::ONE - sea - Fixed::from_ratio(12, 1000)).max(Fixed::from_ratio(5, 100));
    for y in 0..side {
        for x in 0..side {
            let i = y * side + x;
            let h = hash8(i as u32);
            let d = elevation[i] - sea;
            if d < Fixed::ZERO {
                let depth = -d
                    + (Fixed::from_int((h) as i32) - Fixed::from_int(128))
                        * Fixed::from_ratio(4, 100000);
                materials[i] = if depth > Fixed::from_ratio(7, 100) {
                    0
                } else if depth > Fixed::from_ratio(3, 100) {
                    1
                } else if depth > Fixed::from_ratio(8, 1000) {
                    2
                } else {
                    3
                };
            } else if d < Fixed::from_ratio(12, 1000) {
                materials[i] = if h < 34 { 5 } else { 4 };
            } else {
                let relative = (d - Fixed::from_ratio(12, 1000)) / span;
                let level = (if relative < Fixed::from_ratio(5, 10) {
                    1 + ((relative * Fixed::from_int(16)).trunc_to_int() as u8)
                } else if relative < Fixed::from_ratio(78, 100) {
                    9 + (((relative - Fixed::from_ratio(5, 10)) / Fixed::from_ratio(7, 100))
                        .trunc_to_int() as u8)
                } else {
                    13 + (((relative - Fixed::from_ratio(78, 100)) / Fixed::from_ratio(9, 100))
                        .trunc_to_int() as u8)
                })
                .min(15);
                heights[i] = if level <= 8 {
                    level * 2
                } else {
                    16 + (level - 8) * 3
                };
                let gradient = elevation[index(x as i32 - 2, y as i32 - 2, side)]
                    - elevation[index(x as i32 + 2, y as i32 + 2, side)];
                let lit = gradient < -Fixed::from_ratio(7, 1000);
                let shaded = gradient > Fixed::from_ratio(7, 1000);
                materials[i] = if level >= 13 {
                    if shaded { 16 } else { 15 }
                } else if level >= 9 {
                    if lit {
                        14
                    } else if shaded || h < 26 {
                        13
                    } else {
                        12
                    }
                } else if lit {
                    8
                } else if shaded || h < 22 {
                    7
                } else if h > 240 {
                    8
                } else {
                    6
                };
            }
        }
    }
}

fn add_trees(
    elevation: &[Fixed],
    heights: &mut [u8],
    materials: &mut [u8],
    side: usize,
    sea: Fixed,
) {
    for gy in (0..side).step_by(5) {
        for gx in (0..side).step_by(5) {
            let hs = hash8((gy * 977 + gx * 131 + 7) as u32);
            let cx = (gx + usize::from(hs & 3)) as i32;
            let cy = (gy + usize::from((hs >> 2) & 3)) as i32;
            let i = index(cx, cy, side);
            if !(6..=8).contains(&materials[i]) || heights[i] > 14 {
                continue;
            }
            let f = elevation[index(side as i32 - 1 - cx + 911, cy + 613, side)];
            let probability = (f - sea) * Fixed::from_int(30);
            if probability <= Fixed::ZERO
                || probability * Fixed::from_int(255)
                    < Fixed::from_int((hash8(i as u32 + 99)) as i32)
            {
                continue;
            }
            let r = 2 + i32::from((hs >> 4) & 1);
            let base = heights[i];
            for dy in -r..=r {
                for dx in -r..=r {
                    let distance = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                    if distance > Fixed::from_int(r) + Fixed::from_ratio(3, 10) {
                        continue;
                    }
                    let j = index(cx + dx, cy + dy, side);
                    if materials[j] < 4 {
                        continue;
                    }
                    let height = base
                        + 1
                        + (((Fixed::ONE
                            - distance / (Fixed::from_int(r) + Fixed::from_ratio(5, 10)))
                            * Fixed::from_int(r + 1))
                        .round())
                        .trunc_to_int() as u8;
                    if height > heights[j] {
                        heights[j] = height;
                        materials[j] = if distance < Fixed::from_ratio(11, 10) {
                            9
                        } else if distance < Fixed::from_int(r) - Fixed::from_ratio(5, 10) {
                            10
                        } else {
                            11
                        };
                    }
                }
            }
        }
    }
}

fn generate_ground(
    random: &mut Random,
    side: usize,
    sample_shift: u32,
    land: Fixed,
) -> HeightField {
    let mut heights = vec![0; side * side];
    let mut materials = vec![0; side * side];
    {
        let mut elevation = generate_elevation(random, side);
        let sea = find_sea_level(&mut elevation, land);
        fill_land(&elevation, &mut heights, &mut materials, side, sea);
        add_trees(&elevation, &mut heights, &mut materials, side, sea);
    }
    let grid = TerrainGrid::repeating([side as u16; 2], Fixed::from_int(1 << sample_shift))
        .expect("bounded flight grid");
    HeightField::from_planes(grid, heights, materials)
        .and_then(|field| field.with_palette(&TERRAIN_COLORS))
        .expect("generated flight materials")
        .with_coordinate_map(flight_map_coordinates)
}

impl World {
    pub fn new(seed: u32, land: Fixed) -> Self {
        Self::with_size(seed, land, SIDE, CLOUD_SIDE, 256)
    }
    pub(super) fn with_size(
        seed: u32,
        land: Fixed,
        side: usize,
        cloud_side: usize,
        noise_side: usize,
    ) -> Self {
        assert!(side.is_power_of_two() && (64..=SIDE).contains(&side));
        assert!(cloud_side.is_power_of_two() && cloud_side <= CLOUD_SIDE);
        assert!(noise_side.is_power_of_two() && noise_side <= 256);
        let sample_shift = (SIDE / side).trailing_zeros();
        let mut random = Random(seed.wrapping_mul(7919).wrapping_add(13));
        let noise = generate_noise(&mut random, noise_side);
        let ground = generate_ground(&mut random, side, sample_shift, land);
        let mut world = Self {
            ground: Handle::new(ground),
            clouds: vec![0; cloud_side * cloud_side],
            noise,
            spawn: [Fixed::from_int((side << sample_shift) as i32) / 2; 2],
            side,
            cloud_side,
            noise_side,
            sample_shift,
        };
        world.select_spawn(&mut random);
        world.generate_clouds(&mut random);
        world
    }

    fn select_spawn(&mut self, random: &mut Random) {
        let side = self.side;
        let sample_shift = self.sample_shift;
        let directions: [[i32; 2]; 8] = core::array::from_fn(|i| {
            let angle = Fixed::from_int(i as i32) * (super::math::PI / 4);
            let (sin, cos) = angle.sin_cos();
            [cos, sin].map(|axis| (axis * Fixed::from_int(120)).trunc_to_int())
        });
        let mut best = -1;
        for _ in 0..400 {
            let x = (random.next() * Fixed::from_int(side as i32)).trunc_to_int() << sample_shift;
            let y = (random.next() * Fixed::from_int(side as i32)).trunc_to_int() << sample_shift;
            if self.material(x, y) > 2 {
                continue;
            }
            let near = directions
                .iter()
                .filter(|[dx, dy]| self.material(x + dx, y + dy) >= 4)
                .count() as i32;
            if near > best && near <= 5 {
                best = near;
                self.spawn = [Fixed::from_int(x), Fixed::from_int(y)];
            }
        }
    }

    fn generate_clouds(&mut self, random: &mut Random) {
        let cloud_side = self.cloud_side;
        let cloud_groups = (50 * cloud_side * cloud_side / (CLOUD_SIDE * CLOUD_SIDE)).max(1);
        let density_scale = Fixed::from_ratio(cloud_side as i32, CLOUD_SIDE as i32)
            * Fixed::from_ratio(50, cloud_groups as i32).sqrt();
        for i in 0..cloud_groups {
            let rank = (i & 15) as u8;
            let cx = random.next() * Fixed::from_int((cloud_side) as i32);
            let cy = random.next() * Fixed::from_int((cloud_side) as i32);
            let scale = (Fixed::from_ratio(6, 10)
                + random.next() * random.next() * Fixed::from_ratio(125, 100))
                * density_scale;
            let angle = Fixed::from_ratio(35, 100)
                + (random.next() - Fixed::from_ratio(5, 10)) * Fixed::from_ratio(13, 10);
            let (sin, cos) = angle.sin_cos();
            let length = (Fixed::from_int(12) + random.next() * Fixed::from_int(24)) * scale;
            let puffs = 4 + (random.next() * Fixed::from_int(7)).trunc_to_int();
            for puff in 0..puffs {
                let t = Fixed::from_int(puff) / Fixed::from_int(puffs - 1) * Fixed::from_int(2)
                    - Fixed::ONE;
                let px = cx
                    + cos * t * length
                    + (random.next() - Fixed::from_ratio(5, 10)) * Fixed::from_int(9) * scale;
                let py = cy
                    + sin * t * length
                    + (random.next() - Fixed::from_ratio(5, 10)) * Fixed::from_int(9) * scale;
                let radius = (Fixed::from_int(6) + random.next() * Fixed::from_int(8))
                    * scale
                    * (Fixed::ONE - Fixed::from_ratio(45, 100) * t.abs());
                let thickness = (radius * Fixed::from_ratio(6, 10))
                    .round()
                    .clamp(Fixed::from_int(2), Fixed::from_int((CLOUD_LAYERS) as i32));
                let bound = (radius.ceil()).trunc_to_int();
                let radius_squared = radius * radius;
                let x = px.round().trunc_to_int();
                let y = py.round().trunc_to_int();
                for dy in -bound..=bound {
                    for dx in -bound..=bound {
                        let d = Fixed::from_int(dx * dx + dy * dy) / radius_squared;
                        if d > Fixed::ONE {
                            continue;
                        }
                        let n = (((Fixed::ONE - d).sqrt() * thickness)
                            .round()
                            .max(Fixed::ONE))
                        .trunc_to_int() as u8;
                        let j = index(x + dx, y + dy, cloud_side);
                        if n > self.clouds[j] & 15 {
                            self.clouds[j] = (rank << 4) | n;
                        }
                    }
                }
            }
        }
    }

    pub(super) fn at(&self, pose: Option<Pose>) -> Ground<'_> {
        let placement = match pose {
            Some(p) if p == Pose::default() => GroundPose::Original,
            Some(p) if p.pitch.is_zero() && p.roll.is_zero() => GroundPose::Placed {
                origin: p.position,
                rotation: [Fixed::sin_deg(p.yaw), Fixed::cos_deg(p.yaw)],
            },
            _ => GroundPose::Absent,
        };
        Ground {
            world: self,
            placement,
        }
    }

    /// Shares the generated ground with scene objects and additional viewports.
    pub fn field(&self) -> Handle<HeightField> {
        self.ground.clone()
    }

    pub(super) fn ground_index(&self, x: i32, y: i32) -> usize {
        index(x >> self.sample_shift, y >> self.sample_shift, self.side)
    }
    pub(super) fn ground_planes(&self) -> (&[u8], &[u8]) {
        (self.ground.levels(), self.ground.materials())
    }
    pub fn height(&self, x: i32, y: i32) -> u8 {
        self.ground.levels()[self.ground_index(x, y)]
    }
    pub fn material(&self, x: i32, y: i32) -> u8 {
        self.ground.materials()[self.ground_index(x, y)]
    }
    pub fn cloud(&self, x: i32, y: i32) -> u8 {
        self.clouds[index(
            x >> self.sample_shift,
            y >> self.sample_shift,
            self.cloud_side,
        )]
    }
    pub fn period(&self) -> usize {
        self.side << self.sample_shift
    }
    pub fn cloud_period(&self) -> usize {
        self.cloud_side << self.sample_shift
    }
    pub fn noise_at(&self, x: i32, y: i32) -> u8 {
        let shift = (256 / self.noise_side).trailing_zeros();
        self.noise[index(x >> shift, y >> shift, self.noise_side)]
    }
    pub fn bytes(&self) -> usize {
        core::mem::size_of::<mirui::core::cache::handle::CacheEntry<HeightField>>()
            + 2 * core::mem::size_of::<usize>()
            + self.ground.levels().len()
            + self.ground.materials().len()
            + self.clouds.capacity()
            + self.noise.capacity()
    }
}
#[cfg(test)]
#[path = "../tests/unit/world.rs"]
mod tests;
