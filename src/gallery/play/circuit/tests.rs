use super::model::CircuitModel;
use super::tasks::{Board, SET_SIZE, TASK_COUNT, TASKS};
use super::types::{
    CircuitError, CircuitModal, CircuitPage, GateKind, MAX_DRIVEN_INPUTS, MAX_GATES, MAX_TRACE,
    MAX_UNDO, SignalSource,
};
use crate::gallery::play::change::ChangeSet;

/// Y for every row in table order (A is the leftmost column), written out
/// by hand so a typo in a task rule shows up here.
const TABLES: [&str; TASK_COUNT] = [
    "10",       // NOT A
    "0001",     // A AND B
    "0010",     // A AND NOT B
    "1001",     // A XNOR B
    "10",       // NOT A
    "0001",     // A AND B
    "0111",     // A OR B
    "0110",     // A XOR B
    "01101001", // A XOR B XOR C
    "00110101", // A ? C : B
    "00010111", // majority
    "01101000", // exactly one
];

fn solve(model: &mut CircuitModel) -> bool {
    model.verify();
    model.verify_result().is_some_and(|result| result.won)
}

fn assert_board_fits(task: usize, board: Board) {
    let spec = &TASKS[task];
    assert!(board.gates.len() <= MAX_GATES, "{}", spec.name);
    for (index, gate) in board.gates.iter().enumerate() {
        assert!(spec.gates.contains(gate.kind), "{}", spec.name);
        assert!(
            (100..=243).contains(&gate.x),
            "{} gate {}",
            spec.name,
            index + 1
        );
        assert!(
            (84..=230).contains(&gate.y),
            "{} gate {}",
            spec.name,
            index + 1
        );
        for other in &board.gates[index + 1..] {
            assert!(
                (gate.x - other.x).abs() >= 62 || (gate.y - other.y).abs() >= 38,
                "{} gates overlap",
                spec.name
            );
        }
        for source in [gate.a, gate.b, board.output] {
            match source {
                SignalSource::Input(input) => assert!(input < spec.inputs, "{}", spec.name),
                SignalSource::Gate(id) => {
                    assert!((1..=board.gates.len() as u8).contains(&id), "{}", spec.name)
                }
                SignalSource::None => {}
            }
        }
    }
}

/// Clears the starter and rebuilds the reference with the player's tools:
/// add each gate, then wire every pin and Y.
fn build_reference(model: &mut CircuitModel) {
    while model.gate_len() > 0 {
        let first = model.gate(0).unwrap().id;
        model.select_gate(first);
        model.remove_selected().unwrap();
    }
    let reference = TASKS[usize::from(model.task())].reference;
    let mut ids = [0_u8; MAX_GATES];
    for (slot, spec) in ids.iter_mut().zip(reference.gates) {
        model.add_gate(spec.kind).unwrap();
        *slot = model.selected();
    }
    let placed = |source: SignalSource| match source {
        SignalSource::Gate(id) => SignalSource::gate(ids[usize::from(id) - 1]),
        other => other,
    };
    for (index, spec) in reference.gates.iter().enumerate() {
        let pins: &[SignalSource] = if spec.kind == GateKind::Not {
            &[spec.a]
        } else {
            &[spec.a, spec.b]
        };
        for (pin, source) in pins.iter().enumerate() {
            model.select_source(placed(*source));
            model
                .connect_pending_to(Some(ids[index]), pin as u8)
                .unwrap();
        }
    }
    model.select_source(placed(reference.output));
    model.connect_pending_to(None, 0).unwrap();
}

#[test]
fn task_rules_match_their_truth_tables() {
    for (task, table) in TABLES.iter().enumerate() {
        let mut model = CircuitModel::default();
        model.load_task(task as u8, false);
        assert_eq!(table.len(), 1 << TASKS[task].inputs, "{}", TASKS[task].name);
        for (row, expected) in table.bytes().enumerate() {
            let truth = model.truth_row(row as u8).unwrap();
            assert_eq!(
                truth.expected,
                expected == b'1',
                "{} row {row}",
                TASKS[task].name
            );
        }
        assert!(model.truth_row(table.len() as u8).is_none());
    }
}

#[test]
fn reference_boards_pass_every_row_within_six_gates() {
    for task in 0..TASK_COUNT {
        assert_board_fits(task, TASKS[task].reference);
        let mut model = CircuitModel::default();
        model.load_task(task as u8, true);
        assert!(solve(&mut model), "{}", TASKS[task].name);
    }
}

#[test]
fn starter_boards_use_allowed_gates_and_do_not_pass() {
    for task in 0..TASK_COUNT {
        assert_board_fits(task, TASKS[task].starter);
        let mut model = CircuitModel::default();
        model.load_task(task as u8, false);
        assert!(!solve(&mut model), "{}", TASKS[task].name);
    }
}

#[test]
fn each_set_opens_with_a_small_task() {
    for set in TASKS.chunks(SET_SIZE) {
        assert!(set[0].reference.gates.len() <= 2, "{}", set[0].name);
        assert!(set.iter().all(|task| task.gates == set[0].gates));
    }
}

#[test]
fn the_whole_course_can_be_built_by_hand() {
    let mut model = CircuitModel::default();
    for task in 0..TASK_COUNT as u8 {
        assert_eq!(model.task(), task);
        build_reference(&mut model);
        assert!(solve(&mut model), "{}", TASKS[usize::from(task)].name);
        assert!(model.controls().won);
        model.verify_or_next();
    }
    assert_eq!(model.solved_mask().count_ones(), TASK_COUNT as u32);
    assert_eq!(model.modal(), CircuitModal::Tasks);
    assert_eq!(model.task(), TASK_COUNT as u8 - 1);
}

#[test]
fn nand_tasks_refuse_every_other_gate() {
    let mut model = CircuitModel::default();
    model.load_task(5, false);
    assert_eq!(model.allowed().single(), Some(GateKind::Nand));
    for kind in [GateKind::And, GateKind::Or, GateKind::Xor, GateKind::Not] {
        assert_eq!(model.add_gate(kind), Err(CircuitError::NotAllowed));
        assert_eq!(model.set_selected_kind(kind), Err(CircuitError::NotAllowed));
    }
    assert_eq!(model.gate(0).unwrap().kind, GateKind::Nand);
    assert_eq!(model.history_len(), 0);

    model.open_modal(CircuitModal::GateTypes { adding: true });
    model.choose_modal_option(0);
    assert_eq!(model.notice(), Some(CircuitError::NotAllowed));
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.gate_len(), 1);
}

#[test]
fn single_kind_tasks_add_and_delete_without_the_picker() {
    let mut model = CircuitModel::default();
    model.load_task(6, false);
    model.start_add_gate();
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.gate_len(), 2);
    assert_eq!(model.gate(1).unwrap().kind, GateKind::Nand);
    model.start_edit_gate();
    assert_eq!(model.gate_len(), 1);

    model.load_task(0, false);
    model.start_add_gate();
    assert_eq!(model.modal(), CircuitModal::GateTypes { adding: true });
}

#[test]
fn edit_without_a_selection_says_what_to_do() {
    let mut model = CircuitModel::default();
    model.load_task(3, false);
    assert_eq!(model.gate_len(), 0);
    model.start_edit_gate();
    assert_eq!(model.notice(), Some(CircuitError::InvalidGate));
    assert_eq!(model.modal(), CircuitModal::None);
}

#[test]
fn verify_names_the_first_failing_row() {
    let mut model = CircuitModel::default();
    model.load_task(1, false);
    model.verify();
    let result = model.verify_result().unwrap();
    assert_eq!((result.passed, result.total, result.won), (2, 4, false));
    assert!(result.wired);
    let failure = result.failure.unwrap();
    assert_eq!(failure.inputs, 0b10);
    assert!(failure.complete && failure.actual && !failure.expected);

    model.load_task(0, false);
    model.verify();
    assert!(!model.verify_result().unwrap().wired);
    model.load_task(10, false);
    model.verify();
    let result = model.verify_result().unwrap();
    assert!(result.wired);
    assert_eq!(result.passed, 6);
}

#[test]
fn solving_marks_the_task_and_next_moves_on() {
    let mut model = CircuitModel::default();
    model.load_task(1, false);
    model.set_selected_kind(GateKind::And).unwrap();
    assert!(!model.controls().won);
    model.verify_or_next();
    assert!(model.controls().won);
    assert_eq!(model.solved_mask(), 0b10);
    model.toggle_input(0);
    assert!(model.controls().won);
    model.verify_or_next();
    assert_eq!(model.task(), 2);
    assert!(!model.controls().won);
    assert_eq!(model.solved_mask(), 0b10);
}

#[test]
fn picking_the_open_task_keeps_the_board() {
    let mut model = CircuitModel::default();
    model.load_task(3, false);
    model.add_gate(GateKind::Xor).unwrap();
    let board = model.snapshot();
    model.open_modal(CircuitModal::Tasks);
    model.choose_modal_option(3);
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.snapshot(), board);
    assert_eq!(model.history_len(), 1);

    model.open_modal(CircuitModal::Tasks);
    model.choose_modal_option(8);
    assert_eq!(model.task(), 8);
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.history_len(), 0);
}

#[test]
fn switching_tasks_keeps_each_board() {
    let mut model = CircuitModel::default();
    model.choose_task(3);
    model.add_gate(GateKind::Xor).unwrap();
    let mine = model.snapshot();
    model.choose_task(8);
    let parity = model.snapshot();
    model.load_task(8, false);
    assert_eq!(parity, model.snapshot());

    model.choose_task(3);
    assert_eq!(model.snapshot(), mine);
    assert_eq!(model.history_len(), 0);
    assert_eq!(model.verify_result(), None);
    model.choose_task(0);
    model.restart_task();
    model.next_task();
    assert_eq!(model.task(), 1);
    model.choose_task(3);
    assert_eq!(model.snapshot(), mine);
}

#[test]
fn restart_and_reference_can_be_undone() {
    let mut model = CircuitModel::default();
    model.load_task(9, false);
    let starter = model.snapshot();
    model.add_gate(GateKind::Not).unwrap();
    let mine = model.snapshot();

    model.open_modal(CircuitModal::Help);
    model.choose_modal_option(4);
    assert_eq!(model.modal(), CircuitModal::None);
    assert!(solve(&mut model));
    model.undo();
    assert_eq!(model.snapshot(), mine);

    model.open_modal(CircuitModal::Help);
    model.choose_modal_option(3);
    assert_eq!(model.snapshot(), starter);
    model.undo();
    assert_eq!(model.snapshot(), mine);

    let history = model.history_len();
    model.undo();
    model.restart_task();
    assert_eq!(model.snapshot(), starter);
    assert_eq!(model.history_len(), history - 1);
}

#[test]
fn one_input_tasks_scan_two_rows() {
    let mut model = CircuitModel::default();
    assert_eq!(model.input_count(), 1);
    assert_eq!(model.toggle_input(1), ChangeSet::NONE);
    model.set_page(CircuitPage::Trace);
    for _ in 0..3 {
        model.step_trace();
    }
    let inputs: [u8; 3] = core::array::from_fn(|index| model.trace(index).unwrap().inputs);
    assert_eq!(inputs, [0, 1, 0]);
    assert_eq!(model.last_sample().unwrap().inputs, 0);
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
        model.connect_pending_to(Some(1), 0),
        Err(CircuitError::Cycle)
    );
    assert_eq!(model.snapshot(), before);
}

#[test]
fn removing_a_gate_detaches_every_reference() {
    let mut model = CircuitModel::default();
    model.load_task(10, true);
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
    model.load_task(10, true);
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
    model.load_task(10, true);
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
    model.load_task(1, false);
    model.set_selected_kind(GateKind::Not).unwrap();
    assert_eq!(model.gate_by_id(1).unwrap().b, SignalSource::None);
    model.inputs = 0;
    assert!(model.evaluation().value);
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
    model.load_task(1, false);
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
fn modal_options_apply_the_selected_transition_and_close() {
    let mut model = CircuitModel::default();

    model.open_modal(CircuitModal::GateTypes { adding: true });
    let gate_len = model.gate_len();
    let changes = model.select_modal_option(0).unwrap();
    assert!(changes.contains(ChangeSet::MODEL));
    assert!(changes.contains(ChangeSet::VISUAL));
    assert_eq!(model.gate_len(), gate_len + 1);
    assert_eq!(model.modal(), CircuitModal::None);

    model.open_modal(CircuitModal::GateTypes { adding: false });
    model.select_modal_option(3).unwrap();
    assert_eq!(
        model.gate_by_id(model.selected()).unwrap().kind,
        GateKind::Not
    );
    assert_eq!(model.modal(), CircuitModal::None);

    model.open_modal(CircuitModal::Tasks);
    model.select_modal_option(3).unwrap();
    assert_eq!(model.task(), 3);
    assert_eq!(model.modal(), CircuitModal::None);

    model.open_modal(CircuitModal::Help);
    model.select_modal_option(5).unwrap();
    assert_eq!(model.modal(), CircuitModal::None);
}

#[test]
fn failed_modal_mutation_keeps_the_modal_open() {
    let mut model = CircuitModel::default();
    while model.gate_len() < MAX_GATES as u8 {
        model.add_gate(GateKind::And).unwrap();
    }
    model.open_modal(CircuitModal::GateTypes { adding: true });

    assert_eq!(model.select_modal_option(0), Err(CircuitError::Full));
    assert_eq!(model.modal(), CircuitModal::GateTypes { adding: true });
}

#[test]
fn fixed_storage_budget_stays_small() {
    assert!(core::mem::size_of::<CircuitModel>() <= 4096);
    assert_eq!(MAX_DRIVEN_INPUTS, MAX_GATES * 2 + 1);
}

#[test]
fn refused_taps_leave_a_notice_until_the_player_acts_again() {
    let mut model = CircuitModel::default();
    model.select_source(SignalSource::gate(1));
    model.connect_from_tap(Some(1), 0);
    assert_eq!(model.notice(), Some(CircuitError::Cycle));
    model.clear_pending();
    assert_eq!(model.notice(), None);

    while model.gate_len() < MAX_GATES as u8 {
        model.add_gate(GateKind::And).unwrap();
    }
    model.start_add_gate();
    assert_eq!(model.notice(), Some(CircuitError::Full));
    assert_eq!(model.modal(), CircuitModal::None);
    model.clear_pending();
    model.open_modal(CircuitModal::GateTypes { adding: true });
    model.choose_modal_option(0);
    assert_eq!(model.notice(), Some(CircuitError::Full));
    assert_eq!(model.modal(), CircuitModal::None);
}

#[test]
fn a_crowded_board_explains_why_no_gate_was_added() {
    let mut model = CircuitModel::default();
    model.load_task(3, false);
    for _ in 0..5 {
        model.add_gate(GateKind::And).unwrap();
    }
    for (id, y) in [(5, 196), (4, 120)] {
        model.begin_drag(id);
        model.move_drag(215, y);
        model.end_drag().unwrap();
    }
    assert!(model.gate_len() < MAX_GATES as u8);
    model.open_modal(CircuitModal::GateTypes { adding: true });
    model.choose_modal_option(0);
    assert_eq!(model.notice(), Some(CircuitError::Overlap));
    assert_eq!(model.modal(), CircuitModal::None);
}
