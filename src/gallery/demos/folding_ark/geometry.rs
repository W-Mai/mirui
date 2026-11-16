//! Where the 10 x 7 map sits on the 480 x 320 surface, shared by drawing
//! and by taps on the map.

use crate::prelude::{Fixed, Point, Rect};

/// The logical size the surface is drawn at.
pub(super) const SURFACE: (i32, i32) = (480, 320);
/// Height of the header bar.
pub(super) const HEADER: i32 = 28;
/// Distance between neighbouring cells, and the drawn cell inside it.
const PITCH: i32 = 34;
const CELL: i32 = 31;
const COLUMNS: i32 = 10;
const ROWS: i32 = 7;
/// Top-left corner of the grid.
const GRID: (i32, i32) = (71, 36);
/// Margin of the panel drawn around the grid.
const PANEL_PAD: i32 = 4;

/// The panel behind the grid, as `(left, top, width, height)`.
pub(super) const fn panel_box() -> (i32, i32, i32, i32) {
    (
        GRID.0 - PANEL_PAD,
        GRID.1 - PANEL_PAD,
        PITCH * (COLUMNS - 1) + CELL + PANEL_PAD * 2,
        PITCH * (ROWS - 1) + CELL + PANEL_PAD * 2,
    )
}

pub(super) fn panel() -> Rect {
    let (left, top, width, height) = panel_box();
    Rect::new(left, top, width, height)
}

/// The drawn square of `cell`.
pub(super) fn cell_rect(cell: u8) -> Rect {
    Rect::new(
        GRID.0 + i32::from(cell % 10) * PITCH,
        GRID.1 + i32::from(cell / 10) * PITCH,
        CELL,
        CELL,
    )
}

pub(super) fn cell_center(cell: u8) -> Point {
    let rect = cell_rect(cell);
    let half = Fixed::from_ratio(CELL, 2);
    Point::new(rect.x + half, rect.y + half)
}

/// The area the hull covers on `cells`, the first `len` of them used; the
/// first cell is its top-left one.
pub(super) fn hull_rect(cells: [u8; 2], len: u8) -> Rect {
    let first = cell_rect(cells[0]);
    let last = cell_rect(cells[usize::from(len.clamp(1, 2)) - 1]);
    Rect {
        x: first.x,
        y: first.y,
        w: last.x + last.w - first.x,
        h: last.y + last.h - first.y,
    }
}
