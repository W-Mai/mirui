use super::cells::{GLIDER, GRID_HEIGHT, GRID_WIDTH, MossCells};
use crate::gallery::play::change::ChangeSet;

const HISTORY_CAPACITY: usize = 16;

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

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct MossModel {
    cells: MossCells,
    previous: MossCells,
    history: History,
    transaction: Option<PaintTransaction>,
    live_count: u16,
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
        let cells = MossCells::seed(0).expect("built-in seed");
        Self {
            cells,
            previous: MossCells::EMPTY,
            history: History::new(),
            transaction: None,
            live_count: cells.live_count(),
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

    #[cfg(test)]
    pub(crate) const fn history_len(&self) -> u8 {
        self.history.len
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
        self.live_count = snapshot.cells.live_count();
        self.generation = snapshot.generation;
        self.seed_id = snapshot.seed_id;
    }

    fn push_current(&mut self) {
        self.history.push(self.snapshot());
    }

    fn set_cell(&mut self, x: u8, y: u8, age: u8) -> bool {
        let was_live = self.cells.get(x, y) != 0;
        if !self.cells.set(x, y, age) {
            return false;
        }
        let is_live = age != 0;
        if was_live != is_live {
            if is_live {
                self.live_count += 1;
            } else {
                self.live_count -= 1;
            }
        }
        true
    }

    fn paint_inner(&mut self, x: u8, y: u8) -> bool {
        match self.tool {
            MossTool::Plant => self.set_cell(x, y, 1),
            MossTool::Erase => self.set_cell(x, y, 0),
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
                        changed |= self.set_cell(cell_x as u8, cell_y as u8, 1);
                    }
                }
                changed
            }
        }
    }

    fn evolve_once(&mut self) {
        let next = self.cells.evolve();
        self.previous = self.cells;
        self.cells = next;
        self.live_count = next.live_count();
        self.generation = self.generation.saturating_add(1).min(999_999);
        if self.live_count == 0 {
            self.running = false;
            self.elapsed_ms = 0;
        }
    }
}

#[crate::model]
impl MossModel {
    #[observe]
    pub(crate) fn generation(&self) -> u32 {
        self.generation
    }

    #[observe]
    pub(crate) fn tool(&self) -> MossTool {
        self.tool
    }

    #[observe]
    pub(crate) fn modal(&self) -> MossModal {
        self.modal
    }

    #[observe]
    pub(crate) fn seed_id(&self) -> u8 {
        self.seed_id
    }

    #[observe]
    pub(crate) fn rotation(&self) -> u8 {
        self.rotation
    }

    #[observe]
    pub(crate) fn running(&self) -> bool {
        self.running
    }

    #[observe]
    pub(crate) fn can_undo(&self) -> bool {
        self.history.len > 0
    }

    #[observe]
    pub(crate) fn rate(&self) -> u8 {
        [1, 2, 4, 8][self.rate_index as usize]
    }

    #[observe]
    pub(crate) fn live_count(&self) -> u16 {
        self.live_count
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
            if !changed {
                return ChangeSet::NONE;
            }
            self.restore(transaction.snapshot);
            return ChangeSet::MODEL | ChangeSet::VISUAL;
        }
        if self.cells == transaction.snapshot.cells {
            return ChangeSet::NONE;
        }
        self.history.push(transaction.snapshot);
        self.previous = MossCells::EMPTY;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn paint_cell(&mut self, x: u8, y: u8) -> ChangeSet {
        let changes = self.begin_stroke(x, y);
        changes | self.end_stroke(false)
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
        self.live_count = cells.live_count();
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
        self.live_count = 0;
        self.generation = 0;
        self.modal = MossModal::None;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT | ChangeSet::PERSISTENCE
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        let cancelled = self.end_stroke(true);
        self.running = false;
        self.elapsed_ms = 0;
        let Some(snapshot) = self.history.pop() else {
            return cancelled;
        };
        self.restore(snapshot);
        cancelled | ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }
}
