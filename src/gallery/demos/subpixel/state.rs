use crate::prelude::{Entity, Fixed};

pub(super) const BAR_W: i32 = 50;
pub(super) const BAR_H: i32 = 8;
pub(super) const BAR_MARGIN: i32 = 10;
pub(super) const START_Y: i32 = 12;

pub struct BarState {
    pub y: Fixed,
    pub speed_per_second: Fixed,
    pub snap: bool,
    pub x: Fixed,
    pub right_anchored: bool,
}

#[derive(Clone, Copy)]
pub(super) struct BarArena(pub(super) Entity);
