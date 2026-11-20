use crate::{jet, world};
use mirui::types::{Color, Fixed};

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
    u32::from_le_bytes([mixed.r, mixed.g, mixed.b, 255])
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
