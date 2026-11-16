use super::model::PictureModel;
use super::types::{HISTORY_CAPACITY, PictureCell, PictureTool, SAVE_LEN};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionModal, ExpeditionSaveError,
};

fn completed_model(level: u8) -> PictureModel {
    let mut model = PictureModel::default();
    model.level = level;
    model.reset_board();
    while model.modal() == ExpeditionModal::None {
        assert_ne!(model.reveal_hint(), ChangeSet::NONE);
    }
    model
}

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
fn result_and_final_modals_freeze_picture_commands() {
    for (level, modal) in [
        (0, ExpeditionModal::Result),
        (EXPEDITION_LEVELS as u8 - 1, ExpeditionModal::Final),
    ] {
        let mut model = completed_model(level);
        assert_eq!(model.modal(), modal);
        let before = (
            model.cells,
            model.cursor,
            model.tool(),
            model.history_len(),
            model.hints(),
            model.message(),
        );

        assert_eq!(model.move_cursor(Direction4::Right), ChangeSet::NONE);
        assert_eq!(model.apply_cursor(Some(PictureTool::Mark)), ChangeSet::NONE);
        assert_eq!(model.begin_stroke(0), ChangeSet::NONE);
        assert_eq!(model.continue_stroke(1), ChangeSet::NONE);
        assert_eq!(model.apply_cell(0), ChangeSet::NONE);
        assert_eq!(model.check(), ChangeSet::NONE);
        assert_eq!(
            (
                model.cells,
                model.cursor,
                model.tool(),
                model.history_len(),
                model.hints(),
                model.message(),
            ),
            before
        );
    }
}

#[test]
fn stroke_lifecycle_tracks_uncommitted_changes() {
    let mut model = PictureModel::default();
    let cell = (0..model.level().size() * model.level().size())
        .find(|cell| !model.level().is_given(*cell))
        .expect("Picture level has an editable cell");

    assert!(!model.stroke_active());
    assert_ne!(model.begin_stroke(cell), ChangeSet::NONE);
    assert!(model.stroke_active());
    assert_ne!(model.end_stroke(false), ChangeSet::NONE);
    assert!(!model.stroke_active());
}

#[test]
fn model_memory_is_bounded() {
    assert!(core::mem::size_of::<PictureModel>() <= 2_048);
}

#[test]
fn a_line_counts_as_matched_once_its_runs_equal_the_clues() {
    let mut model = PictureModel::default();
    let level = model.level();
    let size = level.size();
    let full: u32 = (1 << size) - 1;
    let rows = |model: &PictureModel| model.matched_lines() & full;
    // Rows and columns with no runs match from the start; the others wait.
    let empty_rows = (0..size)
        .filter(|row| (0..size).all(|offset| !level.target(row * size + offset)))
        .fold(0, |bits, row| bits | 1 << row);
    assert_eq!(rows(&model), empty_rows);
    let row = (0..size)
        .find(|row| (0..size).any(|offset| level.target(row * size + offset)))
        .unwrap();
    for offset in 0..size {
        if level.target(row * size + offset) {
            model.cells.set(row * size + offset, PictureCell::Filled);
        }
    }
    assert_ne!(rows(&model) & 1 << row, 0);
    let wrong = (0..size)
        .find(|offset| !level.target(row * size + offset))
        .map(|offset| row * size + offset);
    if let Some(cell) = wrong {
        model.cells.set(cell, PictureCell::Filled);
        assert_eq!(rows(&model) & 1 << row, 0);
    }
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
