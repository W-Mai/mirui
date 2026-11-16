//! The fifteen puzzles, in three sets of five. `tests.rs` proves that each
//! has exactly one solution and that every mirror and lamp on it matters.

use super::types::MirrorOrientation::{Backslash, Slash};
use super::types::{FixedMirror, GridPoint, Level, MirrorOrientation, MirrorSpec};

const L1_1_MIRRORS: [MirrorSpec; 2] = [MirrorSpec::new(2, 1, Slash), MirrorSpec::new(2, 3, Slash)];
const L1_1_WALLS: [GridPoint; 2] = [GridPoint::new(4, 3), GridPoint::new(4, 4)];
const L1_1_INITIAL: [MirrorOrientation; 2] = [Slash, Backslash];

const L1_2_MIRRORS: [MirrorSpec; 3] = [
    MirrorSpec::new(1, 0, Backslash),
    MirrorSpec::new(1, 4, Backslash),
    MirrorSpec::new(5, 4, Slash),
];
const L1_2_WALLS: [GridPoint; 2] = [GridPoint::new(3, 2), GridPoint::new(3, 3)];
const L1_2_INITIAL: [MirrorOrientation; 3] = [Slash, Backslash, Backslash];

const L1_3_MIRRORS: [MirrorSpec; 4] = [
    MirrorSpec::new(0, 0, Slash),
    MirrorSpec::new(3, 0, Backslash),
    MirrorSpec::new(0, 2, Slash),
    MirrorSpec::new(3, 4, Backslash),
];
const L1_3_WALLS: [GridPoint; 2] = [GridPoint::new(1, 3), GridPoint::new(5, 2)];
const L1_3_INITIAL: [MirrorOrientation; 4] = [Backslash, Backslash, Backslash, Slash];

const L1_4_MIRRORS: [MirrorSpec; 6] = [
    MirrorSpec::new(3, 0, Slash),
    MirrorSpec::new(2, 1, Slash),
    MirrorSpec::new(5, 1, Backslash),
    MirrorSpec::new(3, 3, Backslash),
    MirrorSpec::new(5, 3, Slash),
    MirrorSpec::new(2, 4, Slash),
];
const L1_4_WALLS: [GridPoint; 2] = [GridPoint::new(0, 0), GridPoint::new(6, 4)];
const L1_4_INITIAL: [MirrorOrientation; 6] =
    [Backslash, Backslash, Backslash, Slash, Backslash, Backslash];

const L1_5_MIRRORS: [MirrorSpec; 7] = [
    MirrorSpec::new(5, 0, Backslash),
    MirrorSpec::new(1, 1, Slash),
    MirrorSpec::new(4, 1, Backslash),
    MirrorSpec::new(2, 3, Backslash),
    MirrorSpec::new(4, 3, Slash),
    MirrorSpec::new(1, 4, Backslash),
    MirrorSpec::new(5, 4, Slash),
];
const L1_5_WALLS: [GridPoint; 2] = [GridPoint::new(0, 3), GridPoint::new(6, 2)];
const L1_5_INITIAL: [MirrorOrientation; 7] =
    [Slash, Backslash, Slash, Slash, Backslash, Slash, Backslash];

const L2_1_MIRRORS: [MirrorSpec; 2] = [
    MirrorSpec::new(3, 0, Slash),
    MirrorSpec::new(6, 0, Backslash),
];
const L2_1_FIXED: [FixedMirror; 1] = [FixedMirror::new(3, 4, Slash)];
const L2_1_INITIAL: [MirrorOrientation; 2] = [Backslash, Slash];

const L2_2_MIRRORS: [MirrorSpec; 3] = [
    MirrorSpec::new(3, 0, Slash),
    MirrorSpec::new(6, 0, Backslash),
    MirrorSpec::new(3, 4, Backslash),
];
const L2_2_FIXED: [FixedMirror; 2] = [
    FixedMirror::new(3, 3, Backslash),
    FixedMirror::new(6, 4, Slash),
];
const L2_2_INITIAL: [MirrorOrientation; 3] = [Slash, Slash, Slash];

const L2_3_MIRRORS: [MirrorSpec; 4] = [
    MirrorSpec::new(0, 1, Slash),
    MirrorSpec::new(0, 4, Backslash),
    MirrorSpec::new(2, 4, Slash),
    MirrorSpec::new(4, 4, Backslash),
];
const L2_3_FIXED: [FixedMirror; 2] = [
    FixedMirror::new(2, 0, Backslash),
    FixedMirror::new(4, 1, Backslash),
];
const L2_3_INITIAL: [MirrorOrientation; 4] = [Slash, Slash, Backslash, Slash];

const L2_4_MIRRORS: [MirrorSpec; 5] = [
    MirrorSpec::new(2, 0, Slash),
    MirrorSpec::new(6, 0, Backslash),
    MirrorSpec::new(0, 2, Slash),
    MirrorSpec::new(2, 2, Slash),
    MirrorSpec::new(3, 2, Slash),
];
const L2_4_FIXED: [FixedMirror; 3] = [
    FixedMirror::new(6, 2, Slash),
    FixedMirror::new(0, 3, Backslash),
    FixedMirror::new(3, 4, Slash),
];
const L2_4_INITIAL: [MirrorOrientation; 5] = [Slash, Slash, Backslash, Backslash, Backslash];

const L2_5_MIRRORS: [MirrorSpec; 6] = [
    MirrorSpec::new(6, 0, Backslash),
    MirrorSpec::new(3, 3, Backslash),
    MirrorSpec::new(5, 3, Slash),
    MirrorSpec::new(6, 3, Slash),
    MirrorSpec::new(2, 4, Slash),
    MirrorSpec::new(3, 4, Backslash),
];
const L2_5_FIXED: [FixedMirror; 3] = [
    FixedMirror::new(2, 0, Slash),
    FixedMirror::new(0, 4, Backslash),
    FixedMirror::new(5, 4, Slash),
];
const L2_5_INITIAL: [MirrorOrientation; 6] = [Slash, Slash, Slash, Backslash, Backslash, Slash];

const L3_1_MIRRORS: [MirrorSpec; 4] = [
    MirrorSpec::new(5, 0, Backslash),
    MirrorSpec::new(2, 2, Backslash),
    MirrorSpec::new(2, 4, Backslash),
    MirrorSpec::new(5, 4, Slash),
];
const L3_1_LAMPS: [GridPoint; 1] = [GridPoint::new(5, 2)];
const L3_1_INITIAL: [MirrorOrientation; 4] = [Slash, Slash, Slash, Backslash];

const L3_2_MIRRORS: [MirrorSpec; 4] = [
    MirrorSpec::new(3, 0, Slash),
    MirrorSpec::new(6, 0, Backslash),
    MirrorSpec::new(3, 2, Slash),
    MirrorSpec::new(6, 2, Slash),
];
const L3_2_LAMPS: [GridPoint; 1] = [GridPoint::new(5, 2)];
const L3_2_INITIAL: [MirrorOrientation; 4] = [Slash, Slash, Backslash, Backslash];

const L3_3_MIRRORS: [MirrorSpec; 5] = [
    MirrorSpec::new(5, 0, Backslash),
    MirrorSpec::new(0, 1, Backslash),
    MirrorSpec::new(1, 1, Backslash),
    MirrorSpec::new(0, 4, Backslash),
    MirrorSpec::new(1, 4, Slash),
];
const L3_3_FIXED: [FixedMirror; 1] = [FixedMirror::new(0, 0, Slash)];
const L3_3_LAMPS: [GridPoint; 2] = [GridPoint::new(4, 0), GridPoint::new(1, 3)];
const L3_3_INITIAL: [MirrorOrientation; 5] = [Slash, Slash, Slash, Slash, Slash];

const L3_4_MIRRORS: [MirrorSpec; 6] = [
    MirrorSpec::new(3, 0, Slash),
    MirrorSpec::new(6, 0, Backslash),
    MirrorSpec::new(6, 1, Slash),
    MirrorSpec::new(3, 2, Backslash),
    MirrorSpec::new(6, 2, Backslash),
    MirrorSpec::new(6, 4, Slash),
];
const L3_4_FIXED: [FixedMirror; 1] = [FixedMirror::new(3, 4, Backslash)];
const L3_4_LAMPS: [GridPoint; 1] = [GridPoint::new(6, 3)];
const L3_4_INITIAL: [MirrorOrientation; 6] = [Slash, Slash, Backslash, Slash, Slash, Backslash];

const L3_5_MIRRORS: [MirrorSpec; 7] = [
    MirrorSpec::new(1, 0, Slash),
    MirrorSpec::new(4, 0, Backslash),
    MirrorSpec::new(2, 1, Slash),
    MirrorSpec::new(3, 1, Backslash),
    MirrorSpec::new(2, 3, Slash),
    MirrorSpec::new(3, 3, Slash),
    MirrorSpec::new(2, 4, Slash),
];
const L3_5_FIXED: [FixedMirror; 1] = [FixedMirror::new(1, 4, Backslash)];
const L3_5_WALLS: [GridPoint; 1] = [GridPoint::new(3, 4)];
const L3_5_LAMPS: [GridPoint; 2] = [GridPoint::new(4, 2), GridPoint::new(2, 2)];
const L3_5_INITIAL: [MirrorOrientation; 7] = [
    Backslash, Slash, Backslash, Slash, Backslash, Backslash, Slash,
];

pub(crate) const LEVELS: [Level; super::types::LEVEL_COUNT] = [
    Level {
        name: crate::tr!(en: "Corner", zh: "转角"),
        subtitle: crate::tr!(
            en: "Tap a mirror to flip it: / or \\.",
            zh: "点镜子，它会在 / 和 \\ 之间切换。"
        ),
        source_row: 3,
        target: GridPoint::new(5, 1),
        mirrors: &L1_1_MIRRORS,
        fixed: &[],
        walls: &L1_1_WALLS,
        lamps: &[],
        initial: &L1_1_INITIAL,
    },
    Level {
        name: crate::tr!(en: "U-turn", zh: "U 形弯"),
        subtitle: crate::tr!(
            en: "Go down first, then around below.",
            zh: "先往下走，再从下面绕过去。"
        ),
        source_row: 0,
        target: GridPoint::new(5, 1),
        mirrors: &L1_2_MIRRORS,
        fixed: &[],
        walls: &L1_2_WALLS,
        lamps: &[],
        initial: &L1_2_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Around the wall", zh: "绕墙"),
        subtitle: crate::tr!(
            en: "Every mirror is needed.",
            zh: "每面镜子都要用上。"
        ),
        source_row: 2,
        target: GridPoint::new(6, 4),
        mirrors: &L1_3_MIRRORS,
        fixed: &[],
        walls: &L1_3_WALLS,
        lamps: &[],
        initial: &L1_3_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Crossing", zh: "交叉"),
        subtitle: crate::tr!(
            en: "The beam may cross itself.",
            zh: "光路可以交叉。"
        ),
        source_row: 4,
        target: GridPoint::new(6, 0),
        mirrors: &L1_4_MIRRORS,
        fixed: &[],
        walls: &L1_4_WALLS,
        lamps: &[],
        initial: &L1_4_INITIAL,
    },
    Level {
        name: crate::tr!(en: "7 mirrors", zh: "七面镜"),
        subtitle: crate::tr!(
            en: "Start at the source, one at a time.",
            zh: "从光源出发，一面一面地调。"
        ),
        source_row: 0,
        target: GridPoint::new(2, 2),
        mirrors: &L1_5_MIRRORS,
        fixed: &[],
        walls: &L1_5_WALLS,
        lamps: &[],
        initial: &L1_5_INITIAL,
    },
    Level {
        name: crate::tr!(en: "In the way", zh: "挡路的镜子"),
        subtitle: crate::tr!(
            en: "Riveted mirrors never turn.",
            zh: "有的镜子是固定的，点了不会转。"
        ),
        source_row: 4,
        target: GridPoint::new(6, 4),
        mirrors: &L2_1_MIRRORS,
        fixed: &L2_1_FIXED,
        walls: &[],
        lamps: &[],
        initial: &L2_1_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Double back", zh: "回头"),
        subtitle: crate::tr!(
            en: "One mirror can be used twice.",
            zh: "同一面镜子可以用两次。"
        ),
        source_row: 3,
        target: GridPoint::new(5, 3),
        mirrors: &L2_2_MIRRORS,
        fixed: &L2_2_FIXED,
        walls: &[],
        lamps: &[],
        initial: &L2_2_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Zigzag", zh: "之字"),
        subtitle: crate::tr!(
            en: "Start from the fixed mirrors.",
            zh: "从固定镜开始想。"
        ),
        source_row: 0,
        target: GridPoint::new(6, 4),
        mirrors: &L2_3_MIRRORS,
        fixed: &L2_3_FIXED,
        walls: &[],
        lamps: &[],
        initial: &L2_3_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Middle row", zh: "中间一排"),
        subtitle: crate::tr!(
            en: "Or work back from the receiver.",
            zh: "也可以从接收器往回推。"
        ),
        source_row: 4,
        target: GridPoint::new(4, 3),
        mirrors: &L2_4_MIRRORS,
        fixed: &L2_4_FIXED,
        walls: &[],
        lamps: &[],
        initial: &L2_4_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Outer loop", zh: "外圈"),
        subtitle: crate::tr!(
            en: "The long way round to the left.",
            zh: "接收器在左边，光要绕远路。"
        ),
        source_row: 3,
        target: GridPoint::new(0, 1),
        mirrors: &L2_5_MIRRORS,
        fixed: &L2_5_FIXED,
        walls: &[],
        lamps: &[],
        initial: &L2_5_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Shortcut", zh: "近路"),
        subtitle: crate::tr!(
            en: "Light the lamp before the receiver.",
            zh: "先点亮灯，接收器才会亮。"
        ),
        source_row: 2,
        target: GridPoint::new(2, 0),
        mirrors: &L3_1_MIRRORS,
        fixed: &[],
        walls: &[],
        lamps: &L3_1_LAMPS,
        initial: &L3_1_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Way back", zh: "原路折回"),
        subtitle: crate::tr!(
            en: "Mirrors reflect on both sides.",
            zh: "镜子正反两面都能反光。"
        ),
        source_row: 2,
        target: GridPoint::new(3, 3),
        mirrors: &L3_2_MIRRORS,
        fixed: &[],
        walls: &[],
        lamps: &L3_2_LAMPS,
        initial: &L3_2_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Two lamps", zh: "双灯"),
        subtitle: crate::tr!(
            en: "Light both lamps, in any order.",
            zh: "两个灯都要亮，先后随意。"
        ),
        source_row: 1,
        target: GridPoint::new(5, 1),
        mirrors: &L3_3_MIRRORS,
        fixed: &L3_3_FIXED,
        walls: &[],
        lamps: &L3_3_LAMPS,
        initial: &L3_3_INITIAL,
    },
    Level {
        name: crate::tr!(en: "Ladder", zh: "梯子"),
        subtitle: crate::tr!(
            en: "Four mirrors stacked in a column.",
            zh: "四面镜子叠成一列。"
        ),
        source_row: 2,
        target: GridPoint::new(4, 1),
        mirrors: &L3_4_MIRRORS,
        fixed: &L3_4_FIXED,
        walls: &[],
        lamps: &L3_4_LAMPS,
        initial: &L3_4_INITIAL,
    },
    Level {
        name: crate::tr!(en: "7 mirrors, 2 lamps", zh: "七镜两灯"),
        subtitle: crate::tr!(
            en: "The last puzzle.",
            zh: "最后一关。"
        ),
        source_row: 3,
        target: GridPoint::new(4, 3),
        mirrors: &L3_5_MIRRORS,
        fixed: &L3_5_FIXED,
        walls: &L3_5_WALLS,
        lamps: &L3_5_LAMPS,
        initial: &L3_5_INITIAL,
    },
];
