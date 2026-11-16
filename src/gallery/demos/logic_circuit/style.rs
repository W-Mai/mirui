use crate::prelude::Color;

pub(super) const INK: Color = Color::rgb(32, 41, 50);
pub(super) const BACKGROUND: Color = Color::rgb(237, 238, 232);
pub(super) const PANEL: Color = Color::rgb(252, 252, 246);
pub(super) const WORK: Color = Color::rgb(229, 233, 227);
pub(super) const LINE: Color = Color::rgb(201, 207, 205);
pub(super) const MUTED: Color = Color::rgb(116, 128, 138);
pub(super) const ACCENT: Color = Color::rgb(245, 107, 56);
pub(super) const SIGNAL_OFF: Color = Color::rgb(139, 154, 155);
pub(super) const SIGNAL_ON: Color = ACCENT;
pub(super) const GREEN: Color = Color::rgb(102, 130, 86);
pub(super) const RED: Color = Color::rgb(189, 83, 72);
/// Secondary text on the dark header and tiles.
pub(super) const DIM: Color = Color::rgb(174, 186, 190);
/// The task button on the dark header.
pub(super) const HEADER_BUTTON: Color = Color::rgb(52, 64, 74);
/// A star not earned on the result card.
pub(super) const STAR_EMPTY: Color = Color::rgb(214, 218, 212);

/// The side column right of the board: left, top, width and height.
pub(super) const SIDE: (i32, i32, i32, i32) = (358, 30, 118, 264);
/// Left edge and width of the side column's contents.
pub(super) const SIDE_LEFT: i32 = 364;
pub(super) const SIDE_WIDTH: i32 = 106;
/// The rows of side buttons: board tools, scanner and the main action.
pub(super) const SCAN_TOP: i32 = 196;
pub(super) const TOOLS_TOP: i32 = 228;
pub(super) const PRIMARY_TOP: i32 = 262;
/// The live truth table in the side column.
pub(super) const LIVE_TOP: i32 = 80;
pub(super) const LIVE_STEP: i32 = 15;
/// The input pads: left edge, width and height.
pub(super) const PAD_LEFT: i32 = 8;
pub(super) const PAD_WIDTH: i32 = 36;
pub(super) const PAD_HEIGHT: i32 = 28;
/// The Y pad right of the output pin.
pub(super) const OUTPUT_PAD: (i32, i32, i32, i32) = (322, 144, 28, 36);
/// The result card over the board.
pub(super) const CARD: (i32, i32, i32, i32) = (70, 84, 300, 150);
/// Rows of the full table on the truth page.
pub(super) const TRUTH_TOP: i32 = 74;
pub(super) const TRUTH_STEP: i32 = 24;
