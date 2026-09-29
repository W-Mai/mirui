use super::super::change::ChangeSet;
use super::HISTORY_CAPACITY;
use super::trace::{Trace, trace};
use super::types::{GridPoint, LEVEL_COUNT, LEVELS, Level, MAX_MIRRORS, MirrorOrientation};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct HistoryEntry {
    pub(super) orientation_bits: u8,
    pub(super) moves: u16,
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
#[derive(Clone, Debug)]
pub(crate) struct LumenModel {
    pub(super) level_index: u8,
    pub(super) orientations: [MirrorOrientation; MAX_MIRRORS],
    pub(super) moves: u16,
    pub(super) history: [HistoryEntry; HISTORY_CAPACITY],
    pub(super) history_len: u8,
    pub(super) completed: u8,
    pub(super) selected: u8,
    pub(super) hint: Option<u8>,
    pub(super) trace: Trace,
    pub(super) scan: bool,
    pub(super) scan_phase: u16,
    pub(super) levels_open: bool,
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

    pub(crate) const fn orientations(&self) -> &[MirrorOrientation; MAX_MIRRORS] {
        &self.orientations
    }

    pub(crate) const fn trace(&self) -> &Trace {
        &self.trace
    }

    pub(crate) const fn selected(&self) -> usize {
        self.selected as usize
    }

    pub(crate) const fn scan_phase(&self) -> u16 {
        self.scan_phase
    }

    #[cfg(test)]
    pub(crate) const fn is_completed(&self, level: usize) -> bool {
        level < LEVEL_COUNT && self.completed & (1 << level) != 0
    }

    pub(super) fn load_unchecked(&mut self, index: u8) {
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

    pub(super) fn push_history(&mut self) {
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

    pub(super) fn evaluate(&mut self) {
        self.trace = trace(self.level(), &self.orientations);
        if self.trace.solved {
            self.completed |= 1 << self.level_index;
        }
    }
}

#[crate::model]
impl LumenModel {
    #[observe]
    pub(crate) fn level_index(&self) -> usize {
        self.level_index as usize
    }

    #[observe]
    pub(crate) fn moves(&self) -> u16 {
        self.moves
    }

    #[observe]
    pub(crate) fn hint(&self) -> Option<usize> {
        self.hint.map(usize::from)
    }

    #[observe]
    pub(crate) fn scan(&self) -> bool {
        self.scan
    }

    #[observe]
    pub(crate) fn levels_open(&self) -> bool {
        self.levels_open
    }

    #[observe]
    pub(crate) fn can_undo(&self) -> bool {
        self.history_len > 0
    }

    #[observe]
    pub(crate) fn completion_mask(&self) -> u8 {
        self.completed
    }

    #[observe]
    pub(crate) fn solved(&self) -> bool {
        self.trace.solved
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

    #[model(local)]
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
}
