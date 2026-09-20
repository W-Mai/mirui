use crate::gallery::play::change::ChangeSet;
use crate::gallery::play::clock::BoundedClock;

pub(crate) const MAX_GATES: usize = 6;
pub(crate) const MAX_DRIVEN_INPUTS: usize = 13;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TRACE: usize = 32;
pub(crate) const TASK_COUNT: usize = 3;
const _: () = assert!(MAX_DRIVEN_INPUTS == MAX_GATES * 2 + 1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GateKind {
    And,
    Or,
    Xor,
    Not,
    Nand,
}

impl GateKind {
    pub(crate) const ALL: [Self; 5] = [Self::And, Self::Or, Self::Xor, Self::Not, Self::Nand];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::And => "AND",
            Self::Or => "OR",
            Self::Xor => "XOR",
            Self::Not => "NOT",
            Self::Nand => "NAND",
        }
    }

    const fn evaluate(self, a: bool, b: bool) -> bool {
        match self {
            Self::And => a && b,
            Self::Or => a || b,
            Self::Xor => a != b,
            Self::Not => !a,
            Self::Nand => !(a && b),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SignalSource {
    None,
    Input(u8),
    Gate(u8),
}

impl SignalSource {
    pub(crate) const fn input(index: u8) -> Self {
        Self::Input(index)
    }

    pub(crate) const fn gate(id: u8) -> Self {
        Self::Gate(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CircuitGate {
    pub(crate) id: u8,
    pub(crate) kind: GateKind,
    pub(crate) x: i16,
    pub(crate) y: i16,
    pub(crate) a: SignalSource,
    pub(crate) b: SignalSource,
}

impl CircuitGate {
    const EMPTY: Self = Self {
        id: 0,
        kind: GateKind::And,
        x: 0,
        y: 0,
        a: SignalSource::None,
        b: SignalSource::None,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Evaluation {
    pub(crate) value: bool,
    pub(crate) complete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TruthRow {
    pub(crate) inputs: u8,
    pub(crate) expected: bool,
    pub(crate) actual: bool,
    pub(crate) complete: bool,
}

impl TruthRow {
    pub(crate) const fn passes(self) -> bool {
        self.complete && self.actual == self.expected
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerifyResult {
    pub(crate) passed: u8,
    pub(crate) total: u8,
    pub(crate) won: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TraceSample {
    pub(crate) inputs: u8,
    pub(crate) output: bool,
}

impl TraceSample {
    const EMPTY: Self = Self {
        inputs: 0,
        output: false,
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CircuitPage {
    Wire,
    Truth,
    Trace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CircuitModal {
    None,
    Tasks,
    GateTypes { adding: bool },
    Help,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CircuitError {
    InvalidGate,
    InvalidSource,
    InvalidInput,
    Duplicate,
    Cycle,
    Full,
    Overlap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CircuitSnapshot {
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

#[derive(Clone, Copy, Debug)]
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
    gate_len: u8,
    history_len: u8,
    trace_start: u8,
    trace_len: u8,
    task: u8,
    inputs: u8,
    next_id: u8,
    selected: u8,
    scan_cursor: u8,
    scanning: bool,
    disconnecting: bool,
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
        };
        model.load_task_state(0, false);
        model
    }
}

impl CircuitModel {
    pub(crate) const fn task(&self) -> u8 {
        self.task
    }

    pub(crate) const fn task_name(&self) -> &'static str {
        match self.task {
            0 => "不同才亮",
            1 => "多数表决",
            _ => "二选一",
        }
    }

    pub(crate) const fn target_code(&self) -> &'static str {
        match self.task {
            0 => "Y = A XOR B",
            1 => "Y = AB + AC + BC",
            _ => "Y = A ? C : B",
        }
    }

    pub(crate) const fn task_description(&self) -> &'static str {
        match self.task {
            0 => "A 与 B 不同时，Y 才为 1。",
            1 => "三个输入中至少两个为 1 时，Y 为 1。",
            _ => "A 为 0 选 B，A 为 1 选 C。",
        }
    }

    pub(crate) const fn input_count(&self) -> u8 {
        if self.task == 0 { 2 } else { 3 }
    }

    pub(crate) const fn gate_len(&self) -> u8 {
        self.gate_len
    }

    pub(crate) fn gate(&self, index: usize) -> Option<CircuitGate> {
        (index < usize::from(self.gate_len)).then_some(self.gates[index])
    }

    pub(crate) fn gate_by_id(&self, id: u8) -> Option<CircuitGate> {
        self.gate_index(id).map(|index| self.gates[index])
    }

    pub(crate) const fn selected(&self) -> u8 {
        self.selected
    }

    pub(crate) const fn pending(&self) -> SignalSource {
        self.pending
    }

    pub(crate) const fn output_source(&self) -> SignalSource {
        self.output
    }

    pub(crate) const fn page(&self) -> CircuitPage {
        self.page
    }

    pub(crate) const fn modal(&self) -> CircuitModal {
        self.modal
    }

    pub(crate) const fn scanning(&self) -> bool {
        self.scanning
    }

    pub(crate) const fn disconnecting(&self) -> bool {
        self.disconnecting
    }

    pub(crate) const fn history_len(&self) -> u8 {
        self.history_len
    }

    pub(crate) const fn trace_len(&self) -> u8 {
        self.trace_len
    }

    pub(crate) fn trace(&self, index: usize) -> Option<TraceSample> {
        if index >= usize::from(self.trace_len) {
            return None;
        }
        let slot = (usize::from(self.trace_start) + index) % MAX_TRACE;
        Some(self.trace[slot])
    }

    pub(crate) const fn verify_result(&self) -> Option<VerifyResult> {
        self.verify
    }

    pub(crate) const fn input(&self, index: u8) -> bool {
        self.inputs & (1 << index) != 0
    }

    pub(crate) fn evaluation(&self) -> Evaluation {
        self.evaluate_source(self.output, self.inputs)
    }

    pub(crate) fn source_value(&self, source: SignalSource) -> bool {
        self.evaluate_source(source, self.inputs).value
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
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn close_modal(&mut self) -> ChangeSet {
        self.open_modal(CircuitModal::None)
    }

    pub(crate) fn load_task(&mut self, task: u8, reference: bool) -> ChangeSet {
        if usize::from(task) >= TASK_COUNT {
            return ChangeSet::NONE;
        }
        self.load_task_state(task, reference);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    fn load_task_state(&mut self, task: u8, reference: bool) {
        self.gates = [CircuitGate::EMPTY; MAX_GATES];
        self.history = [CircuitSnapshot::EMPTY; MAX_UNDO];
        self.trace = [TraceSample::EMPTY; MAX_TRACE];
        self.gate_len = 0;
        self.history_len = 0;
        self.trace_start = 0;
        self.trace_len = 0;
        self.output = SignalSource::None;
        self.pending = SignalSource::None;
        self.drag = None;
        self.verify = None;
        self.page = CircuitPage::Wire;
        self.modal = CircuitModal::None;
        self.task = task.min(2);
        self.inputs = 0;
        self.next_id = 1;
        self.selected = 0;
        self.scan_cursor = 0;
        self.scanning = false;
        self.disconnecting = false;
        self.scan_clock.reset();
        match self.task {
            0 => {
                self.seed_gate(
                    if reference {
                        GateKind::Xor
                    } else {
                        GateKind::Or
                    },
                    145,
                    150,
                    SignalSource::input(0),
                    SignalSource::input(1),
                );
                self.output = SignalSource::gate(1);
            }
            1 if reference => {
                self.seed_gate(
                    GateKind::And,
                    105,
                    95,
                    SignalSource::input(0),
                    SignalSource::input(1),
                );
                self.seed_gate(
                    GateKind::And,
                    105,
                    151,
                    SignalSource::input(0),
                    SignalSource::input(2),
                );
                self.seed_gate(
                    GateKind::And,
                    105,
                    207,
                    SignalSource::input(1),
                    SignalSource::input(2),
                );
                self.seed_gate(
                    GateKind::Or,
                    214,
                    119,
                    SignalSource::gate(1),
                    SignalSource::gate(2),
                );
                self.seed_gate(
                    GateKind::Or,
                    214,
                    193,
                    SignalSource::gate(4),
                    SignalSource::gate(3),
                );
                self.output = SignalSource::gate(5);
            }
            1 => {
                self.seed_gate(
                    GateKind::And,
                    110,
                    96,
                    SignalSource::input(0),
                    SignalSource::input(1),
                );
                self.seed_gate(
                    GateKind::Or,
                    215,
                    150,
                    SignalSource::gate(1),
                    SignalSource::None,
                );
                self.output = SignalSource::gate(2);
            }
            _ if reference => {
                self.seed_gate(
                    GateKind::Not,
                    105,
                    94,
                    SignalSource::input(0),
                    SignalSource::None,
                );
                self.seed_gate(
                    GateKind::And,
                    105,
                    206,
                    SignalSource::input(0),
                    SignalSource::input(2),
                );
                self.seed_gate(
                    GateKind::And,
                    203,
                    94,
                    SignalSource::gate(1),
                    SignalSource::input(1),
                );
                self.seed_gate(
                    GateKind::Or,
                    213,
                    187,
                    SignalSource::gate(2),
                    SignalSource::gate(3),
                );
                self.output = SignalSource::gate(4);
            }
            _ => {
                self.seed_gate(
                    GateKind::And,
                    115,
                    149,
                    SignalSource::input(0),
                    SignalSource::input(2),
                );
                self.output = SignalSource::gate(1);
            }
        }
        self.selected = self.gates[0].id;
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

    fn snapshot(&self) -> CircuitSnapshot {
        CircuitSnapshot {
            gates: self.gates,
            gate_len: self.gate_len,
            output: self.output,
            next_id: self.next_id,
        }
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

    pub(crate) fn undo(&mut self) -> ChangeSet {
        if self.history_len == 0 {
            return ChangeSet::NONE;
        }
        self.history_len -= 1;
        let snapshot = self.history[usize::from(self.history_len)];
        self.gates = snapshot.gates;
        self.gate_len = snapshot.gate_len;
        self.output = snapshot.output;
        self.next_id = snapshot.next_id;
        self.verify = None;
        self.pending = SignalSource::None;
        self.drag = None;
        self.selected = self
            .gate_by_id(self.selected)
            .map_or_else(|| self.gate(0).map_or(0, |gate| gate.id), |gate| gate.id);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn add_gate(&mut self, kind: GateKind) -> Result<ChangeSet, CircuitError> {
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
        let Some((x, y)) = slots.into_iter().find(|(x, y)| !self.overlaps(0, *x, *y)) else {
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
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn clear_pending(&mut self) -> ChangeSet {
        if self.pending == SignalSource::None {
            return ChangeSet::NONE;
        }
        self.pending = SignalSource::None;
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

    pub(crate) fn toggle_input(&mut self, index: u8) -> ChangeSet {
        if index >= self.input_count() || self.modal != CircuitModal::None {
            return ChangeSet::NONE;
        }
        self.inputs ^= 1 << index;
        self.verify = None;
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

    pub(crate) fn end_drag(&mut self, cancel: bool) -> Result<ChangeSet, CircuitError> {
        let Some(drag) = self.drag.take() else {
            return Ok(ChangeSet::NONE);
        };
        if cancel || (drag.x == drag.origin_x && drag.y == drag.origin_y) {
            return Ok(ChangeSet::VISUAL);
        }
        if self.overlaps(drag.id, drag.x, drag.y) {
            return Err(CircuitError::Overlap);
        }
        let index = self.gate_index(drag.id).ok_or(CircuitError::InvalidGate)?;
        self.remember();
        self.gates[index].x = drag.x;
        self.gates[index].y = drag.y;
        Ok(ChangeSet::MODEL | ChangeSet::VISUAL)
    }

    pub(crate) fn verify(&mut self) -> ChangeSet {
        let total = 1_u8 << self.input_count();
        let mut passed = 0;
        for row in 0..total {
            passed += u8::from(self.truth_row(row).is_some_and(TruthRow::passes));
        }
        self.verify = Some(VerifyResult {
            passed,
            total,
            won: passed == total,
        });
        self.scanning = false;
        self.scan_clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn truth_row(&self, row: u8) -> Option<TruthRow> {
        let count = self.input_count();
        if row >= 1 << count {
            return None;
        }
        let inputs = if count == 2 {
            ((row & 0b10) >> 1) | ((row & 0b01) << 1)
        } else {
            ((row & 0b100) >> 2) | (row & 0b010) | ((row & 0b001) << 2)
        };
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
        let a = inputs & 1 != 0;
        let b = inputs & 2 != 0;
        let c = inputs & 4 != 0;
        match self.task {
            0 => a != b,
            1 => u8::from(a) + u8::from(b) + u8::from(c) >= 2,
            _ => {
                if a {
                    c
                } else {
                    b
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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
        model.begin_drag(1);
        model.move_drag(180, 180);
        model.end_drag(true).unwrap();
        assert_eq!(model.history_len(), history);
        model.begin_drag(1);
        let gate = model.gate_by_id(2).unwrap();
        model.move_drag(gate.x, gate.y);
        assert_eq!(model.end_drag(false), Err(CircuitError::Overlap));
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
}
