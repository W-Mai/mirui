pub(crate) const GRID_COLUMNS: i8 = 7;
pub(crate) const GRID_ROWS: i8 = 5;
pub(crate) const LEVEL_COUNT: usize = 5;
pub(crate) const MAX_MIRRORS: usize = 7;
pub(crate) const MAX_TRACE_POINTS: usize = 141;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct GridPoint {
    pub(crate) x: i8,
    pub(crate) y: i8,
}

impl GridPoint {
    pub(super) const fn new(x: i8, y: i8) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum MirrorOrientation {
    #[default]
    Slash,
    Backslash,
}

impl MirrorOrientation {
    pub(super) const fn toggled(self) -> Self {
        match self {
            Self::Slash => Self::Backslash,
            Self::Backslash => Self::Slash,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MirrorSpec {
    pub(crate) position: GridPoint,
    pub(crate) solution: MirrorOrientation,
}

impl MirrorSpec {
    pub(super) const fn new(x: i8, y: i8, solution: MirrorOrientation) -> Self {
        Self {
            position: GridPoint::new(x, y),
            solution,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Level {
    pub(crate) name: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) source_row: i8,
    pub(crate) target: GridPoint,
    pub(crate) mirrors: &'static [MirrorSpec],
    pub(crate) walls: &'static [GridPoint],
    pub(crate) initial: &'static [MirrorOrientation],
}

const L1_MIRRORS: [MirrorSpec; 2] = [
    MirrorSpec::new(2, 3, MirrorOrientation::Slash),
    MirrorSpec::new(2, 1, MirrorOrientation::Slash),
];
const L1_WALLS: [GridPoint; 2] = [GridPoint::new(4, 3), GridPoint::new(4, 4)];
const L1_INITIAL: [MirrorOrientation; 2] = [MirrorOrientation::Backslash, MirrorOrientation::Slash];

const L2_MIRRORS: [MirrorSpec; 3] = [
    MirrorSpec::new(1, 0, MirrorOrientation::Backslash),
    MirrorSpec::new(1, 4, MirrorOrientation::Backslash),
    MirrorSpec::new(5, 4, MirrorOrientation::Slash),
];
const L2_WALLS: [GridPoint; 2] = [GridPoint::new(3, 2), GridPoint::new(3, 3)];
const L2_INITIAL: [MirrorOrientation; 3] = [
    MirrorOrientation::Slash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
];

const L3_MIRRORS: [MirrorSpec; 4] = [
    MirrorSpec::new(0, 2, MirrorOrientation::Slash),
    MirrorSpec::new(0, 0, MirrorOrientation::Slash),
    MirrorSpec::new(3, 0, MirrorOrientation::Backslash),
    MirrorSpec::new(3, 4, MirrorOrientation::Backslash),
];
const L3_WALLS: [GridPoint; 2] = [GridPoint::new(1, 3), GridPoint::new(5, 2)];
const L3_INITIAL: [MirrorOrientation; 4] = [
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Slash,
];

const L4_MIRRORS: [MirrorSpec; 6] = [
    MirrorSpec::new(2, 4, MirrorOrientation::Slash),
    MirrorSpec::new(2, 1, MirrorOrientation::Slash),
    MirrorSpec::new(5, 1, MirrorOrientation::Backslash),
    MirrorSpec::new(5, 3, MirrorOrientation::Slash),
    MirrorSpec::new(3, 3, MirrorOrientation::Backslash),
    MirrorSpec::new(3, 0, MirrorOrientation::Slash),
];
const L4_WALLS: [GridPoint; 2] = [GridPoint::new(0, 0), GridPoint::new(6, 4)];
const L4_INITIAL: [MirrorOrientation; 6] = [
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Slash,
    MirrorOrientation::Backslash,
];

const L5_MIRRORS: [MirrorSpec; 7] = [
    MirrorSpec::new(5, 0, MirrorOrientation::Backslash),
    MirrorSpec::new(5, 4, MirrorOrientation::Slash),
    MirrorSpec::new(1, 4, MirrorOrientation::Backslash),
    MirrorSpec::new(1, 1, MirrorOrientation::Slash),
    MirrorSpec::new(4, 1, MirrorOrientation::Backslash),
    MirrorSpec::new(4, 3, MirrorOrientation::Slash),
    MirrorSpec::new(2, 3, MirrorOrientation::Backslash),
];
const L5_WALLS: [GridPoint; 2] = [GridPoint::new(0, 3), GridPoint::new(6, 2)];
const L5_INITIAL: [MirrorOrientation; 7] = [
    MirrorOrientation::Slash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Slash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Slash,
    MirrorOrientation::Backslash,
    MirrorOrientation::Slash,
];

pub(crate) const LEVELS: [Level; LEVEL_COUNT] = [
    Level {
        name: "第一束光",
        subtitle: "先让光向上，再送向右边。",
        source_row: 3,
        target: GridPoint::new(5, 1),
        mirrors: &L1_MIRRORS,
        walls: &L1_WALLS,
        initial: &L1_INITIAL,
    },
    Level {
        name: "折返航线",
        subtitle: "向下走，也能找到出口。",
        source_row: 0,
        target: GridPoint::new(5, 1),
        mirrors: &L2_MIRRORS,
        walls: &L2_WALLS,
        initial: &L2_INITIAL,
    },
    Level {
        name: "绕过岛屿",
        subtitle: "从上方绕一圈，再回到终点。",
        source_row: 2,
        target: GridPoint::new(6, 4),
        mirrors: &L3_MIRRORS,
        walls: &L3_WALLS,
        initial: &L3_INITIAL,
    },
    Level {
        name: "交错的光",
        subtitle: "光线可以相交，不会互相阻挡。",
        source_row: 4,
        target: GridPoint::new(6, 0),
        mirrors: &L4_MIRRORS,
        walls: &L4_WALLS,
        initial: &L4_INITIAL,
    },
    Level {
        name: "最后一公里",
        subtitle: "七面镜片，织出最后一条路径。",
        source_row: 0,
        target: GridPoint::new(2, 2),
        mirrors: &L5_MIRRORS,
        walls: &L5_WALLS,
        initial: &L5_INITIAL,
    },
];
