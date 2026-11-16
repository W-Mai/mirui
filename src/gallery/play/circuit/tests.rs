use super::model::{CircuitModel, SAVE_LEN};
use super::tasks::{Board, GateSpec, SET_SIZE, TASK_COUNT, TASKS};
use super::types::{
    CircuitError, CircuitModal, CircuitPage, GATE_GAP_X, GATE_GAP_Y, GATE_X, GATE_Y, GateKind,
    MAX_DRIVEN_INPUTS, MAX_GATES, MAX_TRACE, MAX_UNDO, SignalSource,
};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::expeditions::ExpeditionSaveError;
use crate::types::Fixed;

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
            (GATE_X.0..=GATE_X.1).contains(&gate.x),
            "{} gate {}",
            spec.name,
            index + 1
        );
        assert!(
            (GATE_Y.0..=GATE_Y.1).contains(&gate.y),
            "{} gate {}",
            spec.name,
            index + 1
        );
        for other in &board.gates[index + 1..] {
            assert!(
                (gate.x - other.x).abs() >= GATE_GAP_X || (gate.y - other.y).abs() >= GATE_GAP_Y,
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
    build_board(model, TASKS[usize::from(model.task())].reference);
}

/// Clears the board and builds `reference` with the player's tools.
fn build_board(model: &mut CircuitModel, reference: Board) {
    while model.gate_len() > 0 {
        let first = model.gate(0).unwrap().id;
        model.select_gate(first);
        model.remove_selected().unwrap();
    }
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
        assert_eq!(model.modal(), CircuitModal::Result);
        assert!(model.stars()[usize::from(task)] >= 2);
        model.verify_or_next();
    }
    assert_eq!(model.solved_mask().count_ones(), TASK_COUNT as u32);
    assert_eq!(model.modal(), CircuitModal::Tasks);
    assert_eq!(model.task(), TASK_COUNT as u8 - 1);
}

const fn spec(kind: GateKind, a: SignalSource, b: SignalSource) -> GateSpec {
    GateSpec {
        kind,
        x: 0,
        y: 0,
        a,
        b,
    }
}

const A: SignalSource = SignalSource::Input(0);
const B: SignalSource = SignalSource::Input(1);
const C: SignalSource = SignalSource::Input(2);

/// Boards with the fewest gates, for the tasks whose reference uses more.
const PAR_BOARDS: [(usize, Board); 2] = [
    (
        9,
        Board {
            gates: &[
                spec(GateKind::Xor, B, C),
                spec(GateKind::And, A, SignalSource::Gate(1)),
                spec(GateKind::Xor, B, SignalSource::Gate(2)),
            ],
            output: SignalSource::Gate(3),
        },
    ),
    (
        10,
        Board {
            gates: &[
                spec(GateKind::And, A, B),
                spec(GateKind::Or, A, B),
                spec(GateKind::And, C, SignalSource::Gate(2)),
                spec(GateKind::Or, SignalSource::Gate(1), SignalSource::Gate(3)),
            ],
            output: SignalSource::Gate(4),
        },
    ),
];

#[test]
fn every_task_can_earn_three_stars() {
    for (task, spec) in TASKS.iter().enumerate() {
        let board = PAR_BOARDS
            .iter()
            .find(|(index, _)| *index == task)
            .map_or(spec.reference, |(_, board)| *board);
        assert_eq!(board.gates.len(), usize::from(spec.par), "{}", spec.name);
        let mut model = CircuitModel::default();
        model.load_task(task as u8, false);
        build_board(&mut model, board);
        assert!(solve(&mut model), "{}", spec.name);
        assert_eq!(model.verify_result().unwrap().stars, 3, "{}", spec.name);
    }
}

#[test]
fn extra_gates_cost_stars_and_the_best_result_is_kept() {
    let mut model = CircuitModel::default();
    model.load_task(10, true);
    model.verify();
    assert_eq!(model.verify_result().unwrap().stars, 2);
    assert_eq!(model.stars()[10], 2);
    model.add_gate(GateKind::And).unwrap();
    model.verify();
    assert_eq!(model.verify_result().unwrap().stars, 1);
    assert_eq!(model.stars()[10], 2);
    model.load_task(10, false);
    model.verify();
    assert_eq!(model.stars()[10], 2);
}

#[test]
fn a_flipped_input_reaches_y_one_wire_at_a_time() {
    let mut model = CircuitModel::default();
    model.load_task(3, true);
    model.inputs = 0b11;
    let xor = SignalSource::gate(1);
    let not = SignalSource::gate(2);
    assert!(model.shown_output().value);
    model.toggle_input(0);
    assert!(!model.evaluation().value);
    assert!(!model.shown_value(A));
    assert!(!model.shown_value(xor));
    assert!(model.shown_output().value);
    assert!(model.wire_front(A).is_none());
    model.advance_ms(40);
    assert!(model.wire_front(A).is_some());
    assert!(!model.shown_value(xor));
    model.advance_ms(80);
    assert!(model.shown_value(xor));
    assert!(model.wire_front(xor).is_some());
    assert!(model.shown_output().value);
    model.advance_ms(200);
    assert!(model.wave_at().is_none());
    assert!(!model.shown_value(not));
    assert!(!model.shown_output().value);
    assert_eq!(model.advance_ms(16), ChangeSet::NONE);
}

#[test]
fn an_edit_or_a_new_flip_settles_the_wave_in_flight() {
    let mut model = CircuitModel::default();
    model.load_task(1, true);
    model.toggle_input(0);
    assert!(model.wave_at().is_some());
    model.toggle_input(1);
    assert!(model.shown_value(B));
    model.select_source(A);
    model.connect_pending_to(Some(1), 1).unwrap();
    assert!(model.wave_at().is_none());
}

#[test]
fn tapping_a_wired_input_takes_its_wire_off() {
    let mut model = CircuitModel::default();
    model.load_task(1, false);
    model.connect_from_tap(Some(1), 1);
    assert_eq!(model.gate_by_id(1).unwrap().b, SignalSource::None);
    assert_eq!(model.history_len(), 1);
    model.connect_from_tap(Some(1), 1);
    assert_eq!(model.notice(), Some(CircuitError::InvalidSource));
    model.connect_from_tap(None, 0);
    assert_eq!(model.output_source(), SignalSource::None);
    model.undo();
    model.undo();
    assert_eq!(model.gate_by_id(1).unwrap().b, B);
}

#[test]
fn tapping_the_selected_gate_opens_its_picker() {
    let mut model = CircuitModel::default();
    model.load_task(1, false);
    assert_eq!(model.selected(), 1);
    model.tap_gate(1);
    assert_eq!(model.modal(), CircuitModal::GateTypes { adding: false });
    model.close_modal();
    model.load_task(5, true);
    model.tap_gate(2);
    assert_eq!(model.selected(), 2);
    model.tap_gate(2);
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.notice(), Some(CircuitError::NotAllowed));
    assert_eq!(model.gate_len(), 2);
}

#[test]
fn the_result_card_pops_its_stars_and_leads_on() {
    let mut model = CircuitModel::default();
    model.load_task(0, true);
    model.verify();
    assert_eq!(model.modal(), CircuitModal::Result);
    assert_eq!(model.star_scale(0), Fixed::ZERO);
    let mut frames = 0;
    while model.advance_ms(16).contains(ChangeSet::VISUAL) {
        frames += 1;
        assert!(frames < 120, "the stars keep moving");
    }
    for star in 0..3 {
        assert_eq!(model.star_scale(star), Fixed::ONE);
    }
    model.select_modal_option(0).unwrap();
    assert_eq!(model.task(), 1);
    assert_eq!(model.modal(), CircuitModal::None);
}

#[test]
fn progress_round_trips_and_reopens_the_saved_task() {
    let mut source = CircuitModel::default();
    source.load_task(10, true);
    source.verify();
    source.choose_task(6);
    let mut bytes = [0; SAVE_LEN];
    assert_eq!(source.encode_progress(&mut bytes), Ok(SAVE_LEN));
    assert_eq!(&bytes[..4], b"LGC1");
    let progress = CircuitModel::decode_progress(&bytes).unwrap();
    let mut restored = CircuitModel::default();
    restored.apply_progress(progress);
    assert_eq!(restored.task(), 6);
    assert_eq!(restored.stars(), source.stars());
    assert_eq!(restored.modal(), CircuitModal::None);
    assert_eq!(restored.history_len(), 0);

    let mut broken = bytes;
    broken[6] ^= 1;
    assert_eq!(
        CircuitModel::decode_progress(&broken),
        Err(ExpeditionSaveError::InvalidChecksum)
    );
    assert_eq!(
        CircuitModel::decode_progress(&bytes[..SAVE_LEN - 1]),
        Err(ExpeditionSaveError::InvalidLength)
    );
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
    assert_eq!(model.modal(), CircuitModal::None);
    assert_eq!(model.notice(), Some(CircuitError::NotAllowed));
    model.remove_from_tap();
    assert_eq!(model.gate_len(), 1);
    assert_eq!(model.notice(), None);

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
fn a_gate_finds_room_when_the_home_slots_are_taken() {
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
    model.open_modal(CircuitModal::GateTypes { adding: true });
    model.choose_modal_option(0);
    assert_eq!(model.notice(), None);
    assert_eq!(model.gate_len(), MAX_GATES as u8);
    let added = model.gate(MAX_GATES - 1).unwrap();
    for index in 0..MAX_GATES - 1 {
        let other = model.gate(index).unwrap();
        assert!((other.x - added.x).abs() >= GATE_GAP_X || (other.y - added.y).abs() >= GATE_GAP_Y);
    }
}
