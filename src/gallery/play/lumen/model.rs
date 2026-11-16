use super::super::change::ChangeSet;
use super::HISTORY_CAPACITY;
use super::levels::LEVELS;
use super::motion::BoardMotion;
use super::trace::{Trace, TraceStop, trace};
use super::types::{GridPoint, LEVEL_COUNT, Level, MAX_MIRRORS, MirrorOrientation};
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{ExpeditionSaveError, finish_packet, validate_packet};

/// Most stars one puzzle gives.
pub(crate) const MAX_STARS: u8 = 3;

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"LUM1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 6 + LEVEL_COUNT;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

/// Stars for a solve in `moves` turns: three at par without a hint, two
/// within one slip of par or after a hint, one otherwise. A slip costs two
/// turns, one there and one back.
pub(crate) const fn rating(moves: u16, par: u8, hinted: bool) -> u8 {
    let par = par as u16;
    let stars = if moves <= par {
        3
    } else if moves <= par + 2 {
        2
    } else {
        1
    };
    if hinted && stars > 2 { 2 } else { stars }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct HistoryEntry {
    pub(super) orientation_bits: u8,
    pub(super) moves: u16,
}

/// Where the beam ended, as the side panel tells it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BeamStatus {
    /// The receiver is lit.
    Lit,
    /// The beam reached the receiver with a lamp still dark.
    LampsDark,
    Wall,
    Edge,
    Loop,
}

/// The line beside the puzzle name: its own tip, or a reply to the last tap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LumenNote {
    Tip,
    /// The first mirror on the beam that faces the wrong way.
    Hint(u8),
    /// The last tap hit a fixed mirror.
    Fixed,
}

/// What a restart keeps: the puzzle in play and the best stars of each.
#[cfg(any(feature = "persistence", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LumenProgress {
    level: u8,
    stars: [u8; LEVEL_COUNT],
}

#[crate::model(
    change = ChangeSet,
    watch(visual = ChangeSet::VISUAL),
    persistence(key = "lumen_lab/progress")
)]
#[derive(Clone)]
pub(crate) struct LumenModel {
    pub(super) level_index: u8,
    pub(super) orientations: [MirrorOrientation; MAX_MIRRORS],
    pub(super) moves: u16,
    pub(super) history: [HistoryEntry; HISTORY_CAPACITY],
    pub(super) history_len: u8,
    /// Best stars of each puzzle; zero until it is solved once.
    pub(super) stars: [u8; LEVEL_COUNT],
    /// Stars of the current solve; zero while unsolved.
    pub(super) earned: u8,
    /// A hint was shown since the puzzle loaded.
    pub(super) hinted: bool,
    pub(super) selected: Option<u8>,
    pub(super) hint: Option<u8>,
    pub(super) fixed_tapped: bool,
    pub(super) trace: Trace,
    pub(super) scan_phase: u16,
    pub(super) levels_open: bool,
    pub(super) motion: BoardMotion,
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
            stars: [0; LEVEL_COUNT],
            earned: 0,
            hinted: false,
            selected: None,
            hint: None,
            fixed_tapped: false,
            trace: Trace::default(),
            scan_phase: 0,
            levels_open: false,
            motion: BoardMotion::new(),
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

    pub(crate) const fn motion(&self) -> &BoardMotion {
        &self.motion
    }

    /// The mirror turned last, if any since the puzzle loaded.
    pub(crate) fn selected(&self) -> Option<usize> {
        self.selected.map(usize::from)
    }

    pub(crate) const fn scan_phase(&self) -> u16 {
        self.scan_phase
    }

    #[cfg(test)]
    pub(crate) const fn is_completed(&self, level: usize) -> bool {
        level < LEVEL_COUNT && self.stars[level] != 0
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
        self.selected = None;
        self.hint = None;
        self.hinted = false;
        self.fixed_tapped = false;
        self.scan_phase = 0;
        self.motion = BoardMotion::new();
        self.retrace(None);
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

    /// Follows the beam and rates a solve; a better rating is kept.
    pub(super) fn evaluate(&mut self) {
        self.trace = trace(self.level(), &self.orientations);
        self.earned = 0;
        if self.trace.solved {
            self.earned = rating(self.moves, self.level().par(), self.hinted);
            let best = &mut self.stars[usize::from(self.level_index)];
            *best = (*best).max(self.earned);
        }
    }

    /// Evaluates the board and draws the beam again: from the source, or
    /// from where it first meets the mirror that moved. A mirror off the
    /// beam changes nothing, so nothing is redrawn.
    fn retrace(&mut self, moved: Option<usize>) {
        self.evaluate();
        let points = self.trace.points();
        let segments = points.len().saturating_sub(1);
        let from = match moved {
            None => 0,
            Some(index) => {
                let position = self.level().mirrors[index].position;
                points
                    .iter()
                    .position(|point| *point == position)
                    .unwrap_or(segments)
            }
        };
        self.motion.draw_beam(from, segments);
    }

    /// The first rotatable mirror on the current beam that faces away from
    /// the solution. While the beam is unsolved one always exists: a beam
    /// that met only correct mirrors would follow the solution all the way.
    pub(super) fn first_wrong_mirror(&self) -> Option<u8> {
        let level = self.level();
        self.trace.points().iter().find_map(|point| {
            let index = level.mirror_at(*point)?;
            (self.orientations[index] != level.mirrors[index].solution).then_some(index as u8)
        })
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn encode_into(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.level_index;
        output[6..SAVE_PAYLOAD].copy_from_slice(&self.stars);
        finish_packet(output, SAVE_PAYLOAD);
        Ok(SAVE_LEN)
    }

    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn decode_progress(input: &[u8]) -> Result<LumenProgress, ExpeditionSaveError> {
        validate_packet(input, SAVE_PAYLOAD)?;
        if input[..4] != SAVE_MAGIC {
            return Err(ExpeditionSaveError::InvalidMagic);
        }
        if input[4] != SAVE_VERSION {
            return Err(ExpeditionSaveError::UnsupportedVersion);
        }
        let level = input[5];
        if usize::from(level) >= LEVEL_COUNT {
            return Err(ExpeditionSaveError::InvalidLevel);
        }
        let mut stars = [0; LEVEL_COUNT];
        stars.copy_from_slice(&input[6..SAVE_PAYLOAD]);
        if stars.iter().any(|stars| *stars > MAX_STARS) {
            return Err(ExpeditionSaveError::InvalidRecord);
        }
        Ok(LumenProgress { level, stars })
    }

    /// Takes back saved stars and reopens the puzzle that was in play.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn restore(&mut self, progress: LumenProgress) {
        self.stars = progress.stars;
        self.levels_open = false;
        self.load_unchecked(progress.level);
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

    /// The fewest turns that solve the current puzzle.
    #[observe]
    pub(crate) fn par(&self) -> u8 {
        self.level().par()
    }

    /// Stars a solve would give right now.
    #[observe]
    pub(crate) fn live_rating(&self) -> u8 {
        rating(self.moves, self.level().par(), self.hinted)
    }

    #[observe]
    pub(crate) fn earned(&self) -> u8 {
        self.earned
    }

    #[observe]
    pub(crate) fn hint(&self) -> Option<usize> {
        self.hint.map(usize::from)
    }

    #[observe]
    pub(crate) fn note(&self) -> LumenNote {
        if self.fixed_tapped {
            LumenNote::Fixed
        } else if let Some(index) = self.hint {
            LumenNote::Hint(index)
        } else {
            LumenNote::Tip
        }
    }

    #[observe]
    pub(crate) fn levels_open(&self) -> bool {
        self.levels_open
    }

    #[observe]
    pub(crate) fn can_undo(&self) -> bool {
        self.history_len > 0
    }

    /// Best stars of every puzzle.
    #[observe]
    pub(crate) fn stars(&self) -> [u8; LEVEL_COUNT] {
        self.stars
    }

    #[observe]
    pub(crate) fn total_stars(&self) -> u8 {
        self.stars.iter().sum()
    }

    #[observe]
    pub(crate) fn completion_mask(&self) -> u16 {
        self.stars
            .iter()
            .enumerate()
            .filter(|(_, stars)| **stars != 0)
            .fold(0, |mask, (level, _)| mask | 1 << level)
    }

    #[observe]
    pub(crate) fn completed_count(&self) -> u8 {
        self.stars.iter().filter(|stars| **stars != 0).count() as u8
    }

    #[observe]
    pub(crate) fn solved(&self) -> bool {
        self.trace.solved
    }

    /// The beam has lit the receiver and the result card is up.
    #[observe]
    pub(crate) fn won(&self) -> bool {
        self.trace.solved && self.motion.result_ready()
    }

    #[observe]
    pub(crate) fn has_next(&self) -> bool {
        usize::from(self.level_index) + 1 < LEVEL_COUNT
    }

    #[observe]
    pub(crate) fn beam_status(&self) -> BeamStatus {
        match self.trace.stop {
            TraceStop::Receiver if self.trace.solved => BeamStatus::Lit,
            TraceStop::Receiver => BeamStatus::LampsDark,
            TraceStop::Wall => BeamStatus::Wall,
            TraceStop::Edge => BeamStatus::Edge,
            TraceStop::Cycle | TraceStop::Budget => BeamStatus::Loop,
        }
    }

    /// Rotatable mirrors the beam touches, and how many the puzzle has.
    #[observe]
    pub(crate) fn mirror_count(&self) -> (u8, u8) {
        (
            self.trace.hit_mirrors.count_ones() as u8,
            self.level().mirrors.len() as u8,
        )
    }

    /// Lamps the beam lights, and how many the puzzle has.
    #[observe]
    pub(crate) fn lamp_count(&self) -> (u8, u8) {
        (
            self.trace.lit_lamps.count_ones() as u8,
            self.level().lamps.len() as u8,
        )
    }

    pub(crate) fn rotate_cell(&mut self, x: i8, y: i8) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        let point = GridPoint::new(x, y);
        if let Some(index) = self.level().mirror_at(point) {
            return self.rotate(index);
        }
        let Some(fixed) = self
            .level()
            .fixed
            .iter()
            .position(|mirror| mirror.position == point)
        else {
            return ChangeSet::NONE;
        };
        self.fixed_tapped = true;
        self.hint = None;
        self.motion.shake(fixed);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    #[model(local)]
    pub(crate) fn rotate(&mut self, index: usize) -> ChangeSet {
        if self.levels_open || index >= self.level().mirrors.len() {
            return ChangeSet::NONE;
        }
        self.push_history();
        self.orientations[index] = self.orientations[index].toggled();
        self.moves = self.moves.saturating_add(1);
        self.selected = Some(index as u8);
        self.hint = None;
        self.fixed_tapped = false;
        self.retrace(Some(index));
        self.motion.turn(index);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.levels_open || self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        let entry = self.history[usize::from(self.history_len)];
        let mut moved = None;
        for (index, orientation) in self.orientations.iter_mut().enumerate() {
            let restored = if entry.orientation_bits & (1 << index) == 0 {
                MirrorOrientation::Slash
            } else {
                MirrorOrientation::Backslash
            };
            if *orientation != restored {
                moved = Some(index);
            }
            *orientation = restored;
        }
        self.moves = entry.moves;
        self.hint = None;
        self.fixed_tapped = false;
        self.retrace(moved);
        if let Some(index) = moved {
            self.motion.turn(index);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn reveal_hint(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        self.hint = self.first_wrong_mirror();
        if self.hint.is_some() {
            self.hinted = true;
        }
        self.fixed_tapped = false;
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

    /// Opens the next puzzle, or the puzzle list after the last one.
    pub(crate) fn next_level(&mut self) -> ChangeSet {
        if self.levels_open {
            return ChangeSet::NONE;
        }
        if !self.has_next() {
            return self.open_levels();
        }
        self.load_unchecked(self.level_index + 1);
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

    /// Ends the running motion at once: the beam, a turn, the stars.
    pub(crate) fn finish_motion(&mut self) -> ChangeSet {
        if !self.motion.moving() {
            return ChangeSet::NONE;
        }
        self.motion.finish(self.trace.solved, self.earned);
        ChangeSet::VISUAL
    }

    /// Runs the board motion on, and the light that flows along a beam
    /// once it is drawn.
    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        if self.levels_open || elapsed_ms == 0 {
            return ChangeSet::NONE;
        }
        let moving = self.motion.moving();
        self.motion
            .advance(elapsed_ms, self.trace.solved, self.earned);
        let flowing = self.motion.beam_complete() && self.trace.points().len() >= 2;
        if flowing {
            self.scan_phase = self.scan_phase.wrapping_add(elapsed_ms.saturating_mul(23));
        }
        if moving || flowing {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }
}
