use super::{Daylight, Palette, aircraft, channels, opaque, packed, terrain};
use mirui::types::Fixed;

struct Atmosphere {
    sky: [u32; 3], // Horizon, midpoint, zenith.
    haze: u32,
    light_direction: [Fixed; 2],
    light_disc: u32,
    light_glow: u32,
    aircraft_outline: u32,
    water_glint: [u32; 2],
    foam: u32,
}

const DAY: Atmosphere = Atmosphere {
    sky: [0xC6E8FA, 0x7ABEF0, 0x3E8EDE],
    haze: 0xBCE0F6,
    light_direction: [Fixed::HALF, Fixed::from_ratio(32, 100)],
    light_disc: 0xFFF4C8,
    light_glow: 0xFFDE96,
    aircraft_outline: 0x0D0F1A,
    water_glint: [0x78BEEC, 0xECFAFF],
    foam: 0xF0FCFF,
};

const SUNSET: Atmosphere = Atmosphere {
    sky: [0xFFC684, 0xEE7A82, 0x544496],
    haze: 0xFAB686,
    light_direction: [Fixed::from_ratio(95, 100), Fixed::HALF],
    light_disc: 0xFFF4C8,
    light_glow: 0xFFDE96,
    aircraft_outline: 0x100F1A,
    water_glint: [0xFFAA6E, 0xFFE2AA],
    foam: 0xFFE2D6,
};

const NIGHT: Atmosphere = Atmosphere {
    sky: [0x24386E, 0x121C46, 0x070A22],
    haze: 0x1C2C5C,
    light_direction: [Fixed::from_ratio(3, 10), Fixed::from_ratio(42, 100)],
    light_disc: 0xE8EEFF,
    light_glow: 0x4660A0,
    aircraft_outline: 0x2C4076,
    water_glint: [0x466EBE, 0xAAC8F0],
    foam: 0x6E8CC8,
};

const CLOUD_TOP: [u32; 2] = [0xDAE6F7, 0xFFFFFF];
const CLOUD_WALL: [u32; 2] = [0x9EB6DE, 0xCADAF0];
const CLOUD_UNDER: u32 = 0x8AA0C8;
const TRAIL: [u32; 3] = [0xFFFFFF, 0xD6E8FA, 0xAACCF0];

pub(super) static PALETTES: [Palette; 3] = [
    bake(Daylight::Day, DAY, &aircraft::DAY),
    bake(Daylight::Sunset, SUNSET, &aircraft::SUNSET),
    bake(Daylight::Night, NIGHT, &aircraft::NIGHT),
];

const fn bake(day: Daylight, air: Atmosphere, jets: &'static [[u32; 96]; 3]) -> Palette {
    let mut top = [0; 17];
    let mut wall = [0; 17];
    let mut top_shade = [0; 17];
    let mut wall_shade = [0; 17];
    let mut i = 0;
    while i < top.len() {
        let upper = grade(terrain::TOP[i], day, ONE);
        let side = grade(terrain::WALL[i], day, ONE);
        top[i] = packed(upper);
        wall[i] = packed(side);
        top_shade[i] = shade(upper);
        wall_shade[i] = shade(side);
        i += 1;
    }

    let cloud_grade = if matches!(day, Daylight::Night) {
        percent(72)
    } else {
        ONE
    };
    let [horizon, midpoint, zenith] = air.sky;
    let mut sky = [0; 8];
    let mut cloud_top = [0; 8];
    let mut cloud_wall = [0; 8];
    i = 0;
    while i < sky.len() {
        let height = i as i32 * ONE / 7;
        let colors = if height < ONE / 2 {
            mix([horizon, midpoint], height * 2)
        } else {
            mix([midpoint, zenith], height * 2 - ONE)
        };
        sky[i] = packed(colors);
        let cap = if i < 2 { i as i32 * (ONE / 2) } else { ONE };
        cloud_top[i] = packed(grade(mix(CLOUD_TOP, cap), day, cloud_grade));
        cloud_wall[i] = packed(grade(mix(CLOUD_WALL, height), day, cloud_grade));
        i += 1;
    }
    Palette {
        day,
        top,
        wall,
        top_shade,
        wall_shade,
        cloud_top,
        cloud_wall,
        cloud_under: packed(grade(channels(CLOUD_UNDER), day, cloud_grade)),
        sky,
        haze: opaque(air.haze),
        sun: air.light_direction,
        sun_color: opaque(air.light_disc),
        sun_glow: opaque(air.light_glow),
        jets,
        outline: opaque(air.aircraft_outline),
        spark: [opaque(air.water_glint[0]), opaque(air.water_glint[1])],
        foam: opaque(air.foam),
        trails: [
            packed(grade(channels(TRAIL[0]), day, percent(80))),
            packed(grade(channels(TRAIL[1]), day, percent(80))),
            packed(grade(channels(TRAIL[2]), day, percent(80))),
        ],
    }
}

// Q8 weights preserve the same rounding as the fixed-point frame colors.
const ONE: i32 = 1 << 8;

const fn percent(value: i32) -> i32 {
    value * ONE / 100
}

const fn mix([from, to]: [u32; 2], weight: i32) -> [u8; 3] {
    let a = channels(from);
    let b = channels(to);
    let mut color = [0; 3];
    let mut i = 0;
    while i < color.len() {
        color[i] = ((a[i] as i32 * (ONE - weight) + b[i] as i32 * weight) >> 8) as u8;
        i += 1;
    }
    color
}

const fn grade(color: [u8; 3], day: Daylight, amount: i32) -> [u8; 3] {
    let [r, g, b] = color;
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let target = match day {
        Daylight::Day => return color,
        Daylight::Sunset => [
            r * percent(95) + g * percent(10) + 12 * ONE,
            g * percent(72) + r * percent(5) + 4 * ONE,
            b * percent(70) + r * percent(8) + 10 * ONE,
        ],
        Daylight::Night => [
            r * percent(22) + 2 * ONE,
            g * percent(30) + 6 * ONE,
            b * percent(50) + 22 * ONE,
        ],
    };
    let mut result = [0; 3];
    let mut i = 0;
    while i < result.len() {
        let base = color[i] as i32 * ONE;
        let channel = (base + (((target[i] - base) * amount) >> 8)) >> 8;
        result[i] = match channel {
            0..=255 => channel as u8,
            ..0 => 0,
            _ => 255,
        };
        i += 1;
    }
    result
}

const fn shade([r, g, b]: [u8; 3]) -> u32 {
    packed([
        ((r as i32 * percent(62)) >> 8) as u8,
        ((g as i32 * percent(67)) >> 8) as u8,
        ((b as i32 * percent(80)) >> 8) as u8,
    ])
}
