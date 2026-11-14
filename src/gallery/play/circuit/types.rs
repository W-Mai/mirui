pub(crate) const MAX_GATES: usize = 6;
pub(crate) const MAX_DRIVEN_INPUTS: usize = 13;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TRACE: usize = 32;
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

    pub(super) const fn evaluate(self, a: bool, b: bool) -> bool {
        match self {
            Self::And => a && b,
            Self::Or => a || b,
            Self::Xor => a != b,
            Self::Not => !a,
            Self::Nand => !(a && b),
        }
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// The gate kinds a task lets the player place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GateSet(u8);

impl GateSet {
    pub(crate) const ALL: Self = Self(0b1_1111);

    pub(crate) const fn only(kind: GateKind) -> Self {
        Self(kind.bit())
    }

    pub(crate) const fn contains(self, kind: GateKind) -> bool {
        self.0 & kind.bit() != 0
    }

    pub(crate) const fn is_all(self) -> bool {
        self.0 == Self::ALL.0
    }

    /// The one kind this set allows, when it allows exactly one.
    pub(crate) fn single(self) -> Option<GateKind> {
        let mut kinds = GateKind::ALL
            .into_iter()
            .filter(|kind| self.contains(*kind));
        match (kinds.next(), kinds.next()) {
            (Some(kind), None) => Some(kind),
            _ => None,
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
    pub(super) const EMPTY: Self = Self {
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
    /// Whether anything drives Y.
    pub(crate) wired: bool,
    /// The first row, in table order, that does not pass.
    pub(crate) failure: Option<TruthRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TraceSample {
    pub(crate) inputs: u8,
    pub(crate) output: bool,
}

impl TraceSample {
    pub(super) const EMPTY: Self = Self {
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

/// What the footer buttons can do right now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Controls {
    pub(crate) page: CircuitPage,
    pub(crate) scanning: bool,
    pub(crate) disconnecting: bool,
    /// The board passed, so verify turns into "next task".
    pub(crate) won: bool,
    pub(crate) last_task: bool,
    /// The task's only gate kind, which the add button places directly.
    pub(crate) single_kind: Option<GateKind>,
    pub(crate) can_add: bool,
    pub(crate) can_edit: bool,
    pub(crate) can_undo: bool,
    pub(crate) can_clear: bool,
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
    /// The task does not allow this gate kind.
    NotAllowed,
}
