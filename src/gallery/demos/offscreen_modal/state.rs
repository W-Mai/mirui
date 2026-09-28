use crate::prelude::Fixed;

pub(super) const WIN_W: i32 = 360;
pub(super) const WIN_H: i32 = 360;
pub(super) const MODAL_W: i32 = 200;
pub(super) const MODAL_H: i32 = 280;
pub(super) const GRID_COLS: i32 = 4;
pub(super) const GRID_ROWS: i32 = 9;
pub(super) const TILE_W: i32 = 40;
pub(super) const TILE_H: i32 = 24;
pub(super) const TILE_GAP: i32 = 4;
pub(super) const TILE_PAD: i32 = 8;

pub(super) const TOGGLE_NS: u64 = 5_000_000_000;
pub(super) const UPDATE_EVERY: u32 = 30;

pub const DEFAULT_VIEW: (u16, u16) = (WIN_W as u16, WIN_H as u16);

#[crate::component]
pub struct ModalAnim {
    pub t: Fixed,
}

#[crate::component]
pub struct FpsReadout {
    pub counter: u32,
    pub accum_render_ns: u64,
}

pub struct ModeToggle {
    pub last_flip_ns: u64,
    pub elapsed_ns: u64,
    pub offscreen: bool,
}
