use crate::gallery::play::expeditions::{BoardShot, Direction4};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TwinMessage {
    Ready,
    Blocked,
    KeyCollected,
    Undone,
    Hint(Direction4, u16),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct TwinState {
    pub(super) a: u8,
    pub(super) b: u8,
    pub(super) key: bool,
}

/// What the boards show: the shared progress of the level, where the two
/// beacons stand, and the direction of the last refused move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TwinBoard {
    pub(crate) shot: BoardShot,
    pub(crate) cells: [u8; 2],
    pub(crate) bump: Direction4,
}

impl TwinBoard {
    /// Nothing drawn yet.
    pub(crate) const UNSEEN: Self = Self {
        shot: BoardShot::UNSEEN,
        cells: [0; 2],
        bump: Direction4::Up,
    };
}
