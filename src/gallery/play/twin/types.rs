use crate::gallery::play::expeditions::Direction4;

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
