use super::model::PictureModel;
use super::types::{HISTORY_CAPACITY, PictureCell, SAVE_LEN};
use crate::gallery::play::expeditions::{EXPEDITION_LEVELS, ExpeditionSaveError};

#[test]
fn generated_clues_match_every_target() {
    for level_index in 0..EXPEDITION_LEVELS as u8 {
        let mut model = PictureModel::default();
        model.level = level_index;
        model.reset_board();
        for row in 0..model.level().size() {
            let mut clues = [0; 5];
            assert!(model.clues(true, row, &mut clues) > 0);
            assert!(model.clues(false, row, &mut clues) > 0);
        }
    }
}

#[test]
fn cancellation_restores_the_entire_stroke() {
    let mut model = PictureModel::default();
    let before = model.cells;
    model.begin_stroke(0);
    model.continue_stroke(24);
    model.end_stroke(true);
    assert_eq!(model.cells, before);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn completion_ignores_unknown_empty_cells() {
    let mut model = PictureModel::default();
    let level = model.level();
    for cell in 0..level.size() * level.size() {
        if level.target(cell) {
            model.cells.set(cell, PictureCell::Filled);
        }
    }
    assert!(model.complete());
}

#[test]
fn history_is_bounded_to_sixty_four_strokes() {
    let mut model = PictureModel::default();
    for _ in 0..80 {
        model.begin_stroke(0);
        model.end_stroke(false);
    }
    assert_eq!(model.history_len(), HISTORY_CAPACITY as u8);
}

#[test]
fn observed_cells_cannot_be_changed() {
    let mut model = PictureModel::default();
    model.level = 12;
    model.reset_board();
    let (cell, _) = model.level().given(0).unwrap();
    let before = model.cell(cell);
    model.begin_stroke(cell);
    assert_eq!(model.cell(cell), before);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn model_memory_is_bounded() {
    assert!(core::mem::size_of::<PictureModel>() <= 2_048);
}

#[test]
fn save_round_trip_preserves_board_history_and_rejects_corruption() {
    let mut model = PictureModel::default();
    model.begin_stroke(0);
    model.end_stroke(false);
    model.cursor = 3;
    let mut bytes = [0; SAVE_LEN];
    model.encode_into(&mut bytes).unwrap();
    let restored = PictureModel::decode(&bytes).unwrap();
    assert_eq!(restored.level, model.level);
    assert_eq!(restored.cells, model.cells);
    assert_eq!(restored.history_len, model.history_len);
    assert_eq!(restored.cursor, model.cursor);
    bytes[20] ^= 1;
    assert!(matches!(
        PictureModel::decode(&bytes),
        Err(ExpeditionSaveError::InvalidChecksum)
    ));
}
