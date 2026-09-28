use super::model::PixelModel;
use super::types::{COLOR_COUNT, FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelFrames, PixelTool};
use crate::gallery::play::change::ChangeSet;

#[test]
fn templates_have_four_independent_legal_frames() {
    for template in 0..3 {
        let mut frames = PixelFrames::template(template).unwrap();
        for frame in 0..FRAME_COUNT {
            for y in 0..GRID_HEIGHT {
                for x in 0..GRID_WIDTH {
                    assert!(frames.get(frame, x, y) <= COLOR_COUNT);
                }
            }
        }
        let before = frames.get(1, 0, 0);
        frames.set(0, 0, 0, 5);
        assert_eq!(frames.get(1, 0, 0), before);
    }
}

#[test]
fn one_drag_is_one_undo_step_and_bresenham_has_no_gaps() {
    let mut model = PixelModel::default();
    model.frames.clear_frame(0);
    let original = model.frames;
    model.begin_stroke(0, 0);
    model.continue_stroke(11, 11);
    model.end_stroke(false);
    assert_eq!(model.history_len(), 1);
    for coordinate in 0..12 {
        assert_eq!(model.frames.get(0, coordinate, coordinate), 1);
    }
    model.undo();
    assert_eq!(model.frames, original);
}

#[test]
fn cancelled_stroke_restores_preview_without_history() {
    let mut model = PixelModel::default();
    let original = model.frames;
    model.begin_stroke(0, 0);
    model.continue_stroke(11, 0);
    model.end_stroke(true);
    assert_eq!(model.frames, original);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn tap_is_one_undo_step() {
    let mut model = PixelModel::default();
    model.frames.clear_frame(0);
    let original = model.frames;
    assert!(model.paint_cell(0, 0).contains(ChangeSet::PERSISTENCE));
    assert_eq!(model.history_len(), 1);
    model.undo();
    assert_eq!(model.frames, original);
}

#[test]
fn mirror_and_eraser_apply_to_both_sides() {
    let mut model = PixelModel::default();
    model.frames.clear_frame(0);
    model.select_color(3);
    model.toggle_mirror();
    model.begin_stroke(0, 0);
    model.end_stroke(false);
    assert_eq!(model.frames.get(0, 0, 0), 3);
    assert_eq!(model.frames.get(0, 11, 0), 3);
    model.set_tool(PixelTool::Erase);
    model.begin_stroke(0, 0);
    model.end_stroke(false);
    assert_eq!(model.frames.get(0, 0, 0), 0);
    assert_eq!(model.frames.get(0, 11, 0), 0);
}

#[test]
fn copy_clear_and_template_load_are_atomic_and_undoable() {
    let mut model = PixelModel::default();
    model.select_frame(1);
    model.copy_previous();
    assert_eq!(model.frames.get(1, 5, 1), model.frames.get(0, 5, 1));
    model.open_clear();
    model.confirm_clear();
    for y in 0..GRID_HEIGHT {
        for x in 0..GRID_WIDTH {
            assert_eq!(model.frames.get(1, x, y), 0);
        }
    }
    model.undo();
    assert_ne!(model.frames.get(1, 5, 1), 0);
    let before = model.frames;
    model.open_templates();
    model.load_template(1);
    assert_ne!(model.frames, before);
    model.undo();
    assert_eq!(model.frames, before);
}

#[test]
fn no_op_stroke_does_not_consume_history() {
    let mut model = PixelModel::default();
    model.set_tool(PixelTool::Erase);
    model.begin_stroke(0, 0);
    model.end_stroke(false);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn history_is_capped_at_sixteen_entries() {
    let mut model = PixelModel::default();
    for index in 0..50 {
        model.open_templates();
        model.load_template((index % 3) as u8);
    }
    assert_eq!(model.history_len(), 16);
    for _ in 0..16 {
        assert!(model.undo().contains(ChangeSet::MODEL));
    }
    assert_eq!(model.undo(), ChangeSet::NONE);
}

#[test]
fn playback_advances_preview_without_mutating_pixels() {
    let mut model = PixelModel::default();
    let frames = model.frames;
    model.toggle_playback();
    assert_eq!(model.advance_ms(249), ChangeSet::NONE);
    assert!(model.advance_ms(1).contains(ChangeSet::VISUAL));
    assert_eq!(model.visible_frame(), 1);
    assert_eq!(model.frames, frames);
    model.toggle_playback();
    assert_eq!(model.advance_ms(500), ChangeSet::NONE);
}

#[test]
fn model_storage_is_bounded() {
    assert!(core::mem::size_of::<PixelModel>() <= 5_500);
    assert_eq!(core::mem::size_of::<PixelFrames>(), 288);
}
