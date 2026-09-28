use super::generation::generate_room;
use super::types::{
    BEAT_LIMIT, BOARD_HEIGHT, BOARD_WIDTH, CELL_COUNT, Direction, EchoCommand, EchoMessage,
    EchoModal, EchoResult, EchoRoom, HISTORY_CAPACITY, MAX_GHOSTS, PackedRoute, ROOM_COUNT,
};
use crate::gallery::play::change::ChangeSet;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EchoState {
    routes: [PackedRoute; MAX_GHOSTS + 1],
    ghost_count: u8,
    tick: u8,
    pos: u8,
    collected: u8,
    steps: u16,
    loops: u8,
    won: bool,
}

impl EchoState {
    const EMPTY: Self = Self {
        routes: [PackedRoute::EMPTY; MAX_GHOSTS + 1],
        ghost_count: 0,
        tick: 0,
        pos: 0,
        collected: 0,
        steps: 0,
        loops: 0,
        won: false,
    };
}

#[derive(Clone, Copy)]
struct EchoHistory {
    entries: [EchoState; HISTORY_CAPACITY],
    start: u8,
    len: u8,
}

impl EchoHistory {
    const fn new() -> Self {
        Self {
            entries: [EchoState::EMPTY; HISTORY_CAPACITY],
            start: 0,
            len: 0,
        }
    }

    fn clear(&mut self) {
        self.start = 0;
        self.len = 0;
    }

    fn push(&mut self, state: EchoState) {
        if usize::from(self.len) < HISTORY_CAPACITY {
            let index = (usize::from(self.start) + usize::from(self.len)) % HISTORY_CAPACITY;
            self.entries[index] = state;
            self.len += 1;
        } else {
            self.entries[usize::from(self.start)] = state;
            self.start = (usize::from(self.start) + 1) as u8 % HISTORY_CAPACITY as u8;
        }
    }

    fn pop(&mut self) -> Option<EchoState> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let index = (usize::from(self.start) + usize::from(self.len)) % HISTORY_CAPACITY;
        Some(self.entries[index])
    }
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
pub(crate) struct EchoModel {
    seed: u32,
    level: u8,
    total_steps: u16,
    total_loops: u16,
    results: [EchoResult; ROOM_COUNT as usize],
    result_len: u8,
    complete: bool,
    pub(super) room: EchoRoom,
    pub(super) state: EchoState,
    history: EchoHistory,
    message: EchoMessage,
    modal: EchoModal,
    peek_ghost: Option<u8>,
    #[cfg(feature = "persistence")]
    replay: EchoReplayLog,
}

impl Default for EchoModel {
    fn default() -> Self {
        Self::new(2718)
    }
}

impl EchoModel {
    pub(crate) fn new(seed: u32) -> Self {
        let mut model = Self {
            seed: seed.max(1),
            level: 0,
            total_steps: 0,
            total_loops: 0,
            results: [EchoResult::default(); ROOM_COUNT as usize],
            result_len: 0,
            complete: false,
            room: EchoRoom::EMPTY,
            state: EchoState::EMPTY,
            history: EchoHistory::new(),
            message: EchoMessage::Ready,
            modal: EchoModal::None,
            peek_ghost: None,
            #[cfg(feature = "persistence")]
            replay: EchoReplayLog::new(ReplayKind::Echo, seed.max(1)),
        };
        model.make_room();
        model
    }

    fn make_room(&mut self) {
        self.room = generate_room(self.seed, self.level);
        self.state = EchoState::EMPTY;
        self.state.pos = self.room.start;
        self.history.clear();
        self.message = EchoMessage::Ready;
        self.modal = EchoModal::None;
        self.peek_ghost = None;
    }

    pub(crate) const fn room(&self) -> &EchoRoom {
        &self.room
    }
    pub(crate) const fn ghost_route_len(&self, ghost: usize) -> u8 {
        self.state.routes[ghost + 1].len()
    }
    pub(crate) const fn collected(&self, gem: usize) -> bool {
        self.state.collected & (1 << gem) != 0
    }
    pub(crate) fn route_cell(&self, route: usize, beat: u8) -> u8 {
        route_position(self.room.start, self.state.routes[route], beat)
    }

    pub(crate) fn ghost_position(&self, ghost: usize, beat: u8) -> u8 {
        let route = self.state.routes[ghost + 1];
        route_position(self.room.start, route, beat.min(route.len()))
    }

    pub(crate) fn door_open(&self, door: usize, beat: u8, player: u8) -> bool {
        let plate = self.room.plates[door];
        player == plate
            || (0..usize::from(self.state.ghost_count))
                .any(|ghost| self.ghost_position(ghost, beat) == plate)
    }

    pub(crate) fn clock_open(&self, cell: u8, beat: u8) -> bool {
        self.room
            .clock_at(cell)
            .is_none_or(|clock| (beat + clock.phase) % 4 < 2)
    }

    pub(crate) fn can_enter(&self, cell: u8, beat: u8, player: u8) -> bool {
        if usize::from(cell) >= CELL_COUNT || self.room.is_wall(usize::from(cell)) {
            return false;
        }
        self.room
            .door_at(cell)
            .is_none_or(|door| self.door_open(door, beat, player))
            && self.clock_open(cell, beat)
    }

    fn step_raw(&mut self, direction: Direction) -> ChangeSet {
        if self.state.won || self.complete || self.state.tick >= BEAT_LIMIT {
            return ChangeSet::NONE;
        }
        let (dx, dy) = direction.delta();
        let x = (self.state.pos % BOARD_WIDTH as u8) as i8 + dx;
        let y = (self.state.pos / BOARD_WIDTH as u8) as i8 + dy;
        if x < 0 || x >= BOARD_WIDTH as i8 || y < 0 || y >= BOARD_HEIGHT as i8 {
            return ChangeSet::NONE;
        }
        let cell = (y as usize * BOARD_WIDTH + x as usize) as u8;
        let beat = self.state.tick + 1;
        if direction != Direction::Wait && !self.can_enter(cell, beat, self.state.pos) {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        self.state.tick = beat;
        self.state.pos = cell;
        let pushed = self.state.routes[0].push(direction);
        debug_assert!(pushed);
        self.state.steps = self.state.steps.saturating_add(1);
        if let Some(gem) = self.room.gem_at(cell)
            && !self.collected(gem)
        {
            self.state.collected |= 1 << gem;
            self.message = EchoMessage::Fragment;
        }
        if let Some(plate) = self.room.plate_at(cell) {
            self.message = EchoMessage::Plate(plate as u8);
        }
        if cell == self.room.end {
            let missing = self.room.gem_count - self.collected_count();
            if missing == 0 {
                self.state.won = true;
                self.modal = EchoModal::Result;
                self.message = EchoMessage::Won;
            } else {
                self.message = EchoMessage::Missing(missing);
            }
        } else if beat == BEAT_LIMIT {
            self.message = EchoMessage::BeatLimit;
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn rewind_raw(&mut self) -> ChangeSet {
        if self.state.won
            || self.complete
            || self.state.ghost_count >= MAX_GHOSTS as u8
            || self.state.routes[0].len() == 0
        {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        let ghost = usize::from(self.state.ghost_count) + 1;
        self.state.routes[ghost] = self.state.routes[0];
        self.state.routes[0] = PackedRoute::EMPTY;
        self.state.ghost_count += 1;
        self.state.tick = 0;
        self.state.pos = self.room.start;
        self.state.loops += 1;
        self.peek_ghost = None;
        self.message = EchoMessage::Recorded;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn restart_raw(&mut self) -> ChangeSet {
        if self.state.won || self.complete || self.state.tick == 0 {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        self.state.routes[0] = PackedRoute::EMPTY;
        self.state.tick = 0;
        self.state.pos = self.room.start;
        self.message = EchoMessage::Restarted;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn clear_room_raw(&mut self) -> ChangeSet {
        if self.complete {
            return ChangeSet::NONE;
        }
        self.history.push(self.state);
        self.state = EchoState::EMPTY;
        self.state.pos = self.room.start;
        self.peek_ghost = None;
        self.modal = EchoModal::None;
        self.message = EchoMessage::Cleared;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn undo_raw(&mut self) -> ChangeSet {
        if self.complete {
            return ChangeSet::NONE;
        }
        let Some(state) = self.history.pop() else {
            return ChangeSet::NONE;
        };
        self.state = state;
        self.peek_ghost = None;
        self.modal = EchoModal::None;
        self.message = EchoMessage::Undone;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    fn continue_archive_raw(&mut self) -> ChangeSet {
        if !self.state.won || self.complete {
            return ChangeSet::NONE;
        }
        self.total_steps = self.total_steps.saturating_add(self.state.steps);
        self.total_loops = self.total_loops.saturating_add(u16::from(self.state.loops));
        self.results[usize::from(self.result_len)] = EchoResult {
            steps: self.state.steps,
            loops: self.state.loops,
        };
        self.result_len += 1;
        if self.level + 1 == ROOM_COUNT {
            self.complete = true;
            self.message = EchoMessage::Complete;
            self.modal = EchoModal::Result;
            ChangeSet::MODEL | ChangeSet::PERSISTENCE
        } else {
            self.level += 1;
            self.make_room();
            ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
        }
    }

    fn set_modal_raw(&mut self, modal: EchoModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        let visual = self.peek_ghost.is_some();
        self.modal = modal;
        self.peek_ghost = None;
        if visual {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::MODEL
        }
    }

    fn set_peek_ghost_raw(&mut self, ghost: Option<u8>) -> ChangeSet {
        if self.peek_ghost == ghost || ghost.is_some_and(|value| value >= self.state.ghost_count) {
            return ChangeSet::NONE;
        }
        self.peek_ghost = ghost;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn apply_command(&mut self, command: EchoCommand) -> ChangeSet {
        #[cfg(feature = "persistence")]
        if self.replay.is_full() {
            return ChangeSet::NONE;
        }
        let changes = match command {
            EchoCommand::Step(direction) => self.step_raw(direction),
            EchoCommand::Rewind => self.rewind_raw(),
            EchoCommand::Restart => self.restart_raw(),
            EchoCommand::Clear => self.clear_room_raw(),
            EchoCommand::Undo => self.undo_raw(),
            EchoCommand::Continue => self.continue_archive_raw(),
        };
        #[cfg(feature = "persistence")]
        if changes.contains(ChangeSet::PERSISTENCE) {
            let recorded = self.replay.record_echo(command).is_ok();
            debug_assert!(recorded);
        }
        changes
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn encode_replay(&self) -> alloc::vec::Vec<u8> {
        self.replay.encode_vec()
    }

    #[cfg(all(feature = "persistence", test))]
    pub(crate) const fn replay_len(&self) -> u16 {
        self.replay.len()
    }
}

#[crate::model]
impl EchoModel {
    #[observe]
    pub(crate) fn level(&self) -> u8 {
        self.level
    }

    #[observe]
    pub(crate) fn tick(&self) -> u8 {
        self.state.tick
    }

    pub(crate) fn position(&self) -> u8 {
        self.state.pos
    }

    #[observe]
    pub(crate) fn steps(&self) -> u16 {
        self.state.steps
    }

    #[observe]
    pub(crate) fn loops(&self) -> u8 {
        self.state.loops
    }

    #[observe]
    pub(crate) fn total_steps(&self) -> u16 {
        self.total_steps
    }

    #[observe]
    pub(crate) fn total_loops(&self) -> u16 {
        self.total_loops
    }

    #[observe]
    pub(crate) fn ghost_count(&self) -> u8 {
        self.state.ghost_count
    }

    #[cfg(test)]
    pub(crate) fn route_len(&self) -> u8 {
        self.state.routes[0].len()
    }

    #[cfg(test)]
    pub(crate) fn won(&self) -> bool {
        self.state.won
    }

    #[observe]
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }

    #[observe]
    pub(crate) fn collected_count(&self) -> u8 {
        self.state.collected.count_ones() as u8
    }

    #[observe]
    pub(crate) fn gem_count(&self) -> u8 {
        self.room.gem_count
    }

    #[observe]
    pub(crate) fn message(&self) -> EchoMessage {
        self.message
    }

    #[observe]
    pub(crate) fn modal(&self) -> EchoModal {
        self.modal
    }

    #[cfg(test)]
    pub(crate) fn history_len(&self) -> u8 {
        self.history.len
    }

    #[observe]
    pub(crate) fn result_len(&self) -> u8 {
        self.result_len
    }

    #[observe]
    pub(crate) fn peek_ghost(&self) -> Option<u8> {
        self.peek_ghost
    }

    #[observe]
    pub(crate) fn ghost_route_lengths(&self) -> [u8; MAX_GHOSTS] {
        [
            self.state.routes[1].len(),
            self.state.routes[2].len(),
            self.state.routes[3].len(),
        ]
    }

    #[observe]
    pub(crate) fn can_rewind(&self) -> bool {
        !self.state.won
            && !self.complete
            && self.state.ghost_count < MAX_GHOSTS as u8
            && self.state.routes[0].len() != 0
    }

    #[observe]
    pub(crate) fn can_undo(&self) -> bool {
        !self.complete && self.history.len != 0
    }

    #[observe]
    pub(crate) fn can_restart(&self) -> bool {
        !self.state.won && !self.complete && self.state.routes[0].len() != 0
    }

    pub(crate) fn step(&mut self, direction: Direction) -> ChangeSet {
        self.apply_command(EchoCommand::Step(direction))
    }

    pub(crate) fn rewind(&mut self) -> ChangeSet {
        self.apply_command(EchoCommand::Rewind)
    }

    pub(crate) fn restart(&mut self) -> ChangeSet {
        self.apply_command(EchoCommand::Restart)
    }

    pub(crate) fn clear_room(&mut self) -> ChangeSet {
        self.apply_command(EchoCommand::Clear)
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        self.apply_command(EchoCommand::Undo)
    }

    pub(crate) fn continue_archive(&mut self) -> ChangeSet {
        self.apply_command(EchoCommand::Continue)
    }

    pub(crate) fn set_modal(&mut self, modal: EchoModal) -> ChangeSet {
        self.set_modal_raw(modal)
    }

    pub(crate) fn toggle_peek_ghost(&mut self, ghost: u8) -> ChangeSet {
        let next = if self.peek_ghost == Some(ghost) {
            None
        } else {
            Some(ghost)
        };
        self.set_peek_ghost_raw(next)
    }

    #[cfg(feature = "persistence")]
    pub(crate) fn restore_replay(&mut self, restored: EchoModel) -> ChangeSet {
        *self = restored;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }
}
fn route_position(start: u8, route: PackedRoute, beat: u8) -> u8 {
    let mut cell = start;
    for index in 0..beat.min(route.len()) {
        let (dx, dy) = route.get(index).unwrap_or(Direction::Wait).delta();
        let x = (cell % BOARD_WIDTH as u8) as i8 + dx;
        let y = (cell / BOARD_WIDTH as u8) as i8 + dy;
        cell = (y as usize * BOARD_WIDTH + x as usize) as u8;
    }
    cell
}
