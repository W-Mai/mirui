use super::*;

fn solved_orientations(level: &Level) -> [MirrorOrientation; MAX_MIRRORS] {
    let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
    for (slot, mirror) in orientations.iter_mut().zip(level.mirrors) {
        *slot = mirror.solution;
    }
    orientations
}

#[test]
fn every_level_starts_unsolved_and_reference_solution_reaches_receiver() {
    for level in &LEVELS {
        let mut initial = [MirrorOrientation::Slash; MAX_MIRRORS];
        for (slot, orientation) in initial.iter_mut().zip(level.initial) {
            *slot = *orientation;
        }
        assert!(!trace(level, &initial).solved, "{}", level.name);
        let solved = trace(level, &solved_orientations(level));
        assert!(solved.solved, "{}", level.name);
        assert_eq!(solved.stop, TraceStop::Receiver);
        assert!(solved.points().len() <= MAX_TRACE_POINTS);
    }
}

#[test]
fn all_orientation_combinations_terminate_within_the_budget() {
    for level in &LEVELS {
        for mask in 0..(1_usize << level.mirrors.len()) {
            let mut orientations = [MirrorOrientation::Slash; MAX_MIRRORS];
            for (index, slot) in orientations
                .iter_mut()
                .enumerate()
                .take(level.mirrors.len())
            {
                *slot = if mask & (1 << index) == 0 {
                    MirrorOrientation::Backslash
                } else {
                    MirrorOrientation::Slash
                };
            }
            let result = trace(level, &orientations);
            assert!(result.steps <= 140);
            assert!(result.points().len() <= MAX_TRACE_POINTS);
        }
    }
}

#[test]
fn rotate_and_undo_restore_orientation_and_move_count() {
    let mut model = LumenModel::new();
    let before = model.orientations;
    assert!(model.rotate(0).contains(ChangeSet::MODEL));
    assert_eq!(model.moves, 1);
    assert!(model.trace.solved);
    assert!(model.undo().contains(ChangeSet::MODEL));
    assert_eq!(model.orientations, before);
    assert_eq!(model.moves, 0);
}

#[test]
fn successful_visit_remains_recorded_after_undo() {
    let mut model = LumenModel::new();
    model.rotate(0);
    model.undo();
    assert!(model.is_completed(0));
    assert!(!model.trace.solved);
}

#[test]
fn invalid_mirror_or_level_does_not_mutate() {
    let mut model = LumenModel::new();
    let before = model.clone();
    assert_eq!(model.rotate(88), ChangeSet::NONE);
    assert_eq!(model.select_level(88), ChangeSet::NONE);
    assert_eq!(model.orientations, before.orientations);
    assert_eq!(model.level_index, before.level_index);
    assert_eq!(model.moves, before.moves);
}

#[test]
fn hint_identifies_mismatch_without_rotating() {
    let mut model = LumenModel::new();
    let before = model.orientations;
    model.reveal_hint();
    assert_eq!(model.hint(), Some(0));
    assert_eq!(model.orientations, before);
}

#[test]
fn undo_is_bounded_to_the_latest_thirty_two_actions() {
    let mut model = LumenModel::new();
    for _ in 0..100 {
        model.rotate(0);
    }
    assert_eq!(model.history_len, 32);
    for _ in 0..32 {
        assert_ne!(model.undo(), ChangeSet::NONE);
    }
    assert_eq!(model.undo(), ChangeSet::NONE);
}

#[test]
fn modal_blocks_board_mutation_and_animation() {
    let mut model = LumenModel::new();
    model.open_levels();
    let before = model.orientations;
    assert_eq!(model.rotate_cell(2, 3), ChangeSet::NONE);
    assert_eq!(model.advance_ms(16), ChangeSet::NONE);
    assert_eq!(model.orientations, before);
}

#[test]
fn scan_animation_stops_without_accumulating_time() {
    let mut model = LumenModel::new();
    let start = model.scan_phase;
    assert!(model.advance_ms(16).contains(ChangeSet::VISUAL));
    assert_ne!(model.scan_phase, start);
    model.toggle_scan();
    let stopped = model.scan_phase;
    assert_eq!(model.advance_ms(1000), ChangeSet::NONE);
    assert_eq!(model.scan_phase, stopped);
}

#[test]
fn model_storage_stays_within_embedded_budget() {
    assert!(core::mem::size_of::<LumenModel>() <= 512);
}
