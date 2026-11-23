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

    pub(crate) fn passage_cover(self, alpha: u8) -> Color {
        match self {
            Self::Day => Color::rgba(241, 234, 219, alpha),
            Self::Night => Color::rgba(27, 23, 22, alpha),
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
    pub(crate) terrain: &'static [Color],
}

const fn palette(normal: [[u8; 4]; 12], terrain: &'static [Color]) -> ScenePalette {
    let mut warning = normal;
    warning[swatch(b'g')] = normal[swatch(b'c')];
    warning[swatch(b'm')] = normal[swatch(b'r')];
    let mut locked = normal;
    locked[swatch(b'g')] = rgba(terrain[Material::Curb as usize]);
    locked[swatch(b'm')] = rgba(terrain[Material::Road as usize]);
    locked[swatch(b'i')] = rgba(terrain[Material::Concrete as usize]);
    ScenePalette {
        normal,
        warning,
        locked,
        terrain,
    }
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Material {
    Water,
    Curb,
    Road,
    Concrete,
    Patina,
    Aggregate,
    Joint,
    Paint,
    Roof,
    RoofEdge,
    Freight,
    Slip,
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
    &[
        Color::rgb(72, 135, 128),  // Water
        Color::rgb(160, 151, 134), // Curb
        Color::rgb(149, 145, 132), // Asphalt
        Color::rgb(224, 210, 185), // Concrete
        Color::rgb(210, 195, 172), // Patina
        Color::rgb(231, 217, 194), // Aggregate
        Color::rgb(192, 177, 155), // Expansion joint
        Color::rgb(251, 235, 203), // Safety paint
        Color::rgb(153, 132, 114), // Roof
        Color::rgb(122, 104, 92),  // Roof edging
        Color::rgb(179, 187, 161), // Freight apron
        Color::rgba(0, 0, 0, 0),   // Recessed slip
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
    &[
        Color::rgb(31, 68, 67),
        Color::rgb(126, 109, 89),
        Color::rgb(75, 78, 70),
        Color::rgb(113, 101, 83),
        Color::rgb(101, 90, 76),
        Color::rgb(122, 110, 91),
        Color::rgb(88, 77, 65),
        Color::rgb(181, 158, 128),
        Color::rgb(105, 74, 62),
        Color::rgb(79, 56, 50),
        Color::rgb(80, 103, 84),
        Color::rgba(0, 0, 0, 0),
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
        const LAYER_COUNT: usize = $h * $samples;
        const DATA: Cells<{ ($capacity) * $samples }, { LAYER_COUNT + 1 }> =
            pack($slices, $samples);
        const LEVELS: [SparseLevel<'static>; 1] =
            [SparseLevel::new(1, &DATA.offsets, DATA.cells.split_at(DATA.used).0).unwrap()];
        PreparedSpriteStack::new(
            [$w as u16, $d as u16, LAYER_COUNT as u16],
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

const BRIDGE_W: usize = crate::world::CELL_SIZE as usize * 2;

const fn bridge_slices<const D: usize>() -> [[[u8; BRIDGE_W]; D]; 4] {
    let mut slices = [[[b'.'; BRIDGE_W]; D]; 4];
    let mut z = 0;
    while z < D {
        let end = z == 0 || z == D - 1;
        let mut x = 0;
        while x < BRIDGE_W {
            let edge = x == 0 || x == BRIDGE_W - 1;
            slices[1][z][x] = if edge {
                b'l'
            } else if end {
                if x % 4 < 2 { b'a' } else { b'd' }
            } else if z % 6 < 2 && (x == BRIDGE_W / 2) {
                b'i'
            } else if z % 3 == 0 {
                b's'
            } else {
                b'd'
            };
            if edge {
                slices[0][z][x] = b'd';
                slices[2][z][x] = b's';
                if z % 4 == 0 || end {
                    slices[3][z][x] = b'l';
                }
            }
            x += 1;
        }
        z += 1;
    }
    slices
}

sprite!(SHORT_BRIDGE, 12, 12, 4, bridge_slices());
sprite!(LONG_BRIDGE, 12, 18, 4, bridge_slices());

pub(super) fn bridge_model(
    bridge: crate::world::Bridge,
    detail: usize,
) -> &'static PreparedSpriteStack<'static> {
    if bridge.length() == 12 {
        &SHORT_BRIDGE[detail]
    } else {
        &LONG_BRIDGE[detail]
    }
}

pub static PORTAL: [PreparedSpriteStack<'static>; 2] = {
    const WIDTH: usize = 13;
    const HEIGHT: usize = 11;
    const DEPTH: usize = 6;
    const SLICES: [[[u8; WIDTH]; DEPTH]; HEIGHT] = {
        let mut slices = [[[b'.'; WIDTH]; DEPTH]; HEIGHT];
        let mut y = 0;
        while y < HEIGHT {
            let mut z = 0;
            while z < DEPTH {
                let mut x = 0;
                while x < WIDTH {
                    let pillar = x < 2 || x >= WIDTH - 2;
                    slices[y][z][x] = if y == 0 && z > 0 && z < DEPTH - 1 {
                        if z == 2 { b'g' } else { b'd' }
                    } else if pillar && y > 0 && y < HEIGHT - 1 {
                        if x % (WIDTH - 1) == 0 {
                            b's'
                        } else if z == 2 {
                            b'm'
                        } else {
                            b'd'
                        }
                    } else if y >= HEIGHT - 2 && z > 0 && z < DEPTH - 1 {
                        if y == HEIGHT - 1 {
                            b'l'
                        } else if x % 3 == 0 {
                            b'm'
                        } else {
                            b'g'
                        }
                    } else {
                        b'.'
                    };
                    x += 1;
                }
                z += 1;
            }
            y += 1;
        }
        slices
    };
    const PIVOT: [i32; 3] = [WIDTH as i32 * 128, DEPTH as i32 * 128, 0];
    const CAPACITY: usize = WIDTH * DEPTH * HEIGHT;
    [
        prepare!(SLICES, WIDTH, DEPTH, HEIGHT, CAPACITY, PIVOT, 1),
        prepare!(SLICES, WIDTH, DEPTH, HEIGHT, CAPACITY, PIVOT, 2),
    ]
};

// Painted containers with dark skid feet, pale twist locks and corrugated sides.
sprite!(
    CARGO,
    5,
    6,
    4,
    [
        [
            *b"d...d", *b".....", *b".....", *b".....", *b".....", *b"d...d"
        ],
        [
            *b"ccccc", *b"coooc", *b"coooc", *b"coooc", *b"coooc", *b"ccccc"
        ],
        [
            *b"lcocl", *b"hoooa", *b"coooc", *b"hoooa", *b"coooc", *b"lcocl"
        ],
        [
            *b"lcccl", *b"hoooh", *b"coooc", *b"hoooh", *b"coooc", *b"lcccl"
        ],
    ]
);

// Roof plant with paired vents and an exposed air intake.
sprite!(
    VENT,
    5,
    5,
    3,
    [
        [*b"ddddd", *b"dsssd", *b"dsssd", *b"dsssd", *b"ddddd"],
        [*b".....", *b".lgl.", *b".sgs.", *b".lgl.", *b"....."],
        [*b".....", *b".lgl.", *b".ldl.", *b".lgl.", *b"....."],
    ]
);

// A quay beacon on a square cast base. The pale cap stays legible in either theme.
sprite!(
    BEACON,
    3,
    3,
    7,
    [
        [*b"ddd", *b"dsd", *b"ddd"],
        [*b"...", *b".s.", *b"..."],
        [*b"...", *b".s.", *b"..."],
        [*b"...", *b".l.", *b"..."],
        [*b".l.", *b"lsl", *b".l."],
        [*b".a.", *b"aia", *b".a."],
        [*b"...", *b".i.", *b"..."],
    ]
);
