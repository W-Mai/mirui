#![doc = " Seeded, wrapping terrain and cloud fields. Map generation is a cold path."]
pub(crate) mod coordinates;
mod generate;
mod ground;

pub(crate) use ground::Ground;

pub use crate::presentation::palette::terrain::{TOP, WALL};
use alloc::vec::Vec;
use mirui::core::cache::Handle;
use mirui::types::{Color, Fixed};
use mirui_sprite_stack::HeightField;
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
impl World {
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
