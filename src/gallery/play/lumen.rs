use super::change::ChangeSet;

pub(crate) const GRID_COLUMNS: i8 = 7;
pub(crate) const GRID_ROWS: i8 = 5;
pub(crate) const LEVEL_COUNT: usize = 5;
pub(crate) const MAX_MIRRORS: usize = 7;
pub(crate) const MAX_TRACE_POINTS: usize = 141;
const HISTORY_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct GridPoint {
    pub(crate) x: i8,
    pub(crate) y: i8,
}

impl GridPoint {
    const fn new(x: i8, y: i8) -> Self {
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
    const fn toggled(self) -> Self {
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
    const fn new(x: i8, y: i8, solution: MirrorOrientation) -> Self {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum TraceStop {
    #[default]
    Edge,
    Wall,
    Receiver,
    Cycle,
    Budget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Trace {
    points: [GridPoint; MAX_TRACE_POINTS],
    len: u8,
    pub(crate) hit_mirrors: u8,
    pub(crate) solved: bool,
    pub(crate) stop: TraceStop,
    pub(crate) steps: u8,
}

impl Default for Trace {
    fn default() -> Self {
        Self {
            points: [GridPoint::default(); MAX_TRACE_POINTS],
            len: 0,
            hit_mirrors: 0,
            solved: false,
            stop: TraceStop::Edge,
            steps: 0,
        }
    }
}

impl Trace {
    pub(crate) fn points(&self) -> &[GridPoint] {
        &self.points[..usize::from(self.len)]
    }

    fn push(&mut self, point: GridPoint) {
        let index = usize::from(self.len);
        if index < MAX_TRACE_POINTS {
            self.points[index] = point;
            self.len += 1;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct HistoryEntry {
    orientation_bits: u8,
    moves: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct LumenModel {
    level_index: u8,
    orientations: [MirrorOrientation; MAX_MIRRORS],
    moves: u16,
    history: [HistoryEntry; HISTORY_CAPACITY],
    history_len: u8,
    completed: u8,
    selected: u8,
    hint: Option<u8>,
    trace: Trace,
    scan: bool,
    scan_phase: u16,
    levels_open: bool,
}

impl Default for LumenModel {
    fn default() -> Self {
        Self::new()
    }
}

impl LumenModel {
    pub(crate) fn new() -> Self {
        let mut model = Self {
            level_index: 0,
            orientations: [MirrorOrientation::Slash; MAX_MIRRORS],
            moves: 0,
            history: [HistoryEntry::default(); HISTORY_CAPACITY],
            history_len: 0,
            completed: 0,
            selected: 0,
            hint: None,
            trace: Trace::default(),
            scan: true,
            scan_phase: 0,
            levels_open: false,
        };
        model.load_unchecked(0);
        model
    }

    pub(crate) fn level(&self) -> &'static Level {
        &LEVELS[usize::from(self.level_index)]
    }

    pub(crate) const fn level_index(&self) -> usize {
        self.level_index as usize
    }

    pub(crate) const fn orientations(&self) -> &[MirrorOrientation; MAX_MIRRORS] {
        &self.orientations
    }

    pub(crate) const fn moves(&self) -> u16 {
        self.moves
    }

    pub(crate) const fn trace(&self) -> &Trace {
        &self.trace
    }

    pub(crate) const fn selected(&self) -> usize {
        self.selected as usize
    }

    pub(crate) const fn hint(&self) -> Option<usize> {
        match self.hint {
            Some(value) => Some(value as usize),
            None => None,
        }
    }

    pub(crate) const fn scan(&self) -> bool {
        self.scan
    }

    pub(crate) const fn scan_phase(&self) -> u16 {
        self.scan_phase
    }

    pub(crate) const fn levels_open(&self) -> bool {
        self.levels_open
    }

    pub(crate) const fn can_undo(&self) -> bool {
        self.history_len > 0
    }

    pub(crate) const fn completed_count(&self) -> u32 {
        self.completed.count_ones()
    }

    pub(crate) const fn is_completed(&self, level: usize) -> bool {
        level < LEVEL_COUNT && self.completed & (1 << level) != 0
    }

    pub(crate) fn rotate_cell(&mut self, x: i8, y: i8) -> ChangeSet {
        let Some(index) = self
            .level()
            .mirrors
            .iter()
            .position(|mirror| mirror.position == GridPoint::new(x, y))
        else {
            return ChangeSet::NONE;
        };
        self.rotate(index)
    }

    pub(crate) fn rotate(&mut self, index: usize) -> ChangeSet {
        if self.levels_open || index >= self.level().mirrors.len() {
            return ChangeSet::NONE;
        }
        self.push_history();
        self.orientations[index] = self.orientations[index].toggled();
        self.moves = self.moves.saturating_add(1);
        self.selected = index as u8;
        self.hint = None;
        self.evaluate();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.levels_open || self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        let entry = self.history[usize::from(self.history_len)];
        for (index, orientation) in self.orientations.iter_mut().enumerate() {
            *orientation = if entry.orientation_bits & (1 << index) == 0 {
                MirrorOrientation::Slash
            } else {
                MirrorOrientation::Backslash
            };
        }
        self.moves = entry.moves;
        self.hint = None;
        self.evaluate();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn reveal_hint(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        self.hint = self
            .level()
            .mirrors
            .iter()
            .enumerate()
            .find(|(index, mirror)| self.orientations[*index] != mirror.solution)
            .map(|(index, _)| index as u8);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn reset(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        let index = self.level_index;
        self.load_unchecked(index);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn next_level(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        self.load_unchecked((self.level_index + 1) % LEVEL_COUNT as u8);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn open_levels(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        self.levels_open = true;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn close_levels(&mut self) -> ChangeSet {
        if !self.levels_open {
            return ChangeSet::NONE;
        }
        self.levels_open = false;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_level(&mut self, index: usize) -> ChangeSet {
        if index >= LEVEL_COUNT {
            return ChangeSet::NONE;
        }
        self.levels_open = false;
        self.load_unchecked(index as u8);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_scan(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        self.scan = !self.scan;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        if !self.scan || self.levels_open || self.trace.points().len() < 2 {
            return ChangeSet::NONE;
        }
        self.scan_phase = self.scan_phase.wrapping_add(elapsed_ms.saturating_mul(23));
        ChangeSet::VISUAL
    }

    fn load_unchecked(&mut self, index: u8) {
        self.level_index = index;
        self.orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
        let initial = LEVELS[usize::from(index)].initial;
        for (slot, orientation) in self.orientations.iter_mut().zip(initial) {
            *slot = *orientation;
        }
        self.moves = 0;
        self.history_len = 0;
        self.selected = 0;
        self.hint = None;
        self.scan_phase = 0;
        self.evaluate();
    }

    fn push_history(&mut self) {
        let mut orientation_bits = 0_u8;
        for (index, orientation) in self.orientations.iter().enumerate() {
            if *orientation == MirrorOrientation::Backslash {
                orientation_bits |= 1 << index;
            }
        }
        let entry = HistoryEntry {
            orientation_bits,
            moves: self.moves,
        };
        if usize::from(self.history_len) == HISTORY_CAPACITY {
            self.history.copy_within(1.., 0);
            self.history[HISTORY_CAPACITY - 1] = entry;
        } else {
            self.history[usize::from(self.history_len)] = entry;
            self.history_len += 1;
        }
    }

    fn evaluate(&mut self) {
        self.trace = trace(self.level(), &self.orientations);
        if self.trace.solved {
            self.completed |= 1 << self.level_index;
        }
    }
}

pub(crate) fn trace(level: &Level, orientations: &[MirrorOrientation; MAX_MIRRORS]) -> Trace {
    const DX: [i8; 4] = [1, 0, -1, 0];
    const DY: [i8; 4] = [0, 1, 0, -1];
    const SLASH: [u8; 4] = [3, 2, 1, 0];
    const BACKSLASH: [u8; 4] = [1, 0, 3, 2];

    let mut result = Trace::default();
    let mut visited = [false; (GRID_COLUMNS as usize) * (GRID_ROWS as usize) * 4];
    let mut x = -1;
    let mut y = level.source_row;
    let mut direction = 0_u8;
    result.push(GridPoint::new(x, y));

    for step in 0..140 {
        x += DX[usize::from(direction)];
        y += DY[usize::from(direction)];
        result.push(GridPoint::new(x, y));
        if !(0..GRID_COLUMNS).contains(&x) || !(0..GRID_ROWS).contains(&y) {
            result.stop = TraceStop::Edge;
            break;
        }

        let state =
            ((y as usize * GRID_COLUMNS as usize + x as usize) * 4) + usize::from(direction);
        if visited[state] {
            result.stop = TraceStop::Cycle;
            break;
        }
        visited[state] = true;
        result.steps = result.steps.saturating_add(1);

        let point = GridPoint::new(x, y);
        if point == level.target {
            result.solved = true;
            result.stop = TraceStop::Receiver;
            break;
        }
        if level.walls.contains(&point) {
            result.stop = TraceStop::Wall;
            break;
        }
        if let Some(index) = level
            .mirrors
            .iter()
            .position(|mirror| mirror.position == point)
        {
            result.hit_mirrors |= 1 << index;
            direction = match orientations[index] {
                MirrorOrientation::Slash => SLASH[usize::from(direction)],
                MirrorOrientation::Backslash => BACKSLASH[usize::from(direction)],
            };
        }
        if step == 139 {
            result.stop = TraceStop::Budget;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solved_orientations(level: &Level) -> [MirrorOrientation; MAX_MIRRORS] {
        let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
        for (slot, mirror) in orientations.iter_mut().zip(level.mirrors) {
            *slot = mirror.solution;
        }
        orientations
    }

    #[test]
    fn every_level_starts_unsolved_and_reference_solution_reaches_receiver() {
        for level in &LEVELS {
            let mut initial = [MirrorOrientation::Slash; MAX_MIRRORS];
            for (slot, orientation) in initial.iter_mut().zip(level.initial) {
                *slot = *orientation;
            }
            assert!(!trace(level, &initial).solved, "{}", level.name);
            let solved = trace(level, &solved_orientations(level));
            assert!(solved.solved, "{}", level.name);
            assert_eq!(solved.stop, TraceStop::Receiver);
            assert!(solved.points().len() <= MAX_TRACE_POINTS);
        }
    }

    #[test]
    fn all_orientation_combinations_terminate_within_the_budget() {
        for level in &LEVELS {
            for mask in 0..(1_usize << level.mirrors.len()) {
                let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
                for (index, slot) in orientations
                    .iter_mut()
                    .enumerate()
                    .take(level.mirrors.len())
                {
                    *slot = if mask & (1 << index) == 0 {
                        MirrorOrientation::Backslash
                    } else {
                        MirrorOrientation::Slash
                    };
                }
                let result = trace(level, &orientations);
                assert!(result.steps <= 140);
                assert!(result.points().len() <= MAX_TRACE_POINTS);
            }
        }
    }

    #[test]
    fn rotate_and_undo_restore_orientation_and_move_count() {
        let mut model = LumenModel::new();
        let before = model.orientations;
        assert!(model.rotate(0).contains(ChangeSet::MODEL));
        assert_eq!(model.moves, 1);
        assert!(model.trace.solved);
        assert!(model.undo().contains(ChangeSet::MODEL));
        assert_eq!(model.orientations, before);
        assert_eq!(model.moves, 0);
    }

    #[test]
    fn successful_visit_remains_recorded_after_undo() {
        let mut model = LumenModel::new();
        model.rotate(0);
        model.undo();
        assert!(model.is_completed(0));
        assert!(!model.trace.solved);
    }

    #[test]
    fn invalid_mirror_or_level_does_not_mutate() {
        let mut model = LumenModel::new();
        let before = model.clone();
        assert_eq!(model.rotate(88), ChangeSet::NONE);
        assert_eq!(model.select_level(88), ChangeSet::NONE);
        assert_eq!(model.orientations, before.orientations);
        assert_eq!(model.level_index, before.level_index);
        assert_eq!(model.moves, before.moves);
    }

    #[test]
    fn hint_identifies_mismatch_without_rotating() {
        let mut model = LumenModel::new();
        let before = model.orientations;
        model.reveal_hint();
        assert_eq!(model.hint(), Some(0));
        assert_eq!(model.orientations, before);
    }

    #[test]
    fn undo_is_bounded_to_the_latest_thirty_two_actions() {
        let mut model = LumenModel::new();
        for _ in 0..100 {
            model.rotate(0);
        }
        assert_eq!(model.history_len, 32);
        for _ in 0..32 {
            assert_ne!(model.undo(), ChangeSet::NONE);
        }
        assert_eq!(model.undo(), ChangeSet::NONE);
    }

    #[test]
    fn modal_blocks_board_mutation_and_animation() {
        let mut model = LumenModel::new();
        model.open_levels();
        let before = model.orientations;
        assert_eq!(model.rotate_cell(2, 3), ChangeSet::NONE);
        assert_eq!(model.advance_ms(16), ChangeSet::NONE);
        assert_eq!(model.orientations, before);
    }

    #[test]
    fn scan_animation_stops_without_accumulating_time() {
        let mut model = LumenModel::new();
        let start = model.scan_phase;
        assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
        assert_ne!(model.scan_phase, start);
        model.toggle_scan();
        let stopped = model.scan_phase;
        assert_eq!(model.advance_ms(1000), ChangeSet::NONE);
        assert_eq!(model.scan_phase, stopped);
    }

    #[test]
    fn model_storage_stays_within_embedded_budget() {
        assert!(core::mem::size_of::<LumenModel>() <= 512);
    }
}
