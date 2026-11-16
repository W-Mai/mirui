//! Where the two boards sit on the 480 x 320 surface, shared by drawing
//! and by taps on the boards.

use crate::prelude::{Fixed, Point, Rect};

/// The logical size the surface is drawn at.
pub(super) const SURFACE: (i32, i32) = (480, 320);
/// Height of the header bar.
pub(super) const HEADER: i32 = 28;
/// Distance between neighbouring cells, and the drawn cell inside it.
pub(super) const PITCH: i32 = 36;
pub(super) const CELL: i32 = 33;
/// Top-left corner of each station's 6 x 6 grid.
const GRID: [(i32, i32); 2] = [(14, 51), (253, 51)];
/// Margin of the panel drawn around each grid.
const PANEL_PAD: i32 = 5;
/// Width and height of a grid: six cells, five gaps.
const GRID_SIZE: i32 = PITCH * 5 + CELL;

/// The panel behind one station's grid, as `(left, top, size)`.
pub(super) const fn panel_box(station: usize) -> (i32, i32, i32) {
    let (x, y) = GRID[station];
    (x - PANEL_PAD, y - PANEL_PAD, GRID_SIZE + PANEL_PAD * 2)
}

/// The panel behind one station's grid.
pub(super) fn panel(station: usize) -> Rect {
    let (left, top, size) = panel_box(station);
    Rect::new(left, top, size, size)
}

/// The area that takes a station's taps, `(left, top, width, height)`: its
/// panel, widened to meet the other station's in the middle.
pub(super) const fn target_box(station: usize) -> (i32, i32, i32, i32) {
    let (left, top, size) = panel_box(station);
    let middle = SURFACE.0 / 2;
    if station == 0 {
        (left, top, middle - left, size)
    } else {
        (middle, top, left + size - middle, size)
    }
}

/// The drawn square of `cell` on `station`'s grid.
pub(super) fn cell_rect(station: usize, cell: u8) -> Rect {
    let (x, y) = GRID[station];
    Rect::new(
        x + i32::from(cell % 6) * PITCH,
        y + i32::from(cell / 6) * PITCH,
        CELL,
        CELL,
    )
}

pub(super) fn cell_center(station: usize, cell: u8) -> Point {
    let rect = cell_rect(station, cell);
    let half = Fixed::from_ratio(CELL, 2);
    Point::new(rect.x + half, rect.y + half)
}
