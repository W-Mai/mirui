use crate::gallery::play::change::ChangeSet;

pub(crate) const BOARD_WIDTH: usize = 12;
pub(crate) const BOARD_HEIGHT: usize = 7;
pub(crate) const CELL_COUNT: usize = BOARD_WIDTH * BOARD_HEIGHT;
pub(crate) const MAX_GATES: usize = 3;
pub(crate) const MAX_GEMS: usize = 4;
pub(crate) const MAX_CLOCKS: usize = 2;
pub(crate) const MAX_GHOSTS: usize = 3;
pub(crate) const BEAT_LIMIT: u8 = 48;
const ROOM_COUNT: u8 = 12;
const HISTORY_CAPACITY: usize = 64;
const ROUTE_BYTES: usize = 18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Right,
    Down,
    Left,
    Wait,
}

impl Direction {
    const fn code(self) -> u8 {
        match self {
            Self::Up => 0,
            Self::Right => 1,
            Self::Down => 2,
            Self::Left => 3,
            Self::Wait => 4,
        }
    }

    const fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Up,
            1 => Self::Right,
            2 => Self::Down,
            3 => Self::Left,
            _ => Self::Wait,
        }
    }

    const fn delta(self) -> (i8, i8) {
        match self {
            Self::Up => (0, -1),
            Self::Right => (1, 0),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Wait => (0, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClockGate {
    pub(crate) cell: u8,
    pub(crate) phase: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EchoRoom {
    walls: [bool; CELL_COUNT],
    pub(crate) start: u8,
    pub(crate) end: u8,
    pub(crate) doors: [u8; MAX_GATES],
    pub(crate) plates: [u8; MAX_GATES],
    pub(crate) gate_count: u8,
    pub(crate) gems: [u8; MAX_GEMS],
    pub(crate) gem_count: u8,
    pub(crate) clocks: [ClockGate; MAX_CLOCKS],
    pub(crate) clock_count: u8,
}

impl EchoRoom {
    const EMPTY: Self = Self {
        walls: [true; CELL_COUNT],
        start: 0,
        end: 0,
        doors: [0; MAX_GATES],
        plates: [0; MAX_GATES],
        gate_count: 0,
        gems: [0; MAX_GEMS],
        gem_count: 0,
        clocks: [ClockGate { cell: 0, phase: 0 }; MAX_CLOCKS],
        clock_count: 0,
    };

    pub(crate) const fn is_wall(&self, cell: usize) -> bool {
        self.walls[cell]
    }

    pub(crate) fn door_at(&self, cell: u8) -> Option<usize> {
        self.doors[..usize::from(self.gate_count)]
            .iter()
            .position(|door| *door == cell)
    }

    pub(crate) fn plate_at(&self, cell: u8) -> Option<usize> {
        self.plates[..usize::from(self.gate_count)]
            .iter()
            .position(|plate| *plate == cell)
    }

    pub(crate) fn gem_at(&self, cell: u8) -> Option<usize> {
        self.gems[..usize::from(self.gem_count)]
            .iter()
            .position(|gem| *gem == cell)
    }

    pub(crate) fn clock_at(&self, cell: u8) -> Option<ClockGate> {
        self.clocks[..usize::from(self.clock_count)]
            .iter()
            .copied()
            .find(|clock| clock.cell == cell)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedRoute {
    bytes: [u8; ROUTE_BYTES],
    len: u8,
}

impl PackedRoute {
    const EMPTY: Self = Self {
        bytes: [0; ROUTE_BYTES],
        len: 0,
    };

    fn push(&mut self, direction: Direction) -> bool {
        if self.len >= BEAT_LIMIT {
            return false;
        }
        let bit = usize::from(self.len) * 3;
        let byte = bit / 8;
        let shift = bit % 8;
        let value = u16::from(direction.code()) << shift;
        self.bytes[byte] |= value as u8;
        if shift > 5 {
            self.bytes[byte + 1] |= (value >> 8) as u8;
        }
        self.len += 1;
        true
    }

    fn get(&self, index: u8) -> Option<Direction> {
        if index >= self.len {
            return None;
        }
        let bit = usize::from(index) * 3;
        let byte = bit / 8;
        let shift = bit % 8;
        let mut value = u16::from(self.bytes[byte]) >> shift;
        if shift > 5 {
            value |= u16::from(self.bytes[byte + 1]) << (8 - shift);
        }
        Some(Direction::from_code((value & 0b111) as u8))
    }

    const fn len(self) -> u8 {
        self.len
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EchoResult {
    pub(crate) steps: u16,
    pub(crate) loops: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EchoMessage {
    Ready,
    Plate(u8),
    Fragment,
    Missing(u8),
    BeatLimit,
    Recorded,
    Restarted,
    Cleared,
    Undone,
    Won,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EchoModal {
    #[default]
    None,
    Tapes,
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EchoState {
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

#[derive(Clone, Copy)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed.max(1))
    }

    fn next(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.0 = value;
        value
    }

    fn index(&mut self, len: u32) -> usize {
        ((u64::from(self.next()) * u64::from(len)) >> 32) as usize
    }

    fn chance_below_half(&mut self) -> bool {
        self.next() < (1 << 31)
    }
}

pub(crate) struct EchoModel {
    seed: u32,
    level: u8,
    total_steps: u16,
    total_loops: u16,
    results: [EchoResult; ROOM_COUNT as usize],
    result_len: u8,
    complete: bool,
    room: EchoRoom,
    state: EchoState,
    history: EchoHistory,
    message: EchoMessage,
    modal: EchoModal,
    peek_ghost: Option<u8>,
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
    pub(crate) const fn level(&self) -> u8 {
        self.level
    }
    pub(crate) const fn tick(&self) -> u8 {
        self.state.tick
    }
    pub(crate) const fn position(&self) -> u8 {
        self.state.pos
    }
    pub(crate) const fn steps(&self) -> u16 {
        self.state.steps
    }
    pub(crate) const fn loops(&self) -> u8 {
        self.state.loops
    }
    pub(crate) const fn total_steps(&self) -> u16 {
        self.total_steps
    }
    pub(crate) const fn total_loops(&self) -> u16 {
        self.total_loops
    }
    pub(crate) const fn ghost_count(&self) -> u8 {
        self.state.ghost_count
    }
    pub(crate) const fn route_len(&self) -> u8 {
        self.state.routes[0].len()
    }
    pub(crate) const fn ghost_route_len(&self, ghost: usize) -> u8 {
        self.state.routes[ghost + 1].len()
    }
    pub(crate) const fn won(&self) -> bool {
        self.state.won
    }
    pub(crate) const fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) const fn collected_count(&self) -> u8 {
        self.state.collected.count_ones() as u8
    }
    pub(crate) const fn collected(&self, gem: usize) -> bool {
        self.state.collected & (1 << gem) != 0
    }
    pub(crate) const fn message(&self) -> EchoMessage {
        self.message
    }
    pub(crate) const fn modal(&self) -> EchoModal {
        self.modal
    }
    pub(crate) const fn history_len(&self) -> u8 {
        self.history.len
    }
    pub(crate) const fn result_len(&self) -> u8 {
        self.result_len
    }
    pub(crate) const fn peek_ghost(&self) -> Option<u8> {
        self.peek_ghost
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

    pub(crate) fn step(&mut self, direction: Direction) -> ChangeSet {
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

    pub(crate) fn rewind(&mut self) -> ChangeSet {
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

    pub(crate) fn restart(&mut self) -> ChangeSet {
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

    pub(crate) fn clear_room(&mut self) -> ChangeSet {
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

    pub(crate) fn undo(&mut self) -> ChangeSet {
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

    pub(crate) fn continue_archive(&mut self) -> ChangeSet {
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
        } else {
            self.level += 1;
            self.make_room();
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_modal(&mut self, modal: EchoModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.peek_ghost = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_peek_ghost(&mut self, ghost: Option<u8>) -> ChangeSet {
        if self.peek_ghost == ghost || ghost.is_some_and(|value| value >= self.state.ghost_count) {
            return ChangeSet::NONE;
        }
        self.peek_ghost = ghost;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }
}

pub(crate) fn generate_room(seed: u32, level: u8) -> EchoRoom {
    let mut rng = Rng::new(
        seed.max(1)
            .wrapping_add(u32::from(level + 1).wrapping_mul(2_246_822_519)),
    );
    let gate_count = if level < 2 {
        1
    } else if level < 6 {
        2
    } else {
        3
    };
    let width = 3 * gate_count + 3;
    let mut room = EchoRoom::EMPTY;
    for y in 1..6 {
        for x in 1..width - 1 {
            room.walls[y * BOARD_WIDTH + x] = false;
        }
    }
    room.start = (3 * BOARD_WIDTH + 1) as u8;
    room.end = (3 * BOARD_WIDTH + width - 2) as u8;
    room.gate_count = gate_count as u8;
    for gate in 0..gate_count {
        let x = 3 * (gate + 1);
        let door_y = 1 + rng.index(5);
        for y in 1..6 {
            room.walls[y * BOARD_WIDTH + x] = true;
        }
        let door = door_y * BOARD_WIDTH + x;
        room.walls[door] = false;
        room.doors[gate] = door as u8;
        let mut choices = [0usize; 3];
        let mut choice_len = 0;
        for y in 1usize..6 {
            if y.abs_diff(door_y) >= 2 {
                choices[choice_len] = y;
                choice_len += 1;
            }
        }
        let plate_y = choices[rng.index(choice_len as u32)];
        room.plates[gate] = (plate_y * BOARD_WIDTH + x - 1) as u8;
    }
    room.gem_count = (gate_count + 1) as u8;
    for partition in 0..=gate_count {
        let x = 3 * partition + 1;
        let y = if partition % 2 == 0 { 1 } else { 5 };
        room.gems[partition] = (y * BOARD_WIDTH + x) as u8;
    }
    if level >= 3 {
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + 4) as u8;
        if room.plate_at(cell).is_none() && room.gem_at(cell).is_none() {
            room.clocks[usize::from(room.clock_count)] = ClockGate {
                cell,
                phase: rng.index(4) as u8,
            };
            room.clock_count += 1;
        }
    }
    if level >= 8 {
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + 7) as u8;
        if room.plate_at(cell).is_none() && room.gem_at(cell).is_none() {
            room.clocks[usize::from(room.clock_count)] = ClockGate {
                cell,
                phase: rng.index(4) as u8,
            };
            room.clock_count += 1;
        }
    }
    for partition in 0..=gate_count {
        let x = 3 * partition + 2;
        if x >= width - 1 {
            continue;
        }
        let cell = ((1 + rng.index(5)) * BOARD_WIDTH + x) as u8;
        let near_door = room.doors[..gate_count]
            .iter()
            .any(|door| cell == *door || cell + 1 == *door || cell == door.saturating_add(1));
        let protected = cell == room.start
            || cell == room.end
            || room.plate_at(cell).is_some()
            || room.gem_at(cell).is_some()
            || near_door
            || room.clock_at(cell).is_some();
        if !protected && rng.chance_below_half() {
            room.walls[usize::from(cell)] = true;
        }
    }
    let flip_x = rng.chance_below_half();
    let flip_y = rng.chance_below_half();
    if flip_x || flip_y {
        room = mirror_room(room, flip_x, flip_y);
    }
    room
}

fn mirror_room(room: EchoRoom, flip_x: bool, flip_y: bool) -> EchoRoom {
    let map = |cell: u8| {
        let x = usize::from(cell) % BOARD_WIDTH;
        let y = usize::from(cell) / BOARD_WIDTH;
        let mapped_x = if flip_x { BOARD_WIDTH - 1 - x } else { x };
        let mapped_y = if flip_y { BOARD_HEIGHT - 1 - y } else { y };
        (mapped_y * BOARD_WIDTH + mapped_x) as u8
    };
    let mut mirrored = EchoRoom::EMPTY;
    for cell in 0..CELL_COUNT {
        mirrored.walls[usize::from(map(cell as u8))] = room.walls[cell];
    }
    mirrored.start = map(room.start);
    mirrored.end = map(room.end);
    mirrored.gate_count = room.gate_count;
    mirrored.gem_count = room.gem_count;
    mirrored.clock_count = room.clock_count;
    for index in 0..usize::from(room.gate_count) {
        mirrored.doors[index] = map(room.doors[index]);
        mirrored.plates[index] = map(room.plates[index]);
    }
    for index in 0..usize::from(room.gem_count) {
        mirrored.gems[index] = map(room.gems[index]);
    }
    for index in 0..usize::from(room.clock_count) {
        mirrored.clocks[index] = ClockGate {
            cell: map(room.clocks[index].cell),
            phase: room.clocks[index].phase,
        };
    }
    mirrored
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

#[cfg(test)]
mod tests {
    use super::*;

    fn path_to(model: &EchoModel, target: u8) -> [Option<Direction>; BEAT_LIMIT as usize] {
        #[derive(Clone, Copy)]
        struct Node {
            cell: u8,
            beat: u8,
            parent: i16,
            direction: Direction,
        }
        const CAPACITY: usize = CELL_COUNT * (BEAT_LIMIT as usize + 1);
        let empty = Node {
            cell: 0,
            beat: 0,
            parent: -1,
            direction: Direction::Wait,
        };
        let mut nodes = [empty; CAPACITY];
        let mut seen = [false; CAPACITY];
        let mut len = 1usize;
        let mut cursor = 0usize;
        nodes[0].cell = model.position();
        nodes[0].beat = model.tick();
        seen[usize::from(model.tick()) * CELL_COUNT + usize::from(model.position())] = true;
        let mut found = None;
        while cursor < len {
            let node = nodes[cursor];
            if node.cell == target {
                found = Some(cursor);
                break;
            }
            if node.beat < BEAT_LIMIT {
                for direction in [
                    Direction::Up,
                    Direction::Right,
                    Direction::Down,
                    Direction::Left,
                    Direction::Wait,
                ] {
                    let (dx, dy) = direction.delta();
                    let x = (node.cell % BOARD_WIDTH as u8) as i8 + dx;
                    let y = (node.cell / BOARD_WIDTH as u8) as i8 + dy;
                    if x < 0 || x >= BOARD_WIDTH as i8 || y < 0 || y >= BOARD_HEIGHT as i8 {
                        continue;
                    }
                    let next = (y as usize * BOARD_WIDTH + x as usize) as u8;
                    let beat = node.beat + 1;
                    if direction != Direction::Wait && !model.can_enter(next, beat, node.cell) {
                        continue;
                    }
                    let key = usize::from(beat) * CELL_COUNT + usize::from(next);
                    if seen[key] {
                        continue;
                    }
                    seen[key] = true;
                    nodes[len] = Node {
                        cell: next,
                        beat,
                        parent: cursor as i16,
                        direction,
                    };
                    if next == target {
                        found = Some(len);
                        len += 1;
                        break;
                    }
                    len += 1;
                }
            }
            if found.is_some() {
                break;
            }
            cursor += 1;
        }
        let mut result = [None; BEAT_LIMIT as usize];
        let Some(mut index) = found else {
            return result;
        };
        let mut count = 0usize;
        while nodes[index].parent >= 0 {
            result[count] = Some(nodes[index].direction);
            count += 1;
            index = nodes[index].parent as usize;
        }
        result[..count].reverse();
        result
    }

    fn walk_to(model: &mut EchoModel, target: u8) {
        for direction in path_to(model, target).into_iter().flatten() {
            assert_ne!(model.step(direction), ChangeSet::NONE);
        }
        assert_eq!(model.position(), target);
    }

    fn solve_room(model: &mut EchoModel) {
        for gate in 0..usize::from(model.room().gate_count) {
            let plate = model.room().plates[gate];
            walk_to(model, plate);
            assert_ne!(model.rewind(), ChangeSet::NONE);
        }
        for gem in 0..usize::from(model.room().gem_count) {
            let cell = model.room().gems[gem];
            if !model.collected(gem) {
                walk_to(model, cell);
            }
        }
        let end = model.room().end;
        walk_to(model, end);
    }

    #[test]
    fn generator_is_deterministic_and_progresses_gate_count() {
        assert_eq!(generate_room(89, 7), generate_room(89, 7));
        assert_ne!(generate_room(89, 7), generate_room(90, 7));
        assert_eq!(generate_room(9, 0).gate_count, 1);
        assert_eq!(generate_room(9, 2).gate_count, 2);
        assert_eq!(generate_room(9, 6).gate_count, 3);
    }

    #[test]
    fn plates_stay_two_rows_from_their_doors() {
        for seed in 1..=100 {
            let room = generate_room(seed, 8);
            for gate in 0..usize::from(room.gate_count) {
                let door_y = room.doors[gate] as usize / BOARD_WIDTH;
                let plate_y = room.plates[gate] as usize / BOARD_WIDTH;
                assert!(door_y.abs_diff(plate_y) >= 2);
            }
        }
    }

    #[test]
    fn movement_waiting_and_closed_doors_follow_the_next_beat() {
        let mut model = EchoModel::new(9);
        let start = model.position();
        assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_eq!(model.position(), start);
        assert_eq!(model.tick(), 1);
        assert_eq!(model.route_len(), 1);
        assert!(!model.can_enter(model.room().doors[0], 1, model.position()));
    }

    #[test]
    fn rewind_records_route_holds_last_cell_and_caps_at_three() {
        let mut model = EchoModel::new(9);
        for ghost in 0..MAX_GHOSTS {
            assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
            assert_ne!(model.rewind(), ChangeSet::NONE);
            assert_eq!(model.ghost_position(ghost, BEAT_LIMIT), model.room().start);
        }
        assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_eq!(model.rewind(), ChangeSet::NONE);
    }

    #[test]
    fn fragments_survive_rewind_and_restart() {
        let mut model = EchoModel::new(9);
        let gem = model.room().gems[0];
        walk_to(&mut model, gem);
        assert_eq!(model.collected_count(), 1);
        assert_ne!(model.rewind(), ChangeSet::NONE);
        assert_eq!(model.collected_count(), 1);
        assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_ne!(model.restart(), ChangeSet::NONE);
        assert_eq!(model.collected_count(), 1);
    }

    #[test]
    fn undo_restores_complete_dynamic_state() {
        let mut model = EchoModel::new(9);
        let plate = model.room().plates[0];
        walk_to(&mut model, plate);
        let before = model.state;
        assert_ne!(model.rewind(), ChangeSet::NONE);
        assert_ne!(model.undo(), ChangeSet::NONE);
        assert_eq!(model.state, before);
    }

    #[test]
    fn clear_preserves_geometry_and_resets_room_progress() {
        let mut model = EchoModel::new(9);
        let room = *model.room();
        assert_ne!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_ne!(model.rewind(), ChangeSet::NONE);
        assert_ne!(model.clear_room(), ChangeSet::NONE);
        assert_eq!(*model.room(), room);
        assert_eq!(model.ghost_count(), 0);
        assert_eq!(model.steps(), 0);
    }

    #[test]
    fn clock_gate_cycles_two_open_then_two_closed() {
        let mut model = EchoModel::new(9);
        model.room.clocks[0] = ClockGate { cell: 17, phase: 0 };
        model.room.clock_count = 1;
        assert!(model.clock_open(17, 0));
        assert!(model.clock_open(17, 1));
        assert!(!model.clock_open(17, 2));
        assert!(!model.clock_open(17, 3));
        assert!(model.clock_open(17, 4));
    }

    #[test]
    fn campaign_requires_exit_and_every_fragment() {
        let mut model = EchoModel::new(9);
        solve_room(&mut model);
        assert!(model.won());
        assert_eq!(model.position(), model.room().end);
        assert_eq!(model.collected_count(), model.room().gem_count);
        assert_eq!(model.step(Direction::Wait), ChangeSet::NONE);
        assert_ne!(model.continue_archive(), ChangeSet::NONE);
        assert_eq!(model.level(), 1);
        assert_eq!(model.ghost_count(), 0);
    }

    #[test]
    fn all_twelve_rooms_can_be_completed() {
        let mut model = EchoModel::new(2718);
        for room in 0..ROOM_COUNT {
            solve_room(&mut model);
            assert!(model.won(), "room {room}");
            assert_ne!(model.continue_archive(), ChangeSet::NONE);
        }
        assert!(model.complete());
        assert_eq!(model.result_len(), ROOM_COUNT);
    }

    #[test]
    fn history_and_model_memory_are_bounded() {
        let mut model = EchoModel::new(9);
        for _ in 0..80 {
            let _ = model.step(Direction::Wait);
            let _ = model.restart();
        }
        assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
        assert!(core::mem::size_of::<EchoModel>() <= 8 * 1024);
    }
}
