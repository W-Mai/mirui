use crate::gallery::play::expeditions::Direction4;

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
