use mirui::{
    types::Color,
    ui::theme::{ColorToken, Theme},
};
use mirui_sprite_stack::{PreparedSpriteStack, SparseLevel, StackSample};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum SceneTone {
    Day,
    Night,
}

impl SceneTone {
    pub(crate) fn from_theme(theme: &Theme) -> Self {
        let Color { r, g, b, .. } = theme.resolve(ColorToken::Surface);
        let luminance = 54 * u16::from(r) + 183 * u16::from(g) + 19 * u16::from(b);
        if luminance >= 128 * 256 {
            Self::Day
        } else {
            Self::Night
        }
    }

    pub(crate) fn palette(self) -> &'static ScenePalette {
        match self {
            Self::Day => &DAY,
            Self::Night => &NIGHT,
        }
    }
}

const fn rgba(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

const fn swatch(mark: u8) -> usize {
    match mark {
        b'd' => 0,  // Deep steel
        b's' => 1,  // Steel
        b'l' => 2,  // Lit steel
        b'g' => 3,  // Jade glass
        b'm' => 4,  // Mint lamp
        b'i' => 5,  // Ivory
        b'a' => 6,  // Signal red
        b'r' => 7,  // Plum warning
        b'c' => 8,  // Enamel shadow
        b'o' => 9,  // Orange enamel
        b'h' => 10, // Enamel highlight
        b'b' => 11, // Blue windshield
        _ => panic!("unknown material swatch"),
    }
}

pub(crate) struct ScenePalette {
    pub(crate) normal: [[u8; 4]; 12],
    pub(crate) warning: [[u8; 4]; 12],
    pub(crate) locked: [[u8; 4]; 12],
    pub(crate) terrain: [Color; 4],
}

const fn palette(normal: [[u8; 4]; 12], terrain: [Color; 4]) -> ScenePalette {
    let mut warning = normal;
    warning[swatch(b'g')] = normal[swatch(b'c')];
    warning[swatch(b'm')] = normal[swatch(b'r')];
    let mut locked = normal;
    locked[swatch(b'g')] = rgba(terrain[1]);
    locked[swatch(b'm')] = rgba(terrain[2]);
    locked[swatch(b'i')] = rgba(terrain[3]);
    ScenePalette {
        normal,
        warning,
        locked,
        terrain,
    }
}

static DAY: ScenePalette = palette(
    [
        rgba(Color::rgb(73, 68, 60)),
        rgba(Color::rgb(130, 123, 105)),
        rgba(Color::rgb(234, 220, 194)),
        rgba(Color::rgb(33, 105, 83)),
        rgba(Color::rgb(66, 185, 133)),
        rgba(Color::rgb(254, 249, 235)),
        rgba(Color::rgb(175, 55, 45)),
        rgba(Color::rgb(129, 53, 99)),
        rgba(Color::rgb(112, 44, 31)),
        rgba(Color::rgb(207, 70, 41)),
        rgba(Color::rgb(253, 153, 107)),
        rgba(Color::rgb(74, 101, 97)),
    ],
    [
        Color::rgb(196, 211, 196),
        Color::rgb(160, 151, 134),
        Color::rgb(206, 190, 164),
        Color::rgb(224, 210, 185),
    ],
);

static NIGHT: ScenePalette = palette(
    [
        rgba(Color::rgb(44, 38, 35)),
        rgba(Color::rgb(80, 70, 61)),
        rgba(Color::rgb(188, 160, 130)),
        rgba(Color::rgb(30, 113, 85)),
        rgba(Color::rgb(121, 218, 161)),
        rgba(Color::rgb(247, 232, 209)),
        rgba(Color::rgb(218, 105, 86)),
        rgba(Color::rgb(223, 127, 176)),
        rgba(Color::rgb(138, 54, 42)),
        rgba(Color::rgb(227, 91, 54)),
        rgba(Color::rgb(255, 176, 134)),
        rgba(Color::rgb(153, 192, 170)),
    ],
    [
        Color::rgb(43, 65, 55),
        Color::rgb(77, 69, 60),
        Color::rgb(96, 85, 71),
        Color::rgb(113, 101, 83),
    ],
);

const EMPTY: StackSample = StackSample {
    x: 0,
    y: 0,
    color: 0,
};

struct Cells<const SIZE: usize, const OFFSETS: usize> {
    cells: [StackSample; SIZE],
    offsets: [usize; OFFSETS],
    used: usize,
}

const fn pack<
    const W: usize,
    const D: usize,
    const H: usize,
    const SIZE: usize,
    const OFFSETS: usize,
>(
    slices: [[[u8; W]; D]; H],
    samples_per_layer: usize,
) -> Cells<SIZE, OFFSETS> {
    let mut data = Cells {
        cells: [EMPTY; SIZE],
        offsets: [0; OFFSETS],
        used: 0,
    };
    let mut y = 0;
    while y < H * samples_per_layer {
        let mut row = 0;
        while row < D {
            let mut x = 0;
            while x < W {
                let material = slices[y / samples_per_layer][row][x];
                if material != b'.' {
                    data.cells[data.used] = StackSample {
                        x: x as u8,
                        y: row as u8,
                        color: swatch(material) as u8,
                    };
                    data.used += 1;
                }
                x += 1;
            }
            row += 1;
        }
        data.offsets[y + 1] = data.used;
        y += 1;
    }
    data
}

macro_rules! sprite {
    ($name:ident, $w:literal, $d:literal, $h:literal, $slices:expr) => {
        pub static $name: [PreparedSpriteStack<'static>; 2] = {
            const SLICES: [[[u8; $w]; $d]; $h] = $slices;
            [
                prepare!(SLICES, $w, $d, $h, $w * $d * $h, [$w * 128, $d * 128, 0], 1),
                prepare!(SLICES, $w, $d, $h, $w * $d * $h, [$w * 128, $d * 128, 0], 2),
            ]
        };
    };
}

macro_rules! prepare {
    ($slices:expr, $w:expr, $d:expr, $h:expr, $capacity:expr, $pivot:expr, $samples:literal) => {{
        const HEIGHT: usize = $h * $samples;
        const DATA: Cells<{ ($capacity) * $samples }, { HEIGHT + 1 }> = pack($slices, $samples);
        const LEVELS: [SparseLevel<'static>; 1] =
            [SparseLevel::new(1, &DATA.offsets, DATA.cells.split_at(DATA.used).0).unwrap()];
        PreparedSpriteStack::new(
            [$w as u16, $d as u16, HEIGHT as u16],
            $pivot,
            256 / $samples,
            &DAY.normal,
            &LEVELS,
        )
        .expect("compile-time sprite cells")
    }};
}

// The bow points toward +Z. The belly, enamel deck and raised cabin have distinct silhouettes.
sprite!(
    SHIP,
    9,
    11,
    6,
    [
        [
            *b".........",
            *b"....d....",
            *b"...ddd...",
            *b"..ddddd..",
            *b".ddddddd.",
            *b".ddddddd.",
            *b".ddddddd.",
            *b".ddddddd.",
            *b".dd...dd.",
            *b".dd...dd.",
            *b"........."
        ],
        [
            *b"....c....",
            *b"...coc...",
            *b"..coooc..",
            *b".coooooc.",
            *b"coooooooc",
            *b"coooooooc",
            *b"coooooooc",
            *b"coosssooc",
            *b"cos...soc",
            *b"cs.....sc",
            *b".m.....m."
        ],
        [
            *b"....h....",
            *b"...hoh...",
            *b"..hoooh..",
            *b".hoaaaoh.",
            *b"hooosoooh",
            *b"hoosssooh",
            *b"hoosssooh",
            *b"hoosssooh",
            *b"ho.....oh",
            *b".l.....l.",
            *b"........."
        ],
        [
            *b".........",
            *b".........",
            *b".........",
            *b"....b....",
            *b"...bbb...",
            *b"...bbb...",
            *b"...dbd...",
            *b"...ddd...",
            *b".........",
            *b".........",
            *b"........."
        ],
        [
            *b".........",
            *b".........",
            *b".........",
            *b".........",
            *b"...bib...",
            *b"...bbb...",
            *b"...ddd...",
            *b"...lsl...",
            *b".........",
            *b".........",
            *b"........."
        ],
        [
            *b".........",
            *b".........",
            *b".........",
            *b".........",
            *b".........",
            *b"....l....",
            *b"....i....",
            *b".........",
            *b".........",
            *b".........",
            *b"........."
        ],
    ]
);

sprite!(
    CORE,
    5,
    5,
    6,
    [
        [*b".....", *b".sss.", *b".sss.", *b".sss.", *b"....."],
        [*b"..l..", *b".ggg.", *b"lgggl", *b".ggg.", *b"..l.."],
        [*b"..m..", *b".gmg.", *b"mgigm", *b".gmg.", *b"..m.."],
        [*b"..m..", *b".gmg.", *b"mmimm", *b".gmg.", *b"..m.."],
        [*b".....", *b".gmg.", *b".mim.", *b".gmg.", *b"....."],
        [*b".....", *b".....", *b"..i..", *b".....", *b"....."],
    ]
);

sprite!(
    GUARD,
    9,
    9,
    5,
    [
        [
            *b".........",
            *b"....d....",
            *b"...ddd...",
            *b"..ddddd..",
            *b".ddddddd.",
            *b".ddddddd.",
            *b".ddd.ddd.",
            *b".........",
            *b"........."
        ],
        [
            *b"....s....",
            *b"...sss...",
            *b"..sssss..",
            *b".sssssss.",
            *b"dsssssssd",
            *b"sssssssss",
            *b"sss...sss",
            *b".s.....s.",
            *b"........."
        ],
        [
            *b".........",
            *b"....r....",
            *b"...rrr...",
            *b"..rsssr..",
            *b".rsssssr.",
            *b".sslllss.",
            *b".sr...rs.",
            *b".........",
            *b"........."
        ],
        [
            *b".........",
            *b".........",
            *b".........",
            *b"...rir...",
            *b"...rrr...",
            *b"...lll...",
            *b".........",
            *b".........",
            *b"........."
        ],
        [
            *b".........",
            *b".........",
            *b".........",
            *b".........",
            *b"....i....",
            *b".........",
            *b".........",
            *b".........",
            *b"........."
        ],
    ]
);

sprite!(LINK, 2, 2, 1, [[*b"mi", *b"im"]]);
sprite!(MARK, 2, 2, 1, [[*b"ar", *b"ra"]]);
sprite!(
    GATE,
    9,
    5,
    7,
    [
        [
            *b"sssssssss",
            *b"sddddddds",
            *b"sdgggggds",
            *b"sddddddds",
            *b"sssssssss"
        ],
        [
            *b"ls.....sl",
            *b"ss.....ss",
            *b"ss.....ss",
            *b"ss.....ss",
            *b"ls.....sl"
        ],
        [
            *b".........",
            *b"ds.....sd",
            *b"gm.....mg",
            *b"ds.....sd",
            *b"........."
        ],
        [
            *b".........",
            *b"ds.....sd",
            *b"gm.....mg",
            *b"ds.....sd",
            *b"........."
        ],
        [
            *b".........",
            *b"ds.....sd",
            *b"gm.....mg",
            *b"ds.....sd",
            *b"........."
        ],
        [
            *b".........",
            *b"sssssssss",
            *b"dgggggggd",
            *b"sssssssss",
            *b"........."
        ],
        [
            *b".........",
            *b".lilmlil.",
            *b".lllllll.",
            *b".........",
            *b"........."
        ],
    ]
);

const BRIDGE_W: usize = 18;
const BRIDGE_D: usize = 52;
const BRIDGE_H: usize = 17;

const fn bridge_slices() -> [[[u8; BRIDGE_W]; BRIDGE_D]; BRIDGE_H] {
    let mut slices = [[[b'.'; BRIDGE_W]; BRIDGE_D]; BRIDGE_H];
    let mut distance = 0;
    while distance < BRIDGE_D {
        let row = BRIDGE_D - 1 - distance;
        let end = BRIDGE_D - 1 - distance;
        let deck = if distance < end { distance } else { end }.saturating_sub(1);
        let deck = if deck > 7 { 7 } else { deck };
        let mut x = 0;
        while x < BRIDGE_W {
            let edge = x == 0 || x == BRIDGE_W - 1;
            slices[deck][row][x] = if edge {
                b'l'
            } else if distance == 8 || distance == BRIDGE_D - 1 {
                if x % 2 == 0 { b'a' } else { b'd' }
            } else if distance % 9 < 3 && (x == 8 || x == 9) {
                b'l'
            } else if (x + 2 * (distance / 6)) % 13 == 0 {
                b's'
            } else {
                b'd'
            };
            if edge && deck > 0 {
                slices[deck - 1][row][x] = b's';
                if distance % 5 == 0 {
                    slices[deck + 1][row][x] = b'l';
                }
            }
            // The two crane portals clear the skiff and stand outside the driving lane.
            if (17 <= distance && distance <= 19) || (37 <= distance && distance <= 39) {
                if x < 2 || x >= BRIDGE_W - 2 {
                    let mut y = 0;
                    while y < BRIDGE_H - 1 {
                        if y != deck {
                            slices[y][row][x] = if edge { b's' } else { b'l' };
                        }
                        y += 1;
                    }
                }
                slices[BRIDGE_H - 1][row][x] = if distance % 2 == 0 && x % 4 < 2 {
                    b'a'
                } else {
                    b'l'
                };
                if (x == 7 || x == 8) && (distance == 18 || distance == 38) {
                    slices[BRIDGE_H - 3][row][x] = b'o';
                    slices[BRIDGE_H - 2][row][x] = b'd';
                }
            }
            x += 1;
        }
        distance += 1;
    }
    slices
}

pub static BRIDGE: [PreparedSpriteStack<'static>; 2] = {
    const SLICES: [[[u8; BRIDGE_W]; BRIDGE_D]; BRIDGE_H] = bridge_slices();
    const PIVOT: [i32; 3] = [BRIDGE_W as i32 * 128, (BRIDGE_D as i32 - 1) * 256, 0];
    const CAPACITY: usize = BRIDGE_W * BRIDGE_D * 2;
    [
        prepare!(SLICES, BRIDGE_W, BRIDGE_D, BRIDGE_H, CAPACITY, PIVOT, 1),
        prepare!(SLICES, BRIDGE_W, BRIDGE_D, BRIDGE_H, CAPACITY, PIVOT, 2),
    ]
};
