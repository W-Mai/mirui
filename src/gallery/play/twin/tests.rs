use super::model::{SAVE_LEN, TwinModel};
use super::rules::{initial, moved, won};
use super::solver::solve;
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::{
    Direction4, EXPEDITION_LEVELS, ExpeditionHintWorkspace, ExpeditionSaveError, twin_level,
};

#[test]
fn all_reference_solutions_complete_at_par() {
    for index in 0..EXPEDITION_LEVELS as u8 {
        let level = twin_level(index).unwrap();
        let mut state = initial(level);
        for step in 0..level.solution_len() {
            state = moved(level, state, level.solution(step).unwrap()).unwrap();
        }
        assert!(won(level, state), "Twin level {index}");
        assert_eq!(level.solution_len(), level.par());
    }
}

#[test]
fn key_opens_doors_only_after_the_collecting_move() {
    for index in 0..EXPEDITION_LEVELS as u8 {
        let level = twin_level(index).unwrap();
        if level.key().is_none() {
            continue;
        }
        let mut state = initial(level);
        for step in 0..level.solution_len() {
            let before = state;
            state = moved(level, state, level.solution(step).unwrap()).unwrap();
            if !before.key && state.key {
                assert_eq!(level.key(), Some(state.a));
                return;
            }
        }
    }
    panic!("no key was collected");
}

#[test]
fn exact_hint_solves_every_initial_state() {
    let mut workspace = ExpeditionHintWorkspace::new();
    for index in 0..EXPEDITION_LEVELS as u8 {
        let level = twin_level(index).unwrap();
        let (_, remaining) = solve(level, initial(level), &mut workspace).unwrap();
        assert_eq!(remaining, u16::from(level.par()), "Twin level {index}");
    }
}

#[test]
fn invalid_moves_only_change_observed_message() {
    let mut model = TwinModel::default();
    let steps = model.steps();
    let direction = Direction4::ALL
        .into_iter()
        .find(|direction| moved(model.level(), model.state, *direction).is_none())
        .expect("initial Twin state has a blocked direction");
    let changes = model.move_direction(direction);
    assert_eq!(model.steps(), steps);
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(!changes.contains(ChangeSet::VISUAL));
    assert!(!changes.contains(ChangeSet::PERSISTENCE));
}

#[test]
fn hints_only_change_observed_and_persisted_state() {
    let mut model = TwinModel::default();
    let changes = model.request_hint(&mut ExpeditionHintWorkspace::new());
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::PERSISTENCE));
    assert!(!changes.contains(ChangeSet::VISUAL));
}

#[test]
fn undo_replays_without_restoring_hint_usage() {
    let mut model = TwinModel::default();
    let direction = model.level().solution(0).unwrap();
    model.move_direction(direction);
    let before = model.state;
    model.request_hint(&mut ExpeditionHintWorkspace::new());
    assert_eq!(model.hints(), 1);
    model.undo();
    assert_ne!(model.state, before);
    assert_eq!(model.hints(), 1);
}

#[test]
fn model_memory_is_bounded() {
    assert!(core::mem::size_of::<TwinModel>() <= 768);
    assert!(core::mem::size_of::<ExpeditionHintWorkspace>() <= 8_128);
}

#[test]
fn save_round_trip_is_atomic_and_checksummed() {
    let mut model = TwinModel::default();
    model.move_direction(model.level().solution(0).unwrap());
    model.request_hint(&mut ExpeditionHintWorkspace::new());
    let mut bytes = [0; SAVE_LEN];
    model.encode_into(&mut bytes).unwrap();
    let restored = TwinModel::decode(&bytes).unwrap();
    assert_eq!(restored.level, model.level);
    assert_eq!(restored.state, model.state);
    assert_eq!(restored.actions.len(), model.actions.len());
    assert_eq!(restored.hints, model.hints);
    bytes[20] ^= 1;
    assert!(matches!(
        TwinModel::decode(&bytes),
        Err(ExpeditionSaveError::InvalidChecksum)
    ));
}
