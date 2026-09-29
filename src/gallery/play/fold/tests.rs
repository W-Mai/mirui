use super::model::{FoldModel, SAVE_LEN};
use super::rules::{initial, moved, won};
use super::solver::solve;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionSaveError, fold_level,
};

#[test]
fn all_reference_solutions_complete_at_par() {
    for index in 0..EXPEDITION_LEVELS as u8 {
        let level = fold_level(index).unwrap();
        let mut state = initial(level);
        for step in 0..level.solution_len() {
            state = moved(level, state, level.solution(step).unwrap()).unwrap();
        }
        assert!(won(level, state), "Fold level {index}");
        assert_eq!(level.solution_len(), level.par());
    }
}

#[test]
fn exact_hint_solves_every_initial_state() {
    let mut workspace = ExpeditionHintWorkspace::new();
    for index in 0..EXPEDITION_LEVELS as u8 {
        let level = fold_level(index).unwrap();
        let (_, remaining) = solve(level, initial(level), &mut workspace).unwrap();
        assert_eq!(remaining, u16::from(level.par()), "Fold level {index}");
    }
}

#[test]
fn invalid_moves_do_not_enter_history() {
    let mut model = FoldModel::default();
    let steps = model.steps();
    let changes = model.move_direction(Direction4::Left);
    assert_eq!(model.steps(), steps);
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(!changes.contains(ChangeSet::VISUAL));
    assert!(!changes.contains(ChangeSet::PERSISTENCE));
}

#[test]
fn hints_only_change_observed_and_persisted_state() {
    let mut model = FoldModel::default();
    let changes = model.request_hint(&mut ExpeditionHintWorkspace::new());
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::PERSISTENCE));
    assert!(!changes.contains(ChangeSet::VISUAL));
}

#[test]
fn model_memory_is_bounded() {
    assert!(core::mem::size_of::<FoldModel>() <= 768);
}

#[test]
fn save_round_trip_is_atomic_and_checksummed() {
    let mut model = FoldModel::default();
    model.move_direction(model.level().solution(0).unwrap());
    model.request_hint(&mut ExpeditionHintWorkspace::new());
    let mut bytes = [0; SAVE_LEN];
    model.encode_into(&mut bytes).unwrap();
    let restored = FoldModel::decode(&bytes).unwrap();
    assert_eq!(restored.level, model.level);
    assert_eq!(restored.state, model.state);
    assert_eq!(restored.actions.len(), model.actions.len());
    assert_eq!(restored.hints, model.hints);
    bytes[20] ^= 1;
    assert!(matches!(
        FoldModel::decode(&bytes),
        Err(ExpeditionSaveError::InvalidChecksum)
    ));
}
