use super::tasks::{Board, TASK_COUNT, TASKS, Task};
use super::types::{
    CircuitError, CircuitGate, CircuitModal, CircuitPage, Controls, Evaluation, GateKind, GateSet,
    MAX_GATES, MAX_TRACE, MAX_UNDO, SignalSource, TraceSample, TruthRow, VerifyResult,
};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CircuitSnapshot {
    gates: [CircuitGate; MAX_GATES],
    gate_len: u8,
    output: SignalSource,
    next_id: u8,
}

impl CircuitSnapshot {
    const EMPTY: Self = Self {
        gates: [CircuitGate::EMPTY; MAX_GATES],
        gate_len: 0,
        output: SignalSource::None,
        next_id: 1,
    };

    fn from_board(board: Board) -> Self {
        let mut snapshot = Self::EMPTY;
        for (slot, spec) in snapshot.gates.iter_mut().zip(board.gates) {
            *slot = CircuitGate {
                id: snapshot.next_id,
                kind: spec.kind,
                x: spec.x,
                y: spec.y,
                a: spec.a,
                b: spec.b,
            };
            snapshot.next_id += 1;
            snapshot.gate_len += 1;
        }
        snapshot.output = board.output;
        snapshot
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GateDrag {
    id: u8,
    origin_x: i16,
    origin_y: i16,
    grab_x: i16,
    grab_y: i16,
    x: i16,
    y: i16,
}

#[crate::model(change = ChangeSet, watch(visual = ChangeSet::VISUAL))]
pub(crate) struct CircuitModel {
    gates: [CircuitGate; MAX_GATES],
    history: [CircuitSnapshot; MAX_UNDO],
    trace: [TraceSample; MAX_TRACE],
    output: SignalSource,
    pending: SignalSource,
    drag: Option<GateDrag>,
    verify: Option<VerifyResult>,
    page: CircuitPage,
    modal: CircuitModal,
    scan_clock: BoundedClock,
    /// The board the player left in each task, restored on return.
    boards: [CircuitSnapshot; TASK_COUNT],
    /// One bit per task whose entry in `boards` holds the player's board.
    visited: u16,
    /// One bit per task that has passed verification.
    solved: u16,
    gate_len: u8,
    history_len: u8,
    trace_start: u8,
    trace_len: u8,
    task: u8,
    pub(super) inputs: u8,
    next_id: u8,
    selected: u8,
    scan_cursor: u8,
    scanning: bool,
    disconnecting: bool,
    /// Why the player's last wire or new gate was refused, until they act again.
    notice: Option<CircuitError>,
}

impl Default for CircuitModel {
    fn default() -> Self {
        let mut model = Self {
            gates: [CircuitGate::EMPTY; MAX_GATES],
            history: [CircuitSnapshot::EMPTY; MAX_UNDO],
            trace: [TraceSample::EMPTY; MAX_TRACE],
            output: SignalSource::None,
            pending: SignalSource::None,
            drag: None,
            verify: None,
            page: CircuitPage::Wire,
            modal: CircuitModal::None,
            scan_clock: BoundedClock::new(2, 120, 1),
            boards: [CircuitSnapshot::EMPTY; TASK_COUNT],
            visited: 0,
            solved: 0,
            gate_len: 0,
            history_len: 0,
            trace_start: 0,
            trace_len: 0,
            task: 0,
            inputs: 0,
            next_id: 1,
            selected: 0,
            scan_cursor: 0,
            scanning: false,
            disconnecting: false,
            notice: None,
        };
        model.load_task_state(0, false);
        model
    }
}

#[crate::model]
impl CircuitModel {
    #[observe]
    pub(crate) fn task(&self) -> u8 {
        self.task
    }

    #[observe]
    pub(crate) fn input_count(&self) -> u8 {
        self.spec().inputs
    }

    /// The gate kinds this task lets the player place.
    #[observe]
    pub(crate) fn allowed(&self) -> GateSet {
        self.spec().gates
    }

    #[observe]
    pub(crate) fn solved_mask(&self) -> u16 {
        self.solved
    }

    #[observe]
    pub(crate) fn gate_len(&self) -> u8 {
        self.gate_len
    }

    pub(crate) fn gate(&self, index: usize) -> Option<CircuitGate> {
        (index < usize::from(self.gate_len)).then_some(self.gates[index])
    }

    pub(crate) fn gate_by_id(&self, id: u8) -> Option<CircuitGate> {
        self.gate_index(id).map(|index| self.gates[index])
    }

    #[observe]
    pub(crate) fn selected(&self) -> u8 {
        self.selected
    }

    #[observe]
    pub(crate) fn pending(&self) -> SignalSource {
        self.pending
    }

    pub(crate) fn output_source(&self) -> SignalSource {
        self.output
    }

    #[observe]
    pub(crate) fn page(&self) -> CircuitPage {
        self.page
    }

    #[observe]
    pub(crate) fn modal(&self) -> CircuitModal {
        self.modal
    }

    #[observe]
    pub(crate) fn scanning(&self) -> bool {
        self.scanning
    }

    #[observe]
    pub(crate) fn disconnecting(&self) -> bool {
        self.disconnecting
    }

    #[observe]
    pub(crate) fn history_len(&self) -> u8 {
        self.history_len
    }

    #[observe]
    pub(crate) fn trace_len(&self) -> u8 {
        self.trace_len
    }

    pub(crate) fn trace(&self, index: usize) -> Option<TraceSample> {
        if index >= usize::from(self.trace_len) {
            return None;
        }
        let slot = (usize::from(self.trace_start) + index) % MAX_TRACE;
        Some(self.trace[slot])
    }

    #[observe]
    pub(crate) fn last_sample(&self) -> Option<TraceSample> {
        self.trace_len
            .checked_sub(1)
            .and_then(|index| self.trace(usize::from(index)))
    }

    #[observe]
    pub(crate) fn verify_result(&self) -> Option<VerifyResult> {
        self.verify
    }

    #[observe]
    pub(crate) fn controls(&self) -> Controls {
        Controls {
            page: self.page,
            scanning: self.scanning,
            disconnecting: self.disconnecting,
            won: self.verify.is_some_and(|result| result.won),
            last_task: usize::from(self.task) + 1 == TASK_COUNT,
            single_kind: self.allowed().single(),
            can_add: usize::from(self.gate_len) < MAX_GATES,
            can_edit: self.gate_by_id(self.selected).is_some(),
            can_undo: self.history_len != 0,
            can_clear: self.trace_len != 0,
        }
    }

    pub(crate) fn input(&self, index: u8) -> bool {
        self.inputs & (1 << index) != 0
    }

    #[observe]
    pub(crate) fn evaluation(&self) -> Evaluation {
        self.evaluate_source(self.output, self.inputs)
    }

    #[observe]
    pub(crate) fn input_values(&self) -> [bool; 3] {
        [self.input(0), self.input(1), self.input(2)]
    }

    #[observe]
    pub(crate) fn gates(&self) -> [Option<CircuitGate>; MAX_GATES] {
        core::array::from_fn(|index| self.gate(index))
    }

    #[observe]
    pub(crate) fn gate_positions(&self) -> [Option<(i16, i16)>; MAX_GATES] {
        core::array::from_fn(|index| {
            self.gate(index)
                .and_then(|gate| self.visual_gate_position(gate.id))
        })
    }

    #[observe]
    pub(crate) fn truth_rows(&self) -> [Option<TruthRow>; 8] {
        core::array::from_fn(|index| self.truth_row(index as u8))
    }

    /// Whether `source` carries a 1 for the current inputs. A signal that
    /// depends on an open pin is not lit.
    pub(crate) fn source_value(&self, source: SignalSource) -> bool {
        let evaluation = self.evaluate_source(source, self.inputs);
        evaluation.value && evaluation.complete
    }

    pub(crate) fn visual_gate_position(&self, id: u8) -> Option<(i16, i16)> {
        if let Some(drag) = self.drag.filter(|drag| drag.id == id) {
            return Some((drag.x, drag.y));
        }
        self.gate_by_id(id).map(|gate| (gate.x, gate.y))
    }

    pub(crate) fn set_page(&mut self, page: CircuitPage) -> ChangeSet {
        if self.page == page {
            return ChangeSet::NONE;
        }
        self.page = page;
        self.pending = SignalSource::None;
        self.disconnecting = false;
        self.drag = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn open_modal(&mut self, modal: CircuitModal) -> ChangeSet {
        if self.modal == modal {
            return ChangeSet::NONE;
        }
        self.modal = modal;
        self.pending = SignalSource::None;
        self.drag = None;
        self.scan_clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        self.open_modal(CircuitModal::None)
    }

    #[observe]
    pub(crate) fn notice(&self) -> Option<CircuitError> {
        self.notice
    }

    /// `select_modal_option` for a tap. A gate that cannot be added or used
    /// closes the picker, so the status line can say why.
    pub(crate) fn choose_modal_option(&mut self, index: usize) -> ChangeSet {
        let result = self.select_modal_option(index);
        let closed = if matches!(
            result,
            Err(CircuitError::Full | CircuitError::Overlap | CircuitError::NotAllowed)
        ) {
            self.close_modal()
        } else {
            ChangeSet::NONE
        };
        self.report(result) | closed
    }

    /// `connect_pending_to` for a tap on an input.
    pub(crate) fn connect_from_tap(&mut self, gate_id: Option<u8>, pin: u8) -> ChangeSet {
        let result = self.connect_pending_to(gate_id, pin);
        self.report(result)
    }

    /// The add button: a task with one gate kind places it straight away,
    /// any other opens the gate picker.
    pub(crate) fn start_add_gate(&mut self) -> ChangeSet {
        if usize::from(self.gate_len) >= MAX_GATES {
            return self.report(Err(CircuitError::Full));
        }
        match self.allowed().single() {
            Some(kind) => {
                let result = self.add_gate(kind);
                self.report(result)
            }
            None => self.open_modal(CircuitModal::GateTypes { adding: true }),
        }
    }

    /// The edit button: with one gate kind there is nothing to change, so
    /// it removes the selected gate; otherwise it opens the picker.
    pub(crate) fn start_edit_gate(&mut self) -> ChangeSet {
        if self.gate_by_id(self.selected).is_none() {
            return self.report(Err(CircuitError::InvalidGate));
        }
        if self.allowed().single().is_some() {
            let result = self.remove_selected();
            self.report(result)
        } else {
            self.open_modal(CircuitModal::GateTypes { adding: false })
        }
    }

    pub(crate) fn select_modal_option(&mut self, index: usize) -> Result<ChangeSet, CircuitError> {
        match self.modal {
            CircuitModal::Tasks => Ok(self.choose_task(index)),
            CircuitModal::GateTypes { adding } if index < GateKind::ALL.len() => {
                let mutation = if adding {
                    self.add_gate(GateKind::ALL[index])
                } else {
                    self.set_selected_kind(GateKind::ALL[index])
                }?;
                Ok(mutation | self.close_modal())
            }
            CircuitModal::GateTypes { adding: false } if index == 5 => {
                let mutation = self.remove_selected()?;
                Ok(mutation | self.close_modal())
            }
            CircuitModal::Help => Ok(match index {
                3 => self.restart_task(),
                4 => self.show_reference(),
                _ => self.close_modal(),
            }),
            _ => Ok(ChangeSet::NONE),
        }
    }

    /// Opens `task` with the board the player left there, or its starter.
    /// Picking the open task only closes the picker.
    pub(crate) fn choose_task(&mut self, task: usize) -> ChangeSet {
        if task == usize::from(self.task) {
            return self.close_modal();
        }
        self.switch_task(task)
    }

    /// After a pass: the next task, or the picker after the last one.
    pub(crate) fn next_task(&mut self) -> ChangeSet {
        let next = usize::from(self.task) + 1;
        if next < TASK_COUNT {
            self.switch_task(next)
        } else {
            self.open_modal(CircuitModal::Tasks)
        }
    }

    /// The verify button turns into "next task" once the board passes.
    pub(crate) fn verify_or_next(&mut self) -> ChangeSet {
        if self.verify.is_some_and(|result| result.won) {
            self.next_task()
        } else {
            self.verify()
        }
    }

    /// Puts the starter board back. Undo returns to the player's board.
    pub(crate) fn restart_task(&mut self) -> ChangeSet {
        self.replace_board(self.spec().starter)
    }

    /// Loads the reference board. Undo returns to the player's board.
    pub(crate) fn show_reference(&mut self) -> ChangeSet {
        self.replace_board(self.spec().reference)
    }

    /// Opens `task` fresh, from its starter or its reference board.
    #[cfg(test)]
    pub(crate) fn load_task(&mut self, task: u8, reference: bool) -> ChangeSet {
        if usize::from(task) >= TASK_COUNT {
            return ChangeSet::NONE;
        }
        self.load_task_state(task, reference);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        self.restore(self.history[usize::from(self.history_len)]);
        self.verify = None;
        self.pending = SignalSource::None;
        self.drag = None;
        self.selected = self
            .gate_by_id(self.selected)
            .map_or_else(|| self.gate(0).map_or(0, |gate| gate.id), |gate| gate.id);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn add_gate(&mut self, kind: GateKind) -> Result<ChangeSet, CircuitError> {
        if !self.allowed().contains(kind) {
            return Err(CircuitError::NotAllowed);
        }
        if usize::from(self.gate_len) >= MAX_GATES {
            return Err(CircuitError::Full);
        }
        let slots = [
            (105, 94),
            (105, 151),
            (105, 208),
            (215, 94),
            (215, 151),
            (215, 208),
        ];
        let free = |&(x, y): &(i16, i16)| !self.overlaps(0, x, y);
        // The six home slots first, then any spot a drag could reach.
        let mut roam = (0..=36_i16)
            .flat_map(|row| (0..=35_i16).map(move |column| (100 + column * 4, 84 + row * 4)));
        let Some((x, y)) = slots.into_iter().find(free).or_else(|| roam.find(free)) else {
            return Err(CircuitError::Overlap);
        };
        self.remember();
        self.seed_gate(kind, x, y, SignalSource::None, SignalSource::None);
        self.selected = self.gates[usize::from(self.gate_len) - 1].id;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn set_selected_kind(&mut self, kind: GateKind) -> Result<ChangeSet, CircuitError> {
        let Some(index) = self.gate_index(self.selected) else {
            return Err(CircuitError::InvalidGate);
        };
        if !self.allowed().contains(kind) {
            return Err(CircuitError::NotAllowed);
        }
        if self.gates[index].kind == kind {
            return Ok(ChangeSet::NONE);
        }
        self.remember();
        self.gates[index].kind = kind;
        if kind == GateKind::Not {
            self.gates[index].b = SignalSource::None;
        }
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn remove_selected(&mut self) -> Result<ChangeSet, CircuitError> {
        let Some(index) = self.gate_index(self.selected) else {
            return Err(CircuitError::InvalidGate);
        };
        let removed = self.gates[index].id;
        self.remember();
        for slot in index..usize::from(self.gate_len) - 1 {
            self.gates[slot] = self.gates[slot + 1];
        }
        self.gate_len -= 1;
        self.gates[usize::from(self.gate_len)] = CircuitGate::EMPTY;
        for gate in &mut self.gates[..usize::from(self.gate_len)] {
            if gate.a == SignalSource::gate(removed) {
                gate.a = SignalSource::None;
            }
            if gate.b == SignalSource::gate(removed) {
                gate.b = SignalSource::None;
            }
        }
        if self.output == SignalSource::gate(removed) {
            self.output = SignalSource::None;
        }
        self.selected = self.gate(0).map_or(0, |gate| gate.id);
        self.pending = SignalSource::None;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn select_gate(&mut self, id: u8) -> ChangeSet {
        if self.gate_by_id(id).is_none() || self.selected == id {
            return ChangeSet::NONE;
        }
        self.selected = id;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn select_source(&mut self, source: SignalSource) -> ChangeSet {
        if !self.valid_source(source) || self.pending == source {
            return ChangeSet::NONE;
        }
        self.pending = source;
        self.disconnecting = false;
        self.notice = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn clear_pending(&mut self) -> ChangeSet {
        if self.pending == SignalSource::None && self.notice.is_none() {
            return ChangeSet::NONE;
        }
        self.pending = SignalSource::None;
        self.notice = None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_disconnecting(&mut self) -> ChangeSet {
        self.disconnecting = !self.disconnecting;
        self.pending = SignalSource::None;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn connect_pending_to(
        &mut self,
        gate_id: Option<u8>,
        pin: u8,
    ) -> Result<ChangeSet, CircuitError> {
        if self.disconnecting {
            return self.disconnect(gate_id, pin);
        }
        let source = self.pending;
        if !self.valid_source(source) {
            return Err(CircuitError::InvalidSource);
        }
        let target = if let Some(id) = gate_id {
            let Some(index) = self.gate_index(id) else {
                return Err(CircuitError::InvalidGate);
            };
            if pin > 1 || (pin == 1 && self.gates[index].kind == GateKind::Not) {
                return Err(CircuitError::InvalidInput);
            }
            if pin == 0 {
                self.gates[index].a
            } else {
                self.gates[index].b
            }
        } else {
            self.output
        };
        if target == source {
            return Err(CircuitError::Duplicate);
        }
        if let Some(id) = gate_id {
            let index = self.gate_index(id).ok_or(CircuitError::InvalidGate)?;
            if pin == 0 {
                self.gates[index].a = source;
            } else {
                self.gates[index].b = source;
            }
            let cycle = self.has_cycle();
            if pin == 0 {
                self.gates[index].a = target;
            } else {
                self.gates[index].b = target;
            }
            if cycle {
                return Err(CircuitError::Cycle);
            }
        }
        self.remember();
        if let Some(id) = gate_id {
            let index = self.gate_index(id).ok_or(CircuitError::InvalidGate)?;
            if pin == 0 {
                self.gates[index].a = source;
            } else {
                self.gates[index].b = source;
            }
        } else {
            self.output = source;
        }
        self.pending = SignalSource::None;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    /// Flips one input. The verdict stays: it covers every input row.
    pub(crate) fn toggle_input(&mut self, index: u8) -> ChangeSet {
        if index >= self.input_count() || self.modal != CircuitModal::None {
            return ChangeSet::NONE;
        }
        self.inputs ^= 1 << index;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    #[cfg(test)]
    pub(crate) fn begin_drag(&mut self, id: u8) -> ChangeSet {
        let Some(gate) = self.gate_by_id(id) else {
            return ChangeSet::NONE;
        };
        self.begin_drag_at(id, gate.x, gate.y)
    }

    pub(crate) fn begin_drag_at(&mut self, id: u8, pointer_x: i16, pointer_y: i16) -> ChangeSet {
        let Some(gate) = self.gate_by_id(id) else {
            return ChangeSet::NONE;
        };
        self.selected = id;
        self.drag = Some(GateDrag {
            id,
            origin_x: gate.x,
            origin_y: gate.y,
            grab_x: pointer_x - gate.x,
            grab_y: pointer_y - gate.y,
            x: gate.x,
            y: gate.y,
        });
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn move_drag(&mut self, x: i16, y: i16) -> ChangeSet {
        let Some(drag) = &mut self.drag else {
            return ChangeSet::NONE;
        };
        let x = snap((x - drag.grab_x).clamp(100, 243));
        let y = snap((y - drag.grab_y).clamp(84, 230));
        if drag.x == x && drag.y == y {
            return ChangeSet::NONE;
        }
        drag.x = x;
        drag.y = y;
        ChangeSet::VISUAL
    }

    pub(crate) fn end_drag(&mut self) -> Result<ChangeSet, CircuitError> {
        let Some(drag) = self.drag else {
            return Ok(ChangeSet::NONE);
        };
        if drag.x == drag.origin_x && drag.y == drag.origin_y {
            self.drag = None;
            return Ok(ChangeSet::VISUAL);
        }
        if self.overlaps(drag.id, drag.x, drag.y) {
            return Err(CircuitError::Overlap);
        }
        let index = self.gate_index(drag.id).ok_or(CircuitError::InvalidGate)?;
        self.drag = None;
        self.remember();
        self.gates[index].x = drag.x;
        self.gates[index].y = drag.y;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn cancel_drag(&mut self) -> ChangeSet {
        if self.drag.take().is_some() {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    /// Checks every input row. A pass marks the task solved.
    pub(crate) fn verify(&mut self) -> ChangeSet {
        let total = 1_u8 << self.input_count();
        let mut passed = 0;
        let mut failure = None;
        for row in 0..total {
            let Some(truth) = self.truth_row(row) else {
                continue;
            };
            if truth.passes() {
                passed += 1;
            } else if failure.is_none() {
                failure = Some(truth);
            }
        }
        let won = passed == total;
        if won {
            self.solved |= 1 << self.task;
        }
        self.verify = Some(VerifyResult {
            passed,
            total,
            won,
            wired: self.output != SignalSource::None,
            failure,
        });
        self.scanning = false;
        self.scan_clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// Row `row` of the truth table, which lists A as its leftmost column.
    pub(crate) fn truth_row(&self, row: u8) -> Option<TruthRow> {
        let count = self.input_count();
        if row >= 1 << count {
            return None;
        }
        let inputs = (0..count)
            .filter(|index| row & (1 << (count - 1 - index)) != 0)
            .fold(0, |inputs, index| inputs | (1 << index));
        let result = self.evaluate_source(self.output, inputs);
        Some(TruthRow {
            inputs,
            expected: self.expected(inputs),
            actual: result.value,
            complete: result.complete,
        })
    }

    pub(crate) fn step_trace(&mut self) -> ChangeSet {
        let total = 1_u8 << self.input_count();
        let row = self.scan_cursor % total;
        self.scan_cursor = (self.scan_cursor + 1) % total;
        let Some(truth) = self.truth_row(row) else {
            return ChangeSet::NONE;
        };
        self.inputs = truth.inputs;
        self.push_trace(TraceSample {
            inputs: truth.inputs,
            output: truth.actual,
        });
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn toggle_scanning(&mut self) -> ChangeSet {
        if self.modal != CircuitModal::None {
            return ChangeSet::NONE;
        }
        self.scanning = !self.scanning;
        self.scan_clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn clear_trace(&mut self) -> ChangeSet {
        if self.trace_len == 0 && self.scan_cursor == 0 {
            return ChangeSet::NONE;
        }
        self.trace = [TraceSample::EMPTY; MAX_TRACE];
        self.trace_start = 0;
        self.trace_len = 0;
        self.scan_cursor = 0;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let active =
            self.scanning && self.modal == CircuitModal::None && self.page == CircuitPage::Trace;
        let steps = self.scan_clock.steps(elapsed_ms, active);
        let mut changes = ChangeSet::NONE;
        for _ in 0..steps {
            changes = changes | self.step_trace();
        }
        changes
    }
}

impl CircuitModel {
    fn spec(&self) -> &'static Task {
        &TASKS[usize::from(self.task)]
    }

    pub(super) fn snapshot(&self) -> CircuitSnapshot {
        CircuitSnapshot {
            gates: self.gates,
            gate_len: self.gate_len,
            output: self.output,
            next_id: self.next_id,
        }
    }

    fn restore(&mut self, snapshot: CircuitSnapshot) {
        self.gates = snapshot.gates;
        self.gate_len = snapshot.gate_len;
        self.output = snapshot.output;
        self.next_id = snapshot.next_id;
    }

    /// Keeps the refusal of a player action for the status line, or clears
    /// the last one when the action went through.
    fn report(&mut self, result: Result<ChangeSet, CircuitError>) -> ChangeSet {
        match result {
            Ok(change) if self.notice.take().is_some() => change | ChangeSet::VISUAL,
            Ok(change) => change,
            Err(error) => {
                self.notice = Some(error);
                ChangeSet::MODEL | ChangeSet::VISUAL
            }
        }
    }

    fn load_task_state(&mut self, task: u8, reference: bool) {
        self.history = [CircuitSnapshot::EMPTY; MAX_UNDO];
        self.trace = [TraceSample::EMPTY; MAX_TRACE];
        self.history_len = 0;
        self.trace_start = 0;
        self.trace_len = 0;
        self.pending = SignalSource::None;
        self.drag = None;
        self.verify = None;
        self.page = CircuitPage::Wire;
        self.modal = CircuitModal::None;
        self.task = task.min(TASK_COUNT as u8 - 1);
        self.inputs = 0;
        self.scan_cursor = 0;
        self.scanning = false;
        self.disconnecting = false;
        self.notice = None;
        self.scan_clock.reset();
        let spec = self.spec();
        let board = if reference {
            spec.reference
        } else {
            spec.starter
        };
        self.restore(CircuitSnapshot::from_board(board));
        self.selected = self.gate(0).map_or(0, |gate| gate.id);
    }

    /// Keeps the current board for a later visit and opens `task`.
    fn switch_task(&mut self, task: usize) -> ChangeSet {
        if task >= TASK_COUNT {
            return ChangeSet::NONE;
        }
        let current = usize::from(self.task);
        self.boards[current] = self.snapshot();
        self.visited |= 1 << current;
        self.load_task_state(task as u8, false);
        if self.visited & (1 << task) != 0 {
            self.restore(self.boards[task]);
            self.selected = self.gate(0).map_or(0, |gate| gate.id);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    /// Swaps in a prepared board as one undoable edit and closes the modal.
    fn replace_board(&mut self, board: Board) -> ChangeSet {
        let next = CircuitSnapshot::from_board(board);
        if next != self.snapshot() {
            self.remember();
            self.restore(next);
            self.pending = SignalSource::None;
            self.drag = None;
            self.disconnecting = false;
            self.notice = None;
            self.selected = self.gate(0).map_or(0, |gate| gate.id);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | self.close_modal()
    }

    fn seed_gate(&mut self, kind: GateKind, x: i16, y: i16, a: SignalSource, b: SignalSource) {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.gates[usize::from(self.gate_len)] = CircuitGate {
            id,
            kind,
            x,
            y,
            a,
            b,
        };
        self.gate_len += 1;
    }

    fn remember(&mut self) {
        let snapshot = self.snapshot();
        if usize::from(self.history_len) == MAX_UNDO {
            self.history.copy_within(1..MAX_UNDO, 0);
            self.history[MAX_UNDO - 1] = snapshot;
        } else {
            self.history[usize::from(self.history_len)] = snapshot;
            self.history_len += 1;
        }
        self.verify = None;
    }

    fn disconnect(&mut self, gate_id: Option<u8>, pin: u8) -> Result<ChangeSet, CircuitError> {
        let old = if let Some(id) = gate_id {
            let index = self.gate_index(id).ok_or(CircuitError::InvalidGate)?;
            if pin > 1 || (pin == 1 && self.gates[index].kind == GateKind::Not) {
                return Err(CircuitError::InvalidInput);
            }
            if pin == 0 {
                self.gates[index].a
            } else {
                self.gates[index].b
            }
        } else {
            self.output
        };
        if old == SignalSource::None {
            return Err(CircuitError::InvalidSource);
        }
        self.remember();
        if let Some(id) = gate_id {
            let index = self.gate_index(id).ok_or(CircuitError::InvalidGate)?;
            if pin == 0 {
                self.gates[index].a = SignalSource::None;
            } else {
                self.gates[index].b = SignalSource::None;
            }
        } else {
            self.output = SignalSource::None;
        }
        self.disconnecting = false;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    fn push_trace(&mut self, sample: TraceSample) {
        if usize::from(self.trace_len) < MAX_TRACE {
            let slot = (usize::from(self.trace_start) + usize::from(self.trace_len)) % MAX_TRACE;
            self.trace[slot] = sample;
            self.trace_len += 1;
        } else {
            self.trace[usize::from(self.trace_start)] = sample;
            self.trace_start = (self.trace_start + 1) % MAX_TRACE as u8;
        }
    }

    fn expected(&self, inputs: u8) -> bool {
        (self.spec().rule)(inputs & 1 != 0, inputs & 2 != 0, inputs & 4 != 0)
    }

    fn evaluate_source(&self, source: SignalSource, inputs: u8) -> Evaluation {
        let mut memo = [false; MAX_GATES];
        let mut ready = [false; MAX_GATES];
        let mut visiting = [false; MAX_GATES];
        let mut complete = true;
        let value = self.evaluate_recursive(
            source,
            inputs,
            &mut memo,
            &mut ready,
            &mut visiting,
            &mut complete,
        );
        Evaluation { value, complete }
    }

    fn evaluate_recursive(
        &self,
        source: SignalSource,
        inputs: u8,
        memo: &mut [bool; MAX_GATES],
        ready: &mut [bool; MAX_GATES],
        visiting: &mut [bool; MAX_GATES],
        complete: &mut bool,
    ) -> bool {
        match source {
            SignalSource::None => {
                *complete = false;
                false
            }
            SignalSource::Input(index) => {
                if index >= self.input_count() {
                    *complete = false;
                    false
                } else {
                    inputs & (1 << index) != 0
                }
            }
            SignalSource::Gate(id) => {
                let Some(index) = self.gate_index(id) else {
                    *complete = false;
                    return false;
                };
                if ready[index] {
                    return memo[index];
                }
                if visiting[index] {
                    *complete = false;
                    return false;
                }
                visiting[index] = true;
                let gate = self.gates[index];
                let a = self.evaluate_recursive(gate.a, inputs, memo, ready, visiting, complete);
                let b = if gate.kind == GateKind::Not {
                    false
                } else {
                    self.evaluate_recursive(gate.b, inputs, memo, ready, visiting, complete)
                };
                let value = gate.kind.evaluate(a, b);
                visiting[index] = false;
                ready[index] = true;
                memo[index] = value;
                value
            }
        }
    }

    fn valid_source(&self, source: SignalSource) -> bool {
        match source {
            SignalSource::None => false,
            SignalSource::Input(index) => index < self.input_count(),
            SignalSource::Gate(id) => self.gate_by_id(id).is_some(),
        }
    }

    fn gate_index(&self, id: u8) -> Option<usize> {
        self.gates[..usize::from(self.gate_len)]
            .iter()
            .position(|gate| gate.id == id)
    }

    fn overlaps(&self, ignored_id: u8, x: i16, y: i16) -> bool {
        self.gates[..usize::from(self.gate_len)]
            .iter()
            .any(|gate| gate.id != ignored_id && (gate.x - x).abs() < 62 && (gate.y - y).abs() < 38)
    }

    fn has_cycle(&self) -> bool {
        let mut states = [0_u8; MAX_GATES];
        for index in 0..usize::from(self.gate_len) {
            if self.visit_cycle(index, &mut states) {
                return true;
            }
        }
        false
    }

    fn visit_cycle(&self, index: usize, states: &mut [u8; MAX_GATES]) -> bool {
        if states[index] == 1 {
            return true;
        }
        if states[index] == 2 {
            return false;
        }
        states[index] = 1;
        let gate = self.gates[index];
        for source in [gate.a, gate.b] {
            if gate.kind == GateKind::Not && source == gate.b {
                continue;
            }
            if let SignalSource::Gate(id) = source
                && let Some(next) = self.gate_index(id)
                && self.visit_cycle(next, states)
            {
                return true;
            }
        }
        states[index] = 2;
        false
    }
}

const fn snap(value: i16) -> i16 {
    ((value + 2) / 4) * 4
}
