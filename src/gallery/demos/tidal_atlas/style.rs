use crate::prelude::Color;
use crate::ui::widgets::{ParagraphStyle, TextAlign};

pub(super) const BACKGROUND: Color = Color::rgb(23, 55, 64);
pub(super) const HEADER: Color = Color::rgb(27, 63, 72);
pub(super) const PANEL: Color = Color::rgb(32, 68, 75);
pub(super) const BOARD: Color = Color::rgb(24, 61, 71);
pub(super) const LINE: Color = Color::rgb(59, 91, 95);
pub(super) const TEXT: Color = Color::rgb(231, 234, 215);
pub(super) const MUTED: Color = Color::rgb(145, 170, 160);
pub(super) const ACCENT: Color = Color::rgb(223, 186, 118);
pub(super) const PAPER: Color = Color::rgb(225, 222, 196);
pub(super) const DARK: Color = Color::rgb(36, 62, 64);
pub(super) const CONTROL: Color = Color::rgb(42, 79, 84);
/// A land tile's cell.
pub(super) const LAND: Color = Color::rgb(51, 86, 89);
pub(super) const CLEAR: Color = Color::rgba(0, 0, 0, 0);

/// The 6x6 board: its top-left corner, the cell size and the step between
/// cells.
pub(super) const BOARD_X: i32 = 6;
pub(super) const BOARD_Y: i32 = 10;
pub(super) const CELL: i32 = 48;
pub(super) const PITCH: i32 = 50;
/// The side panel right of the board.
pub(super) const PANEL_X: i32 = 314;
pub(super) const PANEL_W: i32 = 160;
/// The three tiles in hand, as large as board cells.
pub(super) const RACK_Y: i32 = 118;
pub(super) const RACK_PITCH: i32 = 56;
/// Where a harvest score sits in its cell before it rises.
pub(super) const GAIN_TOP: i32 = 18;

/// The top-left corner of a board cell.
pub(super) const fn cell_origin(index: usize) -> (i32, i32) {
    (
        BOARD_X + (index % 6) as i32 * PITCH,
        BOARD_Y + (index / 6) as i32 * PITCH,
    )
}

/// `color` with its opacity in eighths.
pub(super) const fn eighths(color: Color, alpha: u8) -> Color {
    let alpha = if alpha > 8 { 8 } else { alpha };
    Color {
        a: (alpha as u16 * 255 / 8) as u8,
        ..color
    }
}

pub(super) fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

pub(super) fn end() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::End)
}

pub(super) fn center() -> ParagraphStyle {
    ParagraphStyle::label()
}
