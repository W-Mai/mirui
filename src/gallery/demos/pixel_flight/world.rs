//! Seeded, wrapping terrain and cloud fields. Map generation is a cold path.

use alloc::{vec, vec::Vec};

pub const SIDE: usize = 1024;
pub const CLOUD_SIDE: usize = 512;
pub const CLOUD_ALT: f32 = 40.0;
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
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    }
}

/// A unit is one ground pixel. Reducing the map changes repetition distance.
pub struct World {
    pub heights: Vec<u8>,
    pub materials: Vec<u8>,
    pub clouds: Vec<u8>,
    pub noise: Vec<u8>,
    pub spawn: [f32; 2],
}

fn index(x: i32, y: i32, side: usize) -> usize {
    (y as usize & (side - 1)) * side + (x as usize & (side - 1))
}

fn octave(field: &mut [f32], cell: usize, amplitude: f32, random: &mut Random) {
    let count = SIDE / cell;
    // One bounded lattice; all coordinate interpolation stays on the stack.
    let mut lattice = [0.0; 64 * 64];
    for value in &mut lattice[..count * count] {
        *value = random.next();
    }
    let mut blend = [0.0; SIDE];
    for (x, value) in blend.iter_mut().enumerate() {
        let f = (x % cell) as f32 / cell as f32;
        *value = f * f * (3.0 - 2.0 * f);
    }
    for y in 0..SIDE {
        let a = y / cell * count;
        let b = ((y / cell + 1) % count) * count;
        for x in 0..SIDE {
            let i = x / cell;
            let j = (i + 1) % count;
            let va = lattice[a + i] + (lattice[a + j] - lattice[a + i]) * blend[x];
            let vb = lattice[b + i] + (lattice[b + j] - lattice[b + i]) * blend[x];
            field[y * SIDE + x] += amplitude * (va + (vb - va) * blend[y]);
        }
    }
}

impl World {
    pub fn new(seed: u32, land: f32) -> Self {
        let mut random = Random(seed.wrapping_mul(7919).wrapping_add(13));
        let mut world = Self {
            heights: vec![0; SIDE * SIDE],
            materials: vec![0; SIDE * SIDE],
            clouds: vec![0; CLOUD_SIDE * CLOUD_SIDE],
            noise: (0..256 * 256)
                .map(|_| (random.next() * 256.0) as u8)
                .collect(),
            spawn: [SIDE as f32 / 2.0; 2],
        };
        let mut elevation = vec![0.0; SIDE * SIDE];
        for (cell, amplitude) in [
            (512, 0.55),
            (256, 1.0),
            (128, 0.5),
            (64, 0.25),
            (32, 0.11),
            (16, 0.05),
        ] {
            octave(&mut elevation, cell, amplitude, &mut random);
        }
        let min = elevation.iter().copied().fold(f32::INFINITY, f32::min);
        let max = elevation.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mut bins = [0usize; 1024];
        for value in &mut elevation {
            *value = (*value - min) / (max - min).max(f32::EPSILON);
            bins[(*value * 1023.0) as usize] += 1;
        }
        let target = (1.0 - land.clamp(0.02, 0.9)) * elevation.len() as f32;
        let mut acc = 0;
        let mut sea = 1.0;
        for (i, n) in bins.into_iter().enumerate() {
            acc += n;
            if acc as f32 >= target {
                sea = (i + 1) as f32 / 1023.0;
                break;
            }
        }
        let span = (1.0 - sea - 0.012).max(0.05);
        for y in 0..SIDE {
            for x in 0..SIDE {
                let i = y * SIDE + x;
                let h = hash8(i as u32);
                let d = elevation[i] - sea;
                if d < 0.0 {
                    let depth = -d + (f32::from(h) - 128.0) * 0.00004;
                    world.materials[i] = if depth > 0.07 {
                        0
                    } else if depth > 0.03 {
                        1
                    } else if depth > 0.008 {
                        2
                    } else {
                        3
                    };
                } else if d < 0.012 {
                    world.materials[i] = if h < 34 { 5 } else { 4 };
                } else {
                    let relative = (d - 0.012) / span;
                    let level = (if relative < 0.5 {
                        1 + (relative * 16.0) as u8
                    } else if relative < 0.78 {
                        9 + ((relative - 0.5) / 0.07) as u8
                    } else {
                        13 + ((relative - 0.78) / 0.09) as u8
                    })
                    .min(15);
                    world.heights[i] = if level <= 8 {
                        level * 2
                    } else {
                        16 + (level - 8) * 3
                    };
                    let gradient = elevation[index(x as i32 - 2, y as i32 - 2, SIDE)]
                        - elevation[index(x as i32 + 2, y as i32 + 2, SIDE)];
                    let lit = gradient < -0.007;
                    let shaded = gradient > 0.007;
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
        for gy in (0..SIDE).step_by(5) {
            for gx in (0..SIDE).step_by(5) {
                let hs = hash8((gy * 977 + gx * 131 + 7) as u32);
                let cx = (gx + usize::from(hs & 3)) as i32;
                let cy = (gy + usize::from((hs >> 2) & 3)) as i32;
                let i = index(cx, cy, SIDE);
                if !(6..=8).contains(&world.materials[i]) || world.heights[i] > 14 {
                    continue;
                }
                let f = elevation[index(SIDE as i32 - 1 - cx + 911, cy + 613, SIDE)];
                let probability = (f - sea) * 30.0;
                if probability <= 0.0 || probability * 255.0 < f32::from(hash8(i as u32 + 99)) {
                    continue;
                }
                let r = 2 + i32::from((hs >> 4) & 1);
                let base = world.heights[i];
                for dy in -r..=r {
                    for dx in -r..=r {
                        let distance = ((dx * dx + dy * dy) as f32).sqrt();
                        if distance > r as f32 + 0.3 {
                            continue;
                        }
                        let j = index(cx + dx, cy + dy, SIDE);
                        if world.materials[j] < 4 {
                            continue;
                        }
                        let height = base
                            + 1
                            + ((1.0 - distance / (r as f32 + 0.5)) * (r + 1) as f32).round() as u8;
                        if height > world.heights[j] {
                            world.heights[j] = height;
                            world.materials[j] = if distance < 1.1 {
                                9
                            } else if distance < r as f32 - 0.5 {
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
            let x = (random.next() * SIDE as f32) as i32;
            let y = (random.next() * SIDE as f32) as i32;
            if world.material(x, y) > 2 {
                continue;
            }
            let near = (0..8)
                .filter(|a| {
                    let angle = *a as f32 * core::f32::consts::FRAC_PI_4;
                    world.material(
                        x + (angle.cos() * 120.0) as i32,
                        y + (angle.sin() * 120.0) as i32,
                    ) >= 4
                })
                .count() as i32;
            if near > best && near <= 5 {
                best = near;
                world.spawn = [x as f32, y as f32];
            }
        }
        // Preserve cloud size and density when the wrapping field becomes smaller.
        for i in 0..50 {
            let rank = (i & 15) as u8;
            let cx = random.next() * CLOUD_SIDE as f32;
            let cy = random.next() * CLOUD_SIDE as f32;
            let scale = 0.6 + random.next() * random.next() * 1.25;
            let angle = 0.35 + (random.next() - 0.5) * 1.3;
            let length = (12.0 + random.next() * 24.0) * scale;
            let puffs = 4 + (random.next() * 7.0) as i32;
            for puff in 0..puffs {
                let t = puff as f32 / (puffs - 1) as f32 * 2.0 - 1.0;
                let px = cx + angle.cos() * t * length + (random.next() - 0.5) * 9.0 * scale;
                let py = cy + angle.sin() * t * length + (random.next() - 0.5) * 9.0 * scale;
                let radius = (6.0 + random.next() * 8.0) * scale * (1.0 - 0.45 * t.abs());
                let thickness = (radius * 0.6).round().clamp(2.0, CLOUD_LAYERS as f32);
                let bound = radius.ceil() as i32;
                for dy in -bound..=bound {
                    for dx in -bound..=bound {
                        let d = (dx * dx + dy * dy) as f32 / (radius * radius);
                        if d > 1.0 {
                            continue;
                        }
                        let n = ((1.0 - d).sqrt() * thickness).round().max(1.0) as u8;
                        let j = index(px.round() as i32 + dx, py.round() as i32 + dy, CLOUD_SIDE);
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
        self.heights[index(x, y, SIDE)]
    }
    pub fn material(&self, x: i32, y: i32) -> u8 {
        self.materials[index(x, y, SIDE)]
    }
    pub fn cloud(&self, x: i32, y: i32) -> u8 {
        self.clouds[index(x, y, CLOUD_SIDE)]
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
        let a = World::new(7, 0.33);
        let b = World::new(7, 0.33);
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
        let land = a.materials.iter().filter(|m| **m >= 4).count() as f32 / (SIDE * SIDE) as f32;
        assert!((land - 0.33).abs() < 0.02, "land fraction {land}");
        assert!(a.materials.iter().any(|m| (9..=11).contains(m)));
        assert!(a.clouds.iter().any(|c| *c != 0));
        assert!(a.material(a.spawn[0] as i32, a.spawn[1] as i32) <= 2);
    }
}
