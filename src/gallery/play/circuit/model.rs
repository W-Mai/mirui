use super::tasks::{Board, TASK_COUNT, TASKS, Task};
use super::types::{
    CircuitError, CircuitGate, CircuitModal, CircuitPage, Controls, Evaluation, GATE_GAP_X,
    GATE_GAP_Y, GATE_X, GATE_Y, GateKind, GateSet, MAX_GATES, MAX_TRACE, MAX_UNDO, SignalSource,
    TraceSample, TruthRow, VerifyResult, stars_for,
};
use crate::anim::{Motion, PlayMode, SNAPPY, Spring, Transition, Tween, ease};
use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;
#[cfg(any(feature = "persistence", test))]
use crate::gallery::play::expeditions::{ExpeditionSaveError, finish_packet, validate_packet};
use crate::types::Fixed;

/// How long a flipped input takes to cross one wire, and the longest a
/// whole wave may take however deep the circuit is.
const WAVE_STEP_MS: u16 = 80;
const WAVE_MAX_MS: u16 = 360;
/// The stars of the result card pop in this far apart.
const POP_STAGGER_MS: u16 = 90;

#[cfg(any(feature = "persistence", test))]
const SAVE_MAGIC: [u8; 4] = *b"LGC1";
#[cfg(any(feature = "persistence", test))]
const SAVE_VERSION: u8 = 1;
#[cfg(any(feature = "persistence", test))]
const SAVE_PAYLOAD: usize = 6 + TASK_COUNT;
#[cfg(any(feature = "persistence", test))]
pub(crate) const SAVE_LEN: usize = SAVE_PAYLOAD + 4;

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

/// The stars and the open task, kept between sessions.
#[cfg(any(feature = "persistence", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CircuitProgress {
    task: u8,
    stars: [u8; TASK_COUNT],
}

/// A wave that has already arrived everywhere.
fn settled_wave() -> Tween {
    Tween::new(Fixed::ZERO, Fixed::ZERO, 0, &ease::LINEAR, PlayMode::Once)
}

fn resting_star() -> Motion {
    Spring::preset(Fixed::ZERO, Fixed::ZERO, SNAPPY).into()
}

#[crate::model(
    change = ChangeSet,
    watch(visual = ChangeSet::VISUAL),
    persistence(key = "logic_circuit/progress")
)]
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
    /// The best stars each task has earned; 0 until it passes.
    stars: [u8; TASK_COUNT],
    /// A flipped input travelling through the gates: its value is how many
    /// wires deep the new signal has reached.
    wave: Tween,
    /// The inputs before the flip, which the wires ahead of the wave still carry.
    wave_inputs: u8,
    /// The result card's stars, each a spring from 0 to full size.
    pop: [Motion; 3],
    pop_ms: u16,
    popped: u8,
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
            stars: [0; TASK_COUNT],
            wave: settled_wave(),
            wave_inputs: 0,
            pop: [resting_star(); 3],
            pop_ms: 0,
            popped: 0,
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

    /// One bit per task that has passed.
    #[observe]
    pub(crate) fn solved_mask(&self) -> u16 {
        (0..TASK_COUNT)
            .filter(|task| self.stars[*task] != 0)
            .fold(0, |mask, task| mask | 1 << task)
    }

    /// The best stars of every task.
    #[observe]
    pub(crate) fn stars(&self) -> [u8; TASK_COUNT] {
        self.stars
    }

    /// The fewest gates the open task needs, for three stars.
    #[observe]
    pub(crate) fn par(&self) -> u8 {
        self.spec().par
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

    /// Y as the board shows it: the old value until the wave of a flipped
    /// input reaches Y.
    #[observe]
    pub(crate) fn shown_output(&self) -> Evaluation {
        let now = self.evaluation();
        match self.wave_at() {
            Some(at) if at < Fixed::from_int(i32::from(self.depth(self.output)) + 1) => {
                Evaluation {
                    value: self.evaluate_source(self.output, self.wave_inputs).value,
                    ..now
                }
            }
            _ => now,
        }
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
        if modal == CircuitModal::Result {
            self.pop = [resting_star(); 3];
            self.pop_ms = 0;
            self.popped = 0;
        }
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

    /// A tap on an input pin: finishes the wire from the picked output, or
    /// with no output picked, takes the pin's wire off.
    pub(crate) fn connect_from_tap(&mut self, gate_id: Option<u8>, pin: u8) -> ChangeSet {
        let result = if self.pending == SignalSource::None {
            self.disconnect(gate_id, pin)
        } else {
            self.connect_pending_to(gate_id, pin)
        };
        self.report(result)
    }

    /// A tap on a gate selects it; a tap on the selected gate edits it.
    pub(crate) fn tap_gate(&mut self, id: u8) -> ChangeSet {
        if self.selected == id && self.gate_by_id(id).is_some() {
            self.start_edit_gate()
        } else {
            self.select_gate(id)
        }
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

    /// Opens the picker for the selected gate. A task with one gate kind
    /// has nothing to change, so the status line says so.
    pub(crate) fn start_edit_gate(&mut self) -> ChangeSet {
        if self.gate_by_id(self.selected).is_none() {
            return self.report(Err(CircuitError::InvalidGate));
        }
        if self.allowed().single().is_some() {
            self.report(Err(CircuitError::NotAllowed))
        } else {
            self.open_modal(CircuitModal::GateTypes { adding: false })
        }
    }

    /// The remove button: takes the selected gate and its wires off.
    pub(crate) fn remove_from_tap(&mut self) -> ChangeSet {
        let result = self.remove_selected();
        self.report(result)
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
            CircuitModal::Result => Ok(match index {
                0 => self.next_task(),
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
        self.wave = settled_wave();
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
            (132, 88),
            (132, 162),
            (132, 236),
            (240, 88),
            (240, 162),
            (240, 236),
        ];
        let free = |&(x, y): &(i16, i16)| !self.overlaps(0, x, y);
        // The six home slots first, then any spot a drag could reach.
        let rows = (GATE_Y.1 - GATE_Y.0) / 4;
        let columns = (GATE_X.1 - GATE_X.0) / 4;
        let mut roam = (0..=rows).flat_map(|row| {
            (0..=columns).map(move |column| (GATE_X.0 + column * 4, GATE_Y.0 + row * 4))
        });
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

    pub(crate) fn connect_pending_to(
        &mut self,
        gate_id: Option<u8>,
        pin: u8,
    ) -> Result<ChangeSet, CircuitError> {
        let source = self.pending;
        if !self.valid_source(source) {
            return Err(CircuitError::InvalidSource);
        }
        let target = if let Some(id) = gate_id {
            let Some(index) = self.gate_index(id) else {
                return Err(CircuitError::InvalidGate);
            };
            if pin >= self.gates[index].kind.inputs() {
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

    /// Flips one input and sends the change through the wires. The verdict
    /// stays: it covers every input row.
    pub(crate) fn toggle_input(&mut self, index: u8) -> ChangeSet {
        if index >= self.input_count() || self.modal != CircuitModal::None {
            return ChangeSet::NONE;
        }
        let before = self.inputs;
        self.inputs ^= 1 << index;
        let levels = self.levels();
        self.wave_inputs = before;
        self.wave = Tween::new(
            Fixed::ZERO,
            Fixed::from_int(i32::from(levels)),
            (WAVE_STEP_MS * u16::from(levels)).min(WAVE_MAX_MS),
            &ease::LINEAR,
            PlayMode::Once,
        );
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
        let x = snap((x - drag.grab_x).clamp(GATE_X.0, GATE_X.1));
        let y = snap((y - drag.grab_y).clamp(GATE_Y.0, GATE_Y.1));
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

    /// Checks every input row. A pass keeps the task's best stars and
    /// shows the result card.
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
        let stars = if won {
            stars_for(self.gate_len, self.spec().par)
        } else {
            0
        };
        let task = usize::from(self.task);
        self.stars[task] = self.stars[task].max(stars);
        self.verify = Some(VerifyResult {
            passed,
            total,
            won,
            wired: self.output != SignalSource::None,
            failure,
            stars,
        });
        self.scanning = false;
        self.scan_clock.reset();
        let card = if won {
            self.open_modal(CircuitModal::Result)
        } else {
            ChangeSet::NONE
        };
        ChangeSet::MODEL | ChangeSet::VISUAL | card
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
        self.wave = settled_wave();
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

    /// The frame clock: runs the scanner, carries the wave of a flipped
    /// input and pops the stars of the result card.
    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let active =
            self.scanning && self.modal == CircuitModal::None && self.page == CircuitPage::Trace;
        let steps = self.scan_clock.steps(elapsed_ms, active);
        let mut changes = ChangeSet::NONE;
        for _ in 0..steps {
            changes = changes | self.step_trace();
        }
        if elapsed_ms > 0 && !self.wave.is_finished() {
            self.wave.tick(elapsed_ms);
            changes = changes | ChangeSet::VISUAL;
        }
        if elapsed_ms > 0 && self.modal == CircuitModal::Result && self.popping() {
            self.pop_ms = self.pop_ms.saturating_add(elapsed_ms);
            let earned = self.verify.map_or(0, |result| result.stars);
            while self.popped < earned && self.pop_ms >= u16::from(self.popped) * POP_STAGGER_MS {
                Transition::from(SNAPPY)
                    .retarget(&mut self.pop[usize::from(self.popped)], Fixed::ONE);
                self.popped += 1;
            }
            for star in &mut self.pop {
                star.tick(elapsed_ms);
            }
            changes = changes | ChangeSet::VISUAL;
        }
        changes
    }
}

impl CircuitModel {
    fn spec(&self) -> &'static Task {
        &TASKS[usize::from(self.task)]
    }

    /// How far the wave of the last flipped input has travelled, in wires,
    /// while it is still on its way.
    pub(crate) fn wave_at(&self) -> Option<Fixed> {
        (!self.wave.is_finished()).then(|| self.wave.value())
    }

    /// The value `source` shows: the old one until the wave reaches it.
    pub(crate) fn shown_value(&self, source: SignalSource) -> bool {
        match self.wave_at() {
            Some(at) if at < Fixed::from_int(i32::from(self.depth(source))) => {
                self.previous_value(source)
            }
            _ => self.source_value(source),
        }
    }

    /// The share of a wire from `source` that the wave has crossed, from
    /// its source end, while the wave is on that wire.
    pub(crate) fn wire_front(&self, source: SignalSource) -> Option<Fixed> {
        let at = self.wave_at()? - Fixed::from_int(i32::from(self.depth(source)));
        (at > Fixed::ZERO && at < Fixed::ONE).then_some(at)
    }

    /// The value `source` carried before the last flip.
    pub(crate) fn previous_value(&self, source: SignalSource) -> bool {
        let evaluation = self.evaluate_source(source, self.wave_inputs);
        evaluation.value && evaluation.complete
    }

    /// The size of result star `index`, from 0 to 1 with a little bounce.
    pub(crate) fn star_scale(&self, index: usize) -> Fixed {
        self.pop.get(index).map_or(Fixed::ZERO, Motion::value)
    }

    fn popping(&self) -> bool {
        let earned = self.verify.map_or(0, |result| result.stars);
        self.popped < earned || self.pop.iter().any(|star| !star.is_done())
    }

    /// The gates between the inputs and `source`: an input is 0, a gate
    /// one more than its deepest input.
    fn depth(&self, source: SignalSource) -> u8 {
        let mut visiting = [false; MAX_GATES];
        self.depth_recursive(source, &mut visiting)
    }

    fn depth_recursive(&self, source: SignalSource, visiting: &mut [bool; MAX_GATES]) -> u8 {
        let SignalSource::Gate(id) = source else {
            return 0;
        };
        let Some(index) = self.gate_index(id) else {
            return 0;
        };
        if visiting[index] {
            return 0;
        }
        visiting[index] = true;
        let gate = self.gates[index];
        let a = self.depth_recursive(gate.a, visiting);
        let b = if gate.kind.inputs() == 2 {
            self.depth_recursive(gate.b, visiting)
        } else {
            0
        };
        visiting[index] = false;
        1 + a.max(b)
    }

    /// The wires a flipped input must cross to reach every pin.
    fn levels(&self) -> u8 {
        let deepest = self.gates[..usize::from(self.gate_len)]
            .iter()
            .map(|gate| self.depth(SignalSource::gate(gate.id)))
            .max()
            .unwrap_or(0);
        deepest.max(self.depth(self.output)) + 1
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
        self.wave = settled_wave();
        self.scan_cursor = 0;
        self.scanning = false;
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

    /// Records the board before an edit. The edit also ends a wave in
    /// flight and the last verdict.
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
        self.wave = settled_wave();
    }

    fn disconnect(&mut self, gate_id: Option<u8>, pin: u8) -> Result<ChangeSet, CircuitError> {
        let old = if let Some(id) = gate_id {
            let index = self.gate_index(id).ok_or(CircuitError::InvalidGate)?;
            if pin >= self.gates[index].kind.inputs() {
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
        self.gates[..usize::from(self.gate_len)].iter().any(|gate| {
            gate.id != ignored_id
                && (gate.x - x).abs() < GATE_GAP_X
                && (gate.y - y).abs() < GATE_GAP_Y
        })
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

    /// Writes the stars and the open task into `output`.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn encode_progress(&self, output: &mut [u8]) -> Result<usize, ExpeditionSaveError> {
        if output.len() < SAVE_LEN {
            return Err(ExpeditionSaveError::BufferTooSmall);
        }
        output[..4].copy_from_slice(&SAVE_MAGIC);
        output[4] = SAVE_VERSION;
        output[5] = self.task;
        output[6..SAVE_PAYLOAD].copy_from_slice(&self.stars);
        finish_packet(output, SAVE_PAYLOAD);
        Ok(SAVE_LEN)
    }

    /// Reads a record written by `encode_progress`, checking every field.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn decode_progress(input: &[u8]) -> Result<CircuitProgress, ExpeditionSaveError> {
        validate_packet(input, SAVE_PAYLOAD)?;
        if input[..4] != SAVE_MAGIC {
            return Err(ExpeditionSaveError::InvalidMagic);
        }
        if input[4] != SAVE_VERSION {
            return Err(ExpeditionSaveError::UnsupportedVersion);
        }
        let task = input[5];
        if usize::from(task) >= TASK_COUNT {
            return Err(ExpeditionSaveError::InvalidLevel);
        }
        let mut stars = [0; TASK_COUNT];
        stars.copy_from_slice(&input[6..SAVE_PAYLOAD]);
        if stars.iter().any(|stars| *stars > 3) {
            return Err(ExpeditionSaveError::InvalidRecord);
        }
        Ok(CircuitProgress { task, stars })
    }

    /// Takes back the saved stars and opens the saved task on its starter.
    #[cfg(any(feature = "persistence", test))]
    pub(crate) fn apply_progress(&mut self, progress: CircuitProgress) {
        self.stars = progress.stars;
        self.visited = 0;
        self.load_task_state(progress.task, false);
    }
}

const fn snap(value: i16) -> i16 {
    ((value + 2) / 4) * 4
}
