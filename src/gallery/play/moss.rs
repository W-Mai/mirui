use crate::gallery::play::change::ChangeSet;

pub(crate) const GRID_WIDTH: u8 = 20;
pub(crate) const GRID_HEIGHT: u8 = 12;
const CELL_COUNT: usize = GRID_WIDTH as usize * GRID_HEIGHT as usize;
const HISTORY_CAPACITY: usize = 16;
const RANDOM_THRESHOLD: u32 = 1_245_540_515;
const GLIDER: [(i8, i8); 5] = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MossCells {
    ages: [u8; CELL_COUNT],
}

impl MossCells {
    const EMPTY: Self = Self {
        ages: [0; CELL_COUNT],
    };

    const fn index(x: u8, y: u8) -> Option<usize> {
        if x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return None;
        }
        Some(y as usize * GRID_WIDTH as usize + x as usize)
    }

    pub(crate) fn get(&self, x: u8, y: u8) -> u8 {
        Self::index(x, y).map_or(0, |index| self.ages[index])
    }

    fn set(&mut self, x: u8, y: u8, age: u8) -> bool {
        let Some(index) = Self::index(x, y) else {
            return false;
        };
        let changed = self.ages[index] != age;
        self.ages[index] = age;
        changed
    }

    pub(crate) fn live_count(&self) -> u16 {
        self.ages.iter().filter(|age| **age != 0).count() as u16
    }

    pub(crate) fn seed(kind: u8) -> Option<Self> {
        if kind > 2 {
            return None;
        }
        let mut cells = Self::EMPTY;
        let mut set = |x: i8, y: i8| {
            if (0..GRID_WIDTH as i8).contains(&x) && (0..GRID_HEIGHT as i8).contains(&y) {
                cells.set(x as u8, y as u8, 1);
            }
        };
        match kind {
            0 => {
                for (x, y) in GLIDER {
                    set(x + 2, y + 2);
                    set(17 - x, 9 - y);
                }
                for x in 8..=10 {
                    set(x, 5);
                }
                set(9, 4);
                set(9, 6);
            }
            1 => {
                for (center_x, center_y) in [(5, 4), (14, 7)] {
                    set(center_x - 1, center_y);
                    set(center_x, center_y);
                    set(center_x + 1, center_y);
                }
                for (x, y) in [(9, 8), (10, 8), (9, 9), (10, 9)] {
                    set(x, y);
                }
            }
            _ => {
                let mut state = 2026_u32;
                for y in 2..10 {
                    for x in 2..18 {
                        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        if state < RANDOM_THRESHOLD {
                            set(x, y);
                        }
                    }
                }
            }
        }
        Some(cells)
    }

    fn evolve(&self) -> Self {
        let mut next = Self::EMPTY;
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let mut neighbours = 0_u8;
                for dy in -1_i8..=1 {
                    for dx in -1_i8..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let sample_x = x as i8 + dx;
                        let sample_y = y as i8 + dy;
                        if sample_x >= 0
                            && sample_x < GRID_WIDTH as i8
                            && sample_y >= 0
                            && sample_y < GRID_HEIGHT as i8
                            && self.get(sample_x as u8, sample_y as u8) != 0
                        {
                            neighbours += 1;
                        }
                    }
                }
                let age = self.get(x, y);
                if neighbours == 3 || (age != 0 && neighbours == 2) {
                    next.set(x, y, if age == 0 { 1 } else { age.saturating_add(1) });
                }
            }
        }
        next
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    cells: MossCells,
    previous: MossCells,
    generation: u32,
    seed_id: u8,
}

impl Snapshot {
    const EMPTY: Self = Self {
        cells: MossCells::EMPTY,
        previous: MossCells::EMPTY,
        generation: 0,
        seed_id: 0,
    };
}

#[derive(Clone, Copy, Debug)]
struct History {
    entries: [Snapshot; HISTORY_CAPACITY],
    start: u8,
    len: u8,
}

impl History {
    const fn new() -> Self {
        Self {
            entries: [Snapshot::EMPTY; HISTORY_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn push(&mut self, snapshot: Snapshot) {
        if usize::from(self.len) == HISTORY_CAPACITY {
            self.entries[usize::from(self.start)] = snapshot;
            self.start = (self.start + 1) % HISTORY_CAPACITY as u8;
        } else {
            let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
            self.entries[usize::from(index)] = snapshot;
            self.len += 1;
        }
    }

    fn pop(&mut self) -> Option<Snapshot> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let index = (self.start + self.len) % HISTORY_CAPACITY as u8;
        Some(self.entries[usize::from(index)])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MossTool {
    Plant,
    Erase,
    Glider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MossModal {
    None,
    Seeds,
    Clear,
}

#[derive(Clone, Copy, Debug)]
struct PaintTransaction {
    snapshot: Snapshot,
    last_x: u8,
    last_y: u8,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct MossModel {
    cells: MossCells,
    previous: MossCells,
    history: History,
    transaction: Option<PaintTransaction>,
    generation: u32,
    elapsed_ms: u16,
    tool: MossTool,
    modal: MossModal,
    seed_id: u8,
    rate_index: u8,
    rotation: u8,
    running: bool,
}

impl Default for MossModel {
    fn default() -> Self {
        Self {
            cells: MossCells::seed(0).expect("built-in seed"),
            previous: MossCells::EMPTY,
            history: History::new(),
            transaction: None,
            generation: 0,
            elapsed_ms: 0,
            tool: MossTool::Plant,
            modal: MossModal::None,
            seed_id: 0,
            rate_index: 2,
            rotation: 0,
            running: false,
        }
    }
}

impl MossModel {
    pub(crate) const fn cells(&self) -> &MossCells {
        &self.cells
    }

    pub(crate) const fn previous(&self) -> &MossCells {
        &self.previous
    }

    pub(crate) const fn generation(&self) -> u32 {
        self.generation
    }

    pub(crate) const fn tool(&self) -> MossTool {
        self.tool
    }

    pub(crate) const fn modal(&self) -> MossModal {
        self.modal
    }

    pub(crate) const fn seed_id(&self) -> u8 {
        self.seed_id
    }

    pub(crate) const fn rotation(&self) -> u8 {
        self.rotation
    }

    pub(crate) const fn running(&self) -> bool {
        self.running
    }

    pub(crate) const fn history_len(&self) -> u8 {
        self.history.len
    }

    pub(crate) const fn rate(&self) -> u8 {
        [1, 2, 4, 8][self.rate_index as usize]
    }

    pub(crate) fn live_count(&self) -> u16 {
        self.cells.live_count()
    }

    const fn snapshot(&self) -> Snapshot {
        Snapshot {
            cells: self.cells,
            previous: self.previous,
            generation: self.generation,
            seed_id: self.seed_id,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.cells = snapshot.cells;
        self.previous = snapshot.previous;
        self.generation = snapshot.generation;
        self.seed_id = snapshot.seed_id;
    }

    fn push_current(&mut self) {
        self.history.push(self.snapshot());
    }

    pub(crate) fn begin_stroke(&mut self, x: u8, y: u8) -> ChangeSet {
        if self.modal != MossModal::None || x >= GRID_WIDTH || y >= GRID_HEIGHT {
            return ChangeSet::NONE;
        }
        self.running = false;
        self.elapsed_ms = 0;
        if self.transaction.is_none() {
            self.transaction = Some(PaintTransaction {
                snapshot: self.snapshot(),
                last_x: x,
                last_y: y,
            });
        }
        if self.paint_inner(x, y) {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::MODEL
        }
    }

    pub(crate) fn continue_stroke(&mut self, x: u8, y: u8) -> ChangeSet {
        if x >= GRID_WIDTH || y >= GRID_HEIGHT || self.tool == MossTool::Glider {
            return ChangeSet::NONE;
        }
        let Some(transaction) = self.transaction.as_mut() else {
            return ChangeSet::NONE;
        };
        let (mut x0, mut y0) = (i16::from(transaction.last_x), i16::from(transaction.last_y));
        let (x1, y1) = (i16::from(x), i16::from(y));
        transaction.last_x = x;
        transaction.last_y = y;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        let mut changed = false;
        for _ in 0..40 {
            changed |= self.paint_inner(x0 as u8, y0 as u8);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = error * 2;
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
        if changed {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    pub(crate) fn end_stroke(&mut self, cancelled: bool) -> ChangeSet {
        let Some(transaction) = self.transaction.take() else {
            return ChangeSet::NONE;
        };
        if cancelled {
            let changed = self.cells != transaction.snapshot.cells;
            self.restore(transaction.snapshot);
            return if changed {
                ChangeSet::MODEL | ChangeSet::VISUAL
            } else {
                ChangeSet::NONE
            };
        }
        if self.cells == transaction.snapshot.cells {
            return ChangeSet::NONE;
        }
        self.history.push(transaction.snapshot);
        self.previous = MossCells::EMPTY;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn paint_inner(&mut self, x: u8, y: u8) -> bool {
        match self.tool {
            MossTool::Plant => self.cells.set(x, y, 1),
            MossTool::Erase => self.cells.set(x, y, 0),
            MossTool::Glider => {
                let mut changed = false;
                for (mut offset_x, mut offset_y) in GLIDER {
                    for _ in 0..self.rotation {
                        (offset_x, offset_y) = (2 - offset_y, offset_x);
                    }
                    let cell_x = x as i8 + offset_x - 1;
                    let cell_y = y as i8 + offset_y - 1;
                    if cell_x >= 0
                        && cell_x < GRID_WIDTH as i8
                        && cell_y >= 0
                        && cell_y < GRID_HEIGHT as i8
                    {
                        changed |= self.cells.set(cell_x as u8, cell_y as u8, 1);
                    }
                }
                changed
            }
        }
    }

    pub(crate) fn set_tool(&mut self, tool: MossTool) -> ChangeSet {
        if self.modal != MossModal::None || self.tool == tool {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.tool = tool;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn rotate_glider(&mut self) -> ChangeSet {
        if self.modal != MossModal::None || self.tool != MossTool::Glider {
            return ChangeSet::NONE;
        }
        self.rotation = (self.rotation + 1) % 4;
        ChangeSet::MODEL
    }

    pub(crate) fn cycle_rate(&mut self) -> ChangeSet {
        self.rate_index = (self.rate_index + 1) % 4;
        self.elapsed_ms = 0;
        ChangeSet::MODEL
    }

    pub(crate) fn toggle_running(&mut self) -> ChangeSet {
        if self.modal != MossModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.running = !self.running;
        self.elapsed_ms = 0;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn evolve_once(&mut self) {
        self.previous = self.cells;
        self.cells = self.cells.evolve();
        self.generation = self.generation.saturating_add(1).min(999_999);
        if self.cells.live_count() == 0 {
            self.running = false;
            self.elapsed_ms = 0;
        }
    }

    pub(crate) fn step(&mut self) -> ChangeSet {
        if self.modal != MossModal::None {
            return ChangeSet::NONE;
        }
        self.end_stroke(true);
        self.push_current();
        self.running = false;
        self.elapsed_ms = 0;
        self.evolve_once();
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        if !self.running || self.modal != MossModal::None {
            self.elapsed_ms = 0;
            return ChangeSet::NONE;
        }
        let period = [1_000_u16, 500, 250, 125][self.rate_index as usize];
        self.elapsed_ms = self.elapsed_ms.saturating_add(elapsed_ms.min(250));
        let mut evolved = false;
        for _ in 0..2 {
            if self.elapsed_ms < period || !self.running {
                break;
            }
            self.elapsed_ms -= period;
            self.evolve_once();
            evolved = true;
        }
        if evolved {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    pub(crate) fn open_seeds(&mut self) -> ChangeSet {
        self.end_stroke(true);
        self.running = false;
        self.elapsed_ms = 0;
        self.modal = MossModal::Seeds;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn open_clear(&mut self) -> ChangeSet {
        self.end_stroke(true);
        self.running = false;
        self.elapsed_ms = 0;
        self.modal = MossModal::Clear;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        if self.modal == MossModal::None {
            return ChangeSet::NONE;
        }
        self.modal = MossModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn load_seed(&mut self, seed_id: u8) -> ChangeSet {
        if self.modal != MossModal::Seeds {
            return ChangeSet::NONE;
        }
        let Some(cells) = MossCells::seed(seed_id) else {
            return ChangeSet::NONE;
        };
        self.push_current();
        self.cells = cells;
        self.previous = MossCells::EMPTY;
        self.generation = 0;
        self.seed_id = seed_id;
        self.modal = MossModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT | ChangeSet::PERSISTENCE
    }

    pub(crate) fn confirm_clear(&mut self) -> ChangeSet {
        if self.modal != MossModal::Clear {
            return ChangeSet::NONE;
        }
        self.push_current();
        self.cells = MossCells::EMPTY;
        self.previous = MossCells::EMPTY;
        self.generation = 0;
        self.modal = MossModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT | ChangeSet::PERSISTENCE
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        self.end_stroke(true);
        self.running = false;
        self.elapsed_ms = 0;
        let Some(snapshot) = self.history.pop() else {
            return ChangeSet::NONE;
        };
        self.restore(snapshot);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> MossCells {
        MossCells::EMPTY
    }

    #[test]
    fn empty_and_isolated_cells_die_without_wraparound() {
        assert_eq!(empty().evolve(), empty());
        let mut cells = empty();
        cells.set(0, 0, 1);
        assert_eq!(cells.evolve(), empty());
    }

    #[test]
    fn block_is_stable_and_blinker_returns_after_two_steps() {
        let mut block = empty();
        for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
            block.set(x, y, 1);
        }
        let evolved = block.evolve();
        for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
            assert_eq!(evolved.get(x, y), 2);
        }

        let mut blinker = empty();
        for x in 4..=6 {
            blinker.set(x, 5, 1);
        }
        let second = blinker.evolve().evolve();
        for x in 4..=6 {
            assert_ne!(second.get(x, 5), 0);
        }
        assert_eq!(second.live_count(), 3);
    }

    #[test]
    fn glider_moves_one_cell_after_four_generations() {
        let mut cells = empty();
        for (x, y) in GLIDER {
            cells.set((x + 4) as u8, (y + 3) as u8, 1);
        }
        for _ in 0..4 {
            cells = cells.evolve();
        }
        let mut expected = empty();
        for (x, y) in GLIDER {
            expected.set((x + 5) as u8, (y + 4) as u8, 1);
        }
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                assert_eq!(cells.get(x, y) != 0, expected.get(x, y) != 0);
            }
        }
    }

    #[test]
    fn ages_saturate_and_do_not_change_survival_rules() {
        let mut cells = empty();
        for (x, y) in [(4, 4), (5, 4), (4, 5), (5, 5)] {
            cells.set(x, y, u8::MAX);
        }
        let next = cells.evolve();
        assert_eq!(next.get(4, 4), u8::MAX);

        let mut parents = empty();
        for (x, y) in [(7, 6), (8, 6), (9, 6)] {
            parents.set(x, y, 3);
        }
        assert_eq!(parents.evolve().get(8, 5), 1);
    }

    #[test]
    fn drag_commit_and_cancel_are_atomic() {
        let mut model = MossModel::default();
        let initial = model.cells;
        model.begin_stroke(0, 0);
        model.continue_stroke(19, 11);
        model.end_stroke(true);
        assert_eq!(model.cells, initial);
        assert_eq!(model.history_len(), 0);
        model.begin_stroke(0, 0);
        model.continue_stroke(19, 11);
        model.end_stroke(false);
        assert_eq!(model.history_len(), 1);
        model.undo();
        assert_eq!(model.cells, initial);
    }

    #[test]
    fn automatic_evolution_does_not_accumulate_history() {
        let mut model = MossModel::default();
        model.toggle_running();
        for _ in 0..32 {
            model.advance_ms(125);
        }
        assert_eq!(model.history_len(), 0);
        assert!(model.generation() > 0);
    }

    #[test]
    fn manual_step_seed_and_clear_are_undoable() {
        let mut model = MossModel::default();
        let initial = model.cells;
        model.step();
        assert_eq!(model.history_len(), 1);
        model.undo();
        assert_eq!(model.cells, initial);
        model.open_seeds();
        model.load_seed(1);
        assert_eq!(model.seed_id(), 1);
        model.undo();
        assert_eq!(model.cells, initial);
        model.open_clear();
        model.confirm_clear();
        assert_eq!(model.live_count(), 0);
        model.undo();
        assert_eq!(model.cells, initial);
    }

    #[test]
    fn random_seed_is_fixed_and_history_is_bounded() {
        assert_eq!(MossCells::seed(2), MossCells::seed(2));
        let mut model = MossModel::default();
        for index in 0..40 {
            model.open_seeds();
            model.load_seed((index % 3) as u8);
        }
        assert_eq!(model.history_len(), 16);
    }

    #[test]
    fn model_storage_is_bounded() {
        assert_eq!(core::mem::size_of::<MossCells>(), 240);
        assert!(core::mem::size_of::<MossModel>() <= 8_800);
    }
}
