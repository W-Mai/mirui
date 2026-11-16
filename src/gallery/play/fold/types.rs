use crate::gallery::play::expeditions::{BoardShot, Direction4};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum HullPose {
    #[default]
    Upright,
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FoldMessage {
    Ready,
    Unsupported,
    Seal,
    Bridge(bool),
    Undone,
    Hint(Direction4, u16),
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FoldState {
    pub(super) x: u8,
    pub(super) y: u8,
    pub(super) pose: HullPose,
    pub(super) bridge: bool,
    pub(super) seals: u8,
}

/// What the map shows: the shared progress of the level, the cells under
/// the hull (the first `len` of them), and the direction of the last
/// refused roll.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FoldBoard {
    pub(crate) shot: BoardShot,
    pub(crate) cells: [u8; 2],
    pub(crate) len: u8,
    pub(crate) bump: Direction4,
}

impl FoldBoard {
    /// Nothing drawn yet.
    pub(crate) const UNSEEN: Self = Self {
        shot: BoardShot::UNSEEN,
        cells: [0; 2],
        len: 1,
        bump: Direction4::Up,
    };
}
