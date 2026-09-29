use crate::gallery::play::expeditions::picture_level;
use crate::gallery::play::picture::PictureModel;

const BOARD_AREA_X: i32 = 91;
const BOARD_AREA_WIDTH: i32 = 172;
const BOARD_AREA_HEIGHT: i32 = 191;
#[derive(Clone, Copy)]
pub(super) struct BoardGeometry {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) cell: i32,
    pub(super) size: i32,
}

pub(super) fn board_geometry(model: &PictureModel) -> BoardGeometry {
    board_geometry_for_level(model.level_index())
}

pub(super) fn board_geometry_for_level(level_index: u8) -> BoardGeometry {
    let level = picture_level(level_index).expect("validated Picture level");
    let size = level.size();
    let mut max_column_clues = 1;
    for column in 0..size {
        let mut clues = [0; 5];
        max_column_clues =
            max_column_clues.max(clues_for_level(level_index, false, column, &mut clues));
    }
    let cell = 26.min(
        (BOARD_AREA_HEIGHT - i32::try_from(max_column_clues).unwrap_or(1) * 10) / i32::from(size),
    );
    let board = cell * i32::from(size);
    BoardGeometry {
        x: BOARD_AREA_X + (BOARD_AREA_WIDTH - board) / 2,
        y: 61 + i32::try_from(max_column_clues).unwrap_or(1) * 10,
        cell,
        size: board,
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
