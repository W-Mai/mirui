use crate::gallery::play::expeditions::picture_level;
use crate::gallery::play::picture::PictureModel;

/// The area that holds the board and its clues.
const AREA_LEFT: i32 = 8;
const AREA_TOP: i32 = 32;
const AREA_WIDTH: i32 = 388;
const AREA_HEIGHT: i32 = 260;
/// The height of one stacked column clue, and the width of one row clue.
pub(super) const CLUE_LINE: i32 = 12;
const CLUE_WIDTH: i32 = 11;
const MAX_CELL: i32 = 44;

#[derive(Clone, Copy)]
pub(super) struct BoardGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) cell: i32,
    pub(super) size: i32,
    /// The width of the row clues left of the board.
    pub(super) row_clues: i32,
    /// The height of the column clues above the board.
    pub(super) column_clues: i32,
}

pub(super) fn board_geometry(model: &PictureModel) -> BoardGeometry {
    board_geometry_for_level(model.level_index())
}

/// The largest square cells that leave room for the level's longest clues;
/// the board and its clues sit in the middle of the area.
pub(super) fn board_geometry_for_level(level_index: u8) -> BoardGeometry {
    let level = picture_level(level_index).expect("validated Picture level");
    let size = level.size();
    let (mut rows, mut columns) = (1, 1);
    for line in 0..size {
        let mut clues = [0; 5];
        rows = rows.max(clues_for_level(level_index, true, line, &mut clues));
        columns = columns.max(clues_for_level(level_index, false, line, &mut clues));
    }
    let row_clues = i32::try_from(rows).unwrap_or(5) * CLUE_WIDTH + 6;
    let column_clues = i32::try_from(columns).unwrap_or(5) * CLUE_LINE + 4;
    let cells = i32::from(size);
    let cell = MAX_CELL
        .min((AREA_HEIGHT - column_clues) / cells)
        .min((AREA_WIDTH - row_clues) / cells);
    let board = cell * cells;
    BoardGeometry {
        x: AREA_LEFT + row_clues + (AREA_WIDTH - row_clues - board) / 2,
        y: AREA_TOP + column_clues + (AREA_HEIGHT - column_clues - board) / 2,
        cell,
        size: board,
        row_clues,
        column_clues,
    }
}

pub(super) fn clues_for_level(level_index: u8, row: bool, line: u8, output: &mut [u8; 5]) -> usize {
    let level = picture_level(level_index).expect("validated Picture level");
    let size = level.size();
    let mut len = 0;
    let mut run = 0;
    for offset in 0..=size {
        let filled = if offset == size {
            false
        } else {
            let cell = if row {
                line * size + offset
            } else {
                offset * size + line
            };
            level.target(cell)
        };
        if filled {
            run += 1;
        } else if run != 0 {
            output[len] = run;
            len += 1;
            run = 0;
        }
    }
    if len == 0 {
        output[0] = 0;
        1
    } else {
        len
    }
}
