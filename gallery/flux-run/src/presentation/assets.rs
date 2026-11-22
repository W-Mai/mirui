use mirui::types::Color;
use mirui_sprite_stack::{PreparedSpriteStack, SparseLevel, StackSample};

pub const INK: Color = Color::rgb(13, 19, 32);
pub const PANEL: Color = Color::rgb(22, 32, 48);
pub const WATER: Color = Color::rgb(23, 57, 73);
pub const FOAM: Color = Color::rgb(42, 87, 98);
pub const STEEL: Color = Color::rgb(65, 76, 91);
pub const BRIGHT: Color = Color::rgb(222, 246, 241);
pub const CYAN: Color = Color::rgb(111, 239, 217);
pub const AMBER: Color = Color::rgb(255, 166, 70);
pub const RED: Color = Color::rgb(255, 94, 90);

const fn rgba(color: Color) -> [u8; 4] {
    [color.r, color.g, color.b, color.a]
}

pub static ART: [[u8; 4]; 7] = [
    rgba(INK),
    rgba(STEEL),
    rgba(CYAN),
    rgba(BRIGHT),
    rgba(AMBER),
    rgba(RED),
    rgba(FOAM),
];
pub static WARNING: [[u8; 4]; 7] = [
    rgba(INK),
    rgba(STEEL),
    rgba(RED),
    rgba(AMBER),
    rgba(AMBER),
    rgba(RED),
    rgba(FOAM),
];
pub static LOCKED: [[u8; 4]; 7] = [
    rgba(INK),
    rgba(STEEL),
    rgba(FOAM),
    rgba(STEEL),
    rgba(AMBER),
    rgba(RED),
    rgba(FOAM),
];

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
    const H: usize,
    const L: usize,
    const SIZE: usize,
    const OFFSETS: usize,
>(
    rows: [[[u8; W]; H]; L],
) -> Cells<SIZE, OFFSETS> {
    let mut data = Cells {
        cells: [EMPTY; SIZE],
        offsets: [0; OFFSETS],
        used: 0,
    };
    let mut layer = 0;
    while layer < L {
        let mut y = 0;
        while y < H {
            let mut x = 0;
            while x < W {
                let entry = rows[layer][y][x];
                if entry != b'.' {
                    data.cells[data.used] = StackSample {
                        x: x as u8,
                        y: y as u8,
                        color: entry - b'0',
                    };
                    data.used += 1;
                }
                x += 1;
            }
            y += 1;
        }
        data.offsets[layer + 1] = data.used;
        layer += 1;
    }
    data
}

macro_rules! sprite {
    ($name:ident, $w:literal, $h:literal, $layers:literal, $rows:expr) => {
        pub static $name: PreparedSpriteStack<'static> = {
            const DATA: Cells<{ $w * $h * $layers }, { $layers + 1 }> = pack($rows);
            const LEVEL: SparseLevel<'static> =
                SparseLevel::new(1, &DATA.offsets, DATA.cells.split_at(DATA.used).0).unwrap();
            PreparedSpriteStack::new(
                [$w, $h, $layers],
                [$w * 128, $h * 128, 0],
                256,
                &ART,
                &[LEVEL],
            )
            .expect("compile-time sprite cells")
        };
    };
}

sprite!(
    SHIP,
    7,
    7,
    3,
    [
        [
            *b"..111..",
            *b".11111.",
            *b"1111111",
            *b"1111111",
            *b"1111111",
            *b".11111.",
            *b"..111.."
        ],
        [
            *b"...2...",
            *b"..242..",
            *b".44444.",
            *b"1443441",
            *b"1444441",
            *b".66066.",
            *b"..2.2.."
        ],
        [
            *b".......",
            *b"...3...",
            *b"..232..",
            *b"..222..",
            *b"...1...",
            *b".......",
            *b"......."
        ],
    ]
);
sprite!(
    CORE,
    5,
    5,
    3,
    [
        [*b".....", *b".121.", *b".222.", *b".121.", *b"....."],
        [*b"..2..", *b".232.", *b"23332", *b".232.", *b"..2.."],
        [*b".....", *b"..2..", *b".232.", *b"..2..", *b"....."],
    ]
);
sprite!(
    GUARD,
    7,
    5,
    3,
    [
        [
            *b".11111.",
            *b"1111111",
            *b"1111111",
            *b"1111111",
            *b".11111."
        ],
        [
            *b".15151.",
            *b"1503051",
            *b"1502051",
            *b"1500051",
            *b".15151."
        ],
        [
            *b".......",
            *b"..151..",
            *b"..525..",
            *b"..151..",
            *b"......."
        ],
    ]
);
sprite!(LINK, 2, 2, 1, [[*b"23", *b"32"]]);
sprite!(MARK, 2, 2, 1, [[*b"45", *b"54"]]);
sprite!(
    GATE,
    9,
    5,
    2,
    [
        [
            *b"222222222",
            *b"211111112",
            *b"216666612",
            *b"211111112",
            *b"222222222"
        ],
        [
            *b"33.....33",
            *b"22.....22",
            *b"22.....22",
            *b"22.....22",
            *b"33.....33"
        ],
    ]
);
sprite!(
    DECK,
    14,
    8,
    1,
    [[
        *b"14444444444441",
        *b"16111111111161",
        *b"16111111111161",
        *b"16111111111161",
        *b"16111111111161",
        *b"16111111111161",
        *b"16111111111161",
        *b"14444444444441",
    ]]
);
