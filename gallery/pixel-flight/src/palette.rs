use mirui::types::{Color, Fixed};

mod aircraft;
mod environment;
pub(crate) mod terrain;

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
    let [r, g, b, _] = bg.to_le_bytes();
    let background = Color::rgb(r, g, b);
    let [r, g, b, _] = fg.to_le_bytes();
    let mixed = Color::lerp(background, Color::rgb(r, g, b), amount);
    packed([mixed.r, mixed.g, mixed.b])
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
#[derive(Clone, Copy)]
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
    pub jets: &'static [[u32; 96]; 3],
    pub outline: u32,
    pub spark: [u32; 2],
    pub foam: u32,
    pub trails: [u32; 3],
}
impl Palette {
    pub fn new(day: Daylight) -> Self {
        *Self::shared(day)
    }

    pub(crate) fn shared(day: Daylight) -> &'static Self {
        &environment::PALETTES[day as usize]
    }
}

const fn channels(rgb: u32) -> [u8; 3] {
    let [_, r, g, b] = rgb.to_be_bytes();
    [r, g, b]
}

const fn opaque(rgb: u32) -> u32 {
    packed(channels(rgb))
}

const fn packed([r, g, b]: [u8; 3]) -> u32 {
    u32::from_le_bytes([r, g, b, 255])
}
