pub(crate) const BOARD_WIDTH: usize = 12;
pub(crate) const BOARD_HEIGHT: usize = 7;
pub(crate) const CELL_COUNT: usize = BOARD_WIDTH * BOARD_HEIGHT;
pub(crate) const MAX_GATES: usize = 3;
pub(crate) const MAX_GEMS: usize = 4;
pub(crate) const MAX_CLOCKS: usize = 2;
pub(crate) const MAX_GHOSTS: usize = 3;
pub(crate) const BEAT_LIMIT: u8 = 48;
pub(super) const ROOM_COUNT: u8 = 12;
pub(super) const HISTORY_CAPACITY: usize = 64;
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

    #[cfg(feature = "persistence")]
    pub(crate) const fn try_from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Up),
            1 => Some(Self::Right),
            2 => Some(Self::Down),
            3 => Some(Self::Left),
            4 => Some(Self::Wait),
            _ => None,
        }
    }

    #[cfg(feature = "persistence")]
    pub(crate) const fn wire_code(self) -> u8 {
        self.code()
    }

    pub(super) const fn delta(self) -> (i8, i8) {
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
    pub(super) walls: [bool; CELL_COUNT],
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
    pub(super) const EMPTY: Self = Self {
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
pub(super) struct PackedRoute {
    bytes: [u8; ROUTE_BYTES],
    len: u8,
}

impl PackedRoute {
    pub(super) const EMPTY: Self = Self {
        bytes: [0; ROUTE_BYTES],
        len: 0,
    };

    pub(super) fn push(&mut self, direction: Direction) -> bool {
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

    pub(super) fn get(&self, index: u8) -> Option<Direction> {
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

    pub(super) const fn len(self) -> u8 {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EchoCommand {
    Step(Direction),
    Rewind,
    Restart,
    Clear,
    Undo,
    Continue,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EchoModal {
    #[default]
    None,
    Tapes,
    Result,
}
