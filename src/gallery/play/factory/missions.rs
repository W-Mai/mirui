use super::types::{
    CELL_COUNT, FactoryCell, FactoryLayout, FactoryMission, GRID_HEIGHT, GRID_WIDTH, MISSION_COUNT,
    MaterialStage, ModuleKind,
};
use crate::tr;

/// Reads a floor plan written as rows of two-letter cells: `..` empty,
/// `##` wall, `S` source, `D` dock, `B` belt, `F` furnace, `A` assembler and
/// `I` inspector, each followed by its output: `>` `v` `<` `^`.
const fn layout(rows: [&str; GRID_HEIGHT]) -> FactoryLayout {
    let mut cells = [None; CELL_COUNT];
    let mut y = 0;
    while y < GRID_HEIGHT {
        let row = rows[y].as_bytes();
        assert!(row.len() == GRID_WIDTH * 3 - 1, "a row holds nine cells");
        let mut x = 0;
        while x < GRID_WIDTH {
            cells[y * GRID_WIDTH + x] = cell(row[x * 3], row[x * 3 + 1]);
            x += 1;
        }
        y += 1;
    }
    cells
}

const fn cell(kind: u8, direction: u8) -> Option<FactoryCell> {
    let kind = match kind {
        b'.' => return None,
        b'#' => ModuleKind::Wall,
        b'S' => ModuleKind::Source,
        b'D' => ModuleKind::Dock,
        b'B' => ModuleKind::Belt,
        b'F' => ModuleKind::Furnace,
        b'A' => ModuleKind::Assembler,
        b'I' => ModuleKind::Inspector,
        _ => panic!("unknown module"),
    };
    let direction = match direction {
        b'>' | b'.' | b'#' => 0,
        b'v' => 1,
        b'<' => 2,
        b'^' => 3,
        _ => panic!("unknown direction"),
    };
    Some(FactoryCell { kind, direction })
}

const fn at(x: usize, y: usize) -> u8 {
    (y * GRID_WIDTH + x) as u8
}

pub(crate) static MISSIONS: [FactoryMission; MISSION_COUNT] = [
    FactoryMission {
        name: tr!(en: "Trial Run", zh: "试产零件"),
        description: tr!(
            en: "C6 is missing a belt. Add it, then run a trial.",
            zh: "C6 缺一段传送带。\n补上它，然后试产。"
        ),
        target: MaterialStage::Gear,
        goal: 6,
        limit: 120,
        budget: 20,
        power: 9,
        focus: at(5, 2),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> .. B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
        reference: &layout([
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Corner Line", zh: "转角产线"),
        description: tr!(
            en: "The corner belt points the wrong way, and one cell below is missing.",
            zh: "转角的传送带朝向不对，\n下面还缺一格。"
        ),
        target: MaterialStage::Gear,
        goal: 8,
        limit: 150,
        budget: 22,
        power: 9,
        focus: at(4, 1),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> B> .. .. .. ..",
            ".. .. .. .. Bv .. .. .. ..",
            ".. .. .. .. Bv .. .. .. ..",
            ".. .. .. .. A> B> .. B> D.",
            ".. .. .. .. .. .. .. .. ..",
        ]),
        reference: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> Bv .. .. .. ..",
            ".. .. .. .. Bv .. .. .. ..",
            ".. .. .. .. Bv .. .. .. ..",
            ".. .. .. .. A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Own Machines", zh: "自建机器"),
        description: tr!(
            en: "Fill the two gaps: a furnace first, then an assembler.",
            zh: "两处空位：前面放熔炼炉，\n后面放装配机。"
        ),
        target: MaterialStage::Gear,
        goal: 8,
        limit: 150,
        budget: 24,
        power: 9,
        focus: at(2, 1),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> .. B> B> B> Bv .. ..",
            ".. .. .. .. .. .. Bv .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. Bv .. ..",
            ".. .. .. .. .. .. B> B> D.",
        ]),
        reference: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> B> B> Bv .. ..",
            ".. .. .. .. .. .. Bv .. ..",
            ".. .. .. .. .. .. Av .. ..",
            ".. .. .. .. .. .. Bv .. ..",
            ".. .. .. .. .. .. B> B> D.",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Inspection", zh: "质检上线"),
        description: tr!(
            en: "Gears count only once inspected. Make C7 an inspector.",
            zh: "零件过了质检台才算合格品。\n把 C7 换成质检台。"
        ),
        target: MaterialStage::Certified,
        goal: 8,
        limit: 150,
        budget: 20,
        power: 9,
        focus: at(6, 3),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
        reference: &layout([
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> I> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Wall Detour", zh: "隔墙绕行"),
        description: tr!(
            en: "A wall stands in the way. Go around it, above or below.",
            zh: "隔墙挡在中间，\n从上面或下面绕过去。"
        ),
        target: MaterialStage::Gear,
        goal: 8,
        limit: 180,
        budget: 24,
        power: 9,
        focus: at(3, 2),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. ## .. .. .. ..",
            "S> B> F> B> ## B> A> B> D.",
            ".. .. .. .. ## .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
        reference: &layout([
            ".. .. .. B> B> Bv .. .. ..",
            ".. .. .. B^ ## Bv .. .. ..",
            "S> B> F> B^ ## B> A> B> D.",
            ".. .. .. .. ## .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Shortest Path", zh: "最短路线"),
        description: tr!(
            en: "The credits pay for the shortest route and not one cell more.",
            zh: "信用按最短路线给，\n多一格都不行。"
        ),
        target: MaterialStage::Gear,
        goal: 8,
        limit: 180,
        budget: 22,
        power: 9,
        focus: at(1, 1),
        start: &layout([
            ".. .. .. .. ## .. .. .. ..",
            ".. .. ## .. ## .. ## .. ..",
            "S> F^ ## .. A> .. ## .. D.",
            ".. .. ## .. .. .. ## .. ..",
            ".. .. .. .. ## .. ## .. ..",
            ".. .. .. .. ## .. .. .. ..",
        ]),
        reference: &layout([
            ".. B> B> Bv ## B> B> Bv ..",
            ".. B^ ## Bv ## B^ ## Bv ..",
            "S> F^ ## B> A> B^ ## B> D.",
            ".. .. ## .. .. .. ## .. ..",
            ".. .. .. .. ## .. ## .. ..",
            ".. .. .. .. ## .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Twin Lines", zh: "双线并行"),
        description: tr!(
            en: "One line cannot ship enough in time. Build a second one below.",
            zh: "一条线交不完，\n在下面再搭一条。"
        ),
        target: MaterialStage::Gear,
        goal: 16,
        limit: 60,
        budget: 30,
        power: 14,
        focus: at(1, 4),
        start: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> .. .. .. .. .. .. .. D.",
            ".. .. .. .. .. .. .. .. ..",
        ]),
        reference: &layout([
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> B> F> B> A> B> B> B> D.",
            ".. .. .. .. .. .. .. .. ..",
        ]),
    },
    FactoryMission {
        name: tr!(en: "Merge", zh: "两线合流"),
        description: tr!(
            en: "The power runs one inspector only, so merge the two lines first.",
            zh: "功率只能带一台质检台，\n两条线要先合流。"
        ),
        target: MaterialStage::Certified,
        goal: 20,
        limit: 90,
        budget: 32,
        power: 16,
        focus: at(5, 0),
        start: &layout([
            "S> F> B> A> B> .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. D.",
            ".. .. .. .. .. .. .. .. ..",
            ".. .. .. .. .. .. .. .. ..",
            "S> F> B> A> B> .. .. .. ..",
        ]),
        reference: &layout([
            "S> F> B> A> B> Bv .. .. ..",
            ".. .. .. .. .. Bv .. .. ..",
            ".. .. .. .. .. B> I> B> D.",
            ".. .. .. .. .. B^ .. .. ..",
            ".. .. .. .. .. B^ .. .. ..",
            "S> F> B> A> B> B^ .. .. ..",
        ]),
    },
];
