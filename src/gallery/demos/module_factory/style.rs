use crate::prelude::Color;

pub(super) const INK: Color = Color::rgb(32, 41, 50);
pub(super) const BACKGROUND: Color = Color::rgb(237, 238, 232);
pub(super) const PANEL: Color = Color::rgb(252, 252, 246);
pub(super) const WORK: Color = Color::rgb(229, 233, 227);
pub(super) const LINE: Color = Color::rgb(201, 207, 205);
pub(super) const MUTED: Color = Color::rgb(116, 128, 138);
pub(super) const ACCENT: Color = Color::rgb(245, 107, 56);
pub(super) const CYAN: Color = Color::rgb(67, 145, 157);
pub(super) const GREEN: Color = Color::rgb(88, 132, 92);
pub(super) const RED: Color = Color::rgb(189, 83, 72);
/// Secondary text on the dark header and tiles.
pub(super) const DIM: Color = Color::rgb(174, 186, 190);
/// Buttons on the dark header.
pub(super) const HEADER_BUTTON: Color = Color::rgb(52, 64, 74);
/// A star not earned on the result card.
pub(super) const STAR_EMPTY: Color = Color::rgb(214, 218, 212);

/// The factory floor: left, top, width and height.
pub(super) const FRAME: (i32, i32, i32, i32) = (2, 30, 366, 264);
/// The grid inside the floor, under its column numbers.
pub(super) const GRID_LEFT: i32 = 5;
pub(super) const GRID_TOP: i32 = 46;
pub(super) const CELL: i32 = 40;
/// The side column right of the floor, and its contents.
pub(super) const SIDE: (i32, i32, i32, i32) = (372, 30, 104, 264);
pub(super) const SIDE_LEFT: i32 = 377;
pub(super) const SIDE_WIDTH: i32 = 94;
/// The rows of side buttons: the tool, rotate / undo / step, and run.
pub(super) const TOOL_TOP: i32 = 198;
pub(super) const TOOLS_TOP: i32 = 230;
pub(super) const PRIMARY_TOP: i32 = 262;
/// The order tiles on the order page.
pub(super) const TILE_WIDTH: i32 = 174;
pub(super) const TILE_HEIGHT: i32 = 58;
/// The result card over the floor.
pub(super) const CARD: (i32, i32, i32, i32) = (70, 84, 300, 150);
