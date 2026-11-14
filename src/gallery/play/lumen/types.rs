pub(crate) const GRID_COLUMNS: i8 = 7;
pub(crate) const GRID_ROWS: i8 = 5;
/// Puzzles per set; each set adds one idea and opens with a puzzle that teaches it.
pub(crate) const SET_SIZE: usize = 5;
pub(crate) const SET_COUNT: usize = 3;
pub(crate) const LEVEL_COUNT: usize = SET_SIZE * SET_COUNT;
/// Rotatable mirrors per puzzle; their orientations fit one history byte.
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

/// A mirror the player turns, with the orientation the puzzle needs.
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

/// A mirror bolted to the board: it reflects but never turns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FixedMirror {
    pub(crate) position: GridPoint,
    pub(crate) orientation: MirrorOrientation,
}

impl FixedMirror {
    pub(super) const fn new(x: i8, y: i8, orientation: MirrorOrientation) -> Self {
        Self {
            position: GridPoint::new(x, y),
            orientation,
        }
    }
}

/// One puzzle. Rotatable mirrors are listed in reading order, so mirror `i`
/// wears the label `i + 1` on the board.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Level {
    pub(crate) name: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) source_row: i8,
    pub(crate) target: GridPoint,
    pub(crate) mirrors: &'static [MirrorSpec],
    pub(crate) fixed: &'static [FixedMirror],
    pub(crate) walls: &'static [GridPoint],
    /// Cells the beam must cross before the receiver counts.
    pub(crate) lamps: &'static [GridPoint],
    pub(crate) initial: &'static [MirrorOrientation],
}

impl Level {
    pub(crate) fn mirror_at(&self, point: GridPoint) -> Option<usize> {
        self.mirrors
            .iter()
            .position(|mirror| mirror.position == point)
    }

    pub(crate) fn fixed_at(&self, point: GridPoint) -> Option<MirrorOrientation> {
        self.fixed
            .iter()
            .find(|mirror| mirror.position == point)
            .map(|mirror| mirror.orientation)
    }

    pub(crate) fn lamp_at(&self, point: GridPoint) -> Option<usize> {
        self.lamps.iter().position(|lamp| *lamp == point)
    }

    /// The lamp bits a solving beam lights.
    pub(crate) const fn all_lamps(&self) -> u8 {
        ((1_u16 << self.lamps.len()) - 1) as u8
    }
}

/// A group of puzzles that shares one idea.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LevelSet {
    pub(crate) name: &'static str,
    pub(crate) rule: &'static str,
}

pub(crate) const SETS: [LevelSet; SET_COUNT] = [
    LevelSet {
        name: "转镜",
        rule: "点镜子换方向",
    },
    LevelSet {
        name: "定镜",
        rule: "有的镜子转不动",
    },
    LevelSet {
        name: "点灯",
        rule: "先点亮所有灯",
    },
];
