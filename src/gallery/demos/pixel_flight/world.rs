#![doc = " Seeded, wrapping terrain and cloud fields. Map generation is a cold path."]
use super::math::FixedMath as _;
use crate::types::Fixed;
use alloc::{vec, vec::Vec};
pub const SIDE: usize = 1024;
pub const CLOUD_SIDE: usize = 512;
pub const CLOUD_ALT: Fixed = Fixed::from_int(40);
pub const CLOUD_LAYERS: usize = 8;
pub const MAX_HEIGHT: usize = 48;
pub const TOP: [[u8; 3]; 17] = [
    [36, 104, 180],
    [44, 124, 198],
    [66, 160, 216],
    [104, 204, 214],
    [238, 220, 164],
    [222, 200, 140],
    [96, 178, 52],
    [70, 148, 44],
    [134, 206, 76],
    [70, 158, 64],
    [42, 122, 54],
    [28, 92, 46],
    [146, 140, 134],
    [118, 112, 110],
    [178, 172, 162],
    [246, 250, 255],
    [214, 226, 244],
];
pub const WALL: [[u8; 3]; 17] = [
    [36, 104, 180],
    [44, 124, 198],
    [66, 160, 216],
    [104, 204, 214],
    [204, 180, 122],
    [204, 180, 122],
    [56, 120, 44],
    [50, 108, 42],
    [60, 128, 46],
    [24, 78, 42],
    [22, 72, 40],
    [20, 64, 38],
    [96, 90, 92],
    [86, 82, 86],
    [104, 98, 98],
    [186, 202, 228],
    [176, 192, 220],
];
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
    pub heights: Vec<u8>,
    pub materials: Vec<u8>,
    pub clouds: Vec<u8>,
    pub noise: Vec<u8>,
    pub spawn: [Fixed; 2],
    side: usize,
    cloud_side: usize,
    noise_side: usize,
    sample_shift: u32,
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
        let mut noise = vec![0; noise_side * noise_side];
        let stride = 256 / noise_side;
        for i in 0..256 * 256 {
            let value = (random.next() * Fixed::from_int(256)).trunc_to_int() as u8;
            let (x, y) = (i % 256, i / 256);
            if x % stride == 0 && y % stride == 0 {
                noise[(y / stride) * noise_side + x / stride] = value;
            }
        }
        let mut world = Self {
            heights: vec![0; side * side],
            materials: vec![0; side * side],
            clouds: vec![0; cloud_side * cloud_side],
            noise,
            spawn: [Fixed::from_int((side << sample_shift) as i32) / 2; 2],
            side,
            cloud_side,
            noise_side,
            sample_shift,
        };
        let mut elevation = vec![Fixed::from_ratio(0, 1); side * side];
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
                &mut random,
                &mut lattice,
                &mut blend,
            );
        }
        let min = elevation.iter().copied().fold(Fixed::MAX, Fixed::min);
        let max = elevation.iter().copied().fold(Fixed::MIN, Fixed::max);
        let mut bins = [0usize; 1024];
        for value in &mut elevation {
            *value = (*value - min) / (max - min).max(Fixed::from_ratio(1, 256));
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
                    world.materials[i] = if depth > Fixed::from_ratio(7, 100) {
                        0
                    } else if depth > Fixed::from_ratio(3, 100) {
                        1
                    } else if depth > Fixed::from_ratio(8, 1000) {
                        2
                    } else {
                        3
                    };
                } else if d < Fixed::from_ratio(12, 1000) {
                    world.materials[i] = if h < 34 { 5 } else { 4 };
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
                    world.heights[i] = if level <= 8 {
                        level * 2
                    } else {
                        16 + (level - 8) * 3
                    };
                    let gradient = elevation[index(x as i32 - 2, y as i32 - 2, side)]
                        - elevation[index(x as i32 + 2, y as i32 + 2, side)];
                    let lit = gradient < -Fixed::from_ratio(7, 1000);
                    let shaded = gradient > Fixed::from_ratio(7, 1000);
                    world.materials[i] = if level >= 13 {
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
        for gy in (0..side).step_by(5) {
            for gx in (0..side).step_by(5) {
                let hs = hash8((gy * 977 + gx * 131 + 7) as u32);
                let cx = (gx + usize::from(hs & 3)) as i32;
                let cy = (gy + usize::from((hs >> 2) & 3)) as i32;
                let i = index(cx, cy, side);
                if !(6..=8).contains(&world.materials[i]) || world.heights[i] > 14 {
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
                let base = world.heights[i];
                for dy in -r..=r {
                    for dx in -r..=r {
                        let distance = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                        if distance > Fixed::from_int(r) + Fixed::from_ratio(3, 10) {
                            continue;
                        }
                        let j = index(cx + dx, cy + dy, side);
                        if world.materials[j] < 4 {
                            continue;
                        }
                        let height = base
                            + 1
                            + (((Fixed::ONE
                                - distance / (Fixed::from_int(r) + Fixed::from_ratio(5, 10)))
                                * Fixed::from_int(r + 1))
                            .round())
                            .trunc_to_int() as u8;
                        if height > world.heights[j] {
                            world.heights[j] = height;
                            world.materials[j] = if distance < Fixed::from_ratio(11, 10) {
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
        let mut best = -1;
        for _ in 0..400 {
            let x = (random.next() * Fixed::from_int(side as i32)).trunc_to_int() << sample_shift;
            let y = (random.next() * Fixed::from_int(side as i32)).trunc_to_int() << sample_shift;
            if world.material(x, y) > 2 {
                continue;
            }
            let near = (0..8)
                .filter(|a| {
                    let angle = Fixed::from_int(*a) * (super::math::PI / 4);
                    world.material(
                        x + (angle.cos() * Fixed::from_int(120)).trunc_to_int(),
                        y + (angle.sin() * Fixed::from_int(120)).trunc_to_int(),
                    ) >= 4
                })
                .count() as i32;
            if near > best && near <= 5 {
                best = near;
                world.spawn = [Fixed::from_int(x), Fixed::from_int(y)];
            }
        }
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
            let length = (Fixed::from_int(12) + random.next() * Fixed::from_int(24)) * scale;
            let puffs = 4 + (random.next() * Fixed::from_int(7)).trunc_to_int();
            for puff in 0..puffs {
                let t = Fixed::from_int(puff) / Fixed::from_int(puffs - 1) * Fixed::from_int(2)
                    - Fixed::ONE;
                let px = cx
                    + angle.cos() * t * length
                    + (random.next() - Fixed::from_ratio(5, 10)) * Fixed::from_int(9) * scale;
                let py = cy
                    + angle.sin() * t * length
                    + (random.next() - Fixed::from_ratio(5, 10)) * Fixed::from_int(9) * scale;
                let radius = (Fixed::from_int(6) + random.next() * Fixed::from_int(8))
                    * scale
                    * (Fixed::ONE - Fixed::from_ratio(45, 100) * t.abs());
                let thickness = (radius * Fixed::from_ratio(6, 10))
                    .round()
                    .clamp(Fixed::from_int(2), Fixed::from_int((CLOUD_LAYERS) as i32));
                let bound = (radius.ceil()).trunc_to_int();
                for dy in -bound..=bound {
                    for dx in -bound..=bound {
                        let d = Fixed::from_int(dx * dx + dy * dy) / (radius * radius);
                        if d > Fixed::ONE {
                            continue;
                        }
                        let n = (((Fixed::ONE - d).sqrt() * thickness)
                            .round()
                            .max(Fixed::ONE))
                        .trunc_to_int() as u8;
                        let j = index(
                            (px.round()).trunc_to_int() + dx,
                            (py.round()).trunc_to_int() + dy,
                            cloud_side,
                        );
                        if n > world.clouds[j] & 15 {
                            world.clouds[j] = (rank << 4) | n;
                        }
                    }
                }
            }
        }
        world
    }
    pub fn height(&self, x: i32, y: i32) -> u8 {
        self.heights[index(x >> self.sample_shift, y >> self.sample_shift, self.side)]
    }
    pub fn material(&self, x: i32, y: i32) -> u8 {
        self.materials[index(x >> self.sample_shift, y >> self.sample_shift, self.side)]
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
        self.heights.capacity()
            + self.materials.capacity()
            + self.clouds.capacity()
            + self.noise.capacity()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn seeded_fields_are_bounded_wrapped_and_deterministic() {
        let a = World::new(7, Fixed::from_ratio(33, 100));
        let b = World::new(7, Fixed::from_ratio(33, 100));
        assert_eq!(a.bytes(), 2_424_832);
        assert_eq!(a.heights, b.heights);
        assert_eq!(a.materials, b.materials);
        assert_eq!(a.clouds, b.clouds);
        assert!(a.heights.iter().all(|h| usize::from(*h) < MAX_HEIGHT));
        assert!(a.materials.iter().all(|m| usize::from(*m) < TOP.len()));
        for (x, y) in [(-1, -1), (0, 0), (123, 456), (-99_999, 123_456)] {
            assert_eq!(a.height(x, y), a.height(x + SIDE as i32, y - SIDE as i32));
            assert_eq!(
                a.cloud(x, y),
                a.cloud(x - CLOUD_SIDE as i32, y + CLOUD_SIDE as i32)
            );
        }
        let land = Fixed::from_int((a.materials.iter().filter(|m| **m >= 4).count()) as i32)
            / Fixed::from_int((SIDE * SIDE) as i32);
        assert!(
            (land - Fixed::from_ratio(33, 100)).abs() < Fixed::from_ratio(1, 50),
            "land fraction {land}"
        );
        assert!(a.materials.iter().any(|m| (9..=11).contains(m)));
        assert!(a.clouds.iter().any(|c| *c != 0));
        assert!(a.material((a.spawn[0]).trunc_to_int(), (a.spawn[1]).trunc_to_int()) <= 2);
    }
}
