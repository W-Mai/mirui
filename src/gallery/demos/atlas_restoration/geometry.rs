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
    let size = model.level().size();
    let mut max_column_clues = 1;
    for column in 0..size {
        let mut clues = [0; 5];
        max_column_clues = max_column_clues.max(model.clues(false, column, &mut clues));
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
