use super::model::CircuitModel;
use super::types::{
    CircuitError, CircuitModal, CircuitPage, GateKind, MAX_DRIVEN_INPUTS, MAX_GATES, MAX_TRACE,
    MAX_UNDO, SignalSource, TASK_COUNT,
};
use crate::gallery::play::change::ChangeSet;

#[test]
fn reference_circuits_pass_exhaustive_truth_tables() {
    for task in 0..TASK_COUNT as u8 {
        let mut model = CircuitModel::default();
        model.load_task(task, true);
        model.verify();
        assert_eq!(model.verify_result().is_some_and(|result| result.won), true);
    }
}

#[test]
fn starter_circuits_are_incomplete_or_incorrect() {
    for task in 0..TASK_COUNT as u8 {
        let mut model = CircuitModel::default();
        model.load_task(task, false);
        model.verify();
        assert_eq!(
            model.verify_result().is_some_and(|result| result.won),
            false
        );
    }
}

#[test]
fn xor_edit_fixes_first_task() {
    let mut model = CircuitModel::default();
    model.set_selected_kind(GateKind::Xor).unwrap();
    model.verify();
    assert_eq!(model.verify_result().is_some_and(|result| result.won), true);
}

#[test]
fn direct_cycle_rejection_is_atomic() {
    let mut model = CircuitModel::default();
    let before = model.snapshot();
    model.select_source(SignalSource::gate(1));
    assert_eq!(
        model.connect_pending_to(Some(1), 0),
        Err(CircuitError::Cycle)
    );
    assert_eq!(model.snapshot(), before);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn indirect_cycle_rejection_is_atomic() {
    let mut model = CircuitModel::default();
    model.add_gate(GateKind::And).unwrap();
    let second = model.selected();
    model.select_source(SignalSource::gate(1));
    model.connect_pending_to(Some(second), 0).unwrap();
    let before = model.snapshot();
    model.select_source(SignalSource::gate(second));
    assert_eq!(
        model.connect_pending_to(Some(1), 1),
        Err(CircuitError::Cycle)
    );
    assert_eq!(model.snapshot(), before);
}

#[test]
fn removing_a_gate_detaches_every_reference() {
    let mut model = CircuitModel::default();
    model.load_task(1, true);
    model.select_gate(4);
    model.remove_selected().unwrap();
    assert_eq!(model.gate_by_id(5).unwrap().a, SignalSource::None);
    model.select_gate(5);
    model.remove_selected().unwrap();
    assert_eq!(model.output_source(), SignalSource::None);
}

#[test]
fn undo_restores_deleted_gate_and_wires() {
    let mut model = CircuitModel::default();
    model.load_task(1, true);
    let before = model.snapshot();
    model.select_gate(4);
    model.remove_selected().unwrap();
    model.undo();
    assert_eq!(model.snapshot(), before);
}

#[test]
fn gate_and_history_capacity_are_bounded() {
    let mut model = CircuitModel::default();
    model.load_task(0, false);
    while model.gate_len() < MAX_GATES as u8 {
        model.add_gate(GateKind::And).unwrap();
    }
    assert_eq!(model.add_gate(GateKind::Or), Err(CircuitError::Full));
    for index in 0..90 {
        model
            .set_selected_kind(if index % 2 == 0 {
                GateKind::Or
            } else {
                GateKind::Xor
            })
            .unwrap();
    }
    assert_eq!(model.history_len(), MAX_UNDO as u8);
}

#[test]
fn drag_cancel_and_overlap_do_not_write_history() {
    let mut model = CircuitModel::default();
    model.load_task(1, true);
    let history = model.history_len();
    let origin = model.visual_gate_position(1);
    model.begin_drag(1);
    assert_eq!(model.end_drag(), Ok(ChangeSet::VISUAL));
    model.move_drag(180, 180);
    assert_eq!(model.visual_gate_position(1), origin);
    assert_eq!(model.history_len(), history);
    model.begin_drag(1);
    model.move_drag(180, 180);
    assert_eq!(model.cancel_drag(), ChangeSet::VISUAL);
    assert_eq!(model.visual_gate_position(1), origin);
    assert_eq!(model.history_len(), history);
    model.begin_drag(1);
    let gate = model.gate_by_id(2).unwrap();
    model.move_drag(gate.x, gate.y);
    let before = model.snapshot();
    let drag_position = model.visual_gate_position(1);
    assert_eq!(model.end_drag(), Err(CircuitError::Overlap));
    assert_eq!(model.snapshot(), before);
    assert_eq!(model.visual_gate_position(1), drag_position);
    assert_eq!(model.history_len(), history);
}

#[test]
fn not_gate_ignores_and_clears_second_input() {
    let mut model = CircuitModel::default();
    model.set_selected_kind(GateKind::Not).unwrap();
    assert_eq!(model.gate_by_id(1).unwrap().b, SignalSource::None);
    model.inputs = 0;
    assert_eq!(model.evaluation().value, true);
}

#[test]
fn verification_does_not_mutate_manual_inputs() {
    let mut model = CircuitModel::default();
    model.toggle_input(0);
    let before = model.inputs;
    model.verify();
    assert_eq!(model.inputs, before);
}

#[test]
fn trace_ring_retains_the_latest_thirty_two_samples() {
    let mut model = CircuitModel::default();
    for _ in 0..140 {
        model.step_trace();
    }
    assert_eq!(model.trace_len(), MAX_TRACE as u8);
    let mut seen = [false; 4];
    for index in 28..32 {
        seen[usize::from(model.trace(index).unwrap().inputs)] = true;
    }
    assert!(seen.into_iter().all(|present| present));
}

#[test]
fn modal_and_pause_discard_scan_time_debt() {
    let mut model = CircuitModel::default();
    model.set_page(CircuitPage::Trace);
    model.toggle_scanning();
    model.advance_ms(400);
    model.open_modal(CircuitModal::Help);
    model.advance_ms(400);
    model.close_modal();
    assert_eq!(model.advance_ms(100), ChangeSet::NONE);
    assert_eq!(model.trace_len(), 0);
}

#[test]
fn fixed_storage_budget_stays_small() {
    assert!(core::mem::size_of::<CircuitModel>() <= 4096);
    assert_eq!(MAX_DRIVEN_INPUTS, MAX_GATES * 2 + 1);
}
