use crate::core::i18n::Phrase;

pub(crate) const MAX_GATES: usize = 6;
pub(crate) const MAX_DRIVEN_INPUTS: usize = 13;
pub(crate) const MAX_UNDO: usize = 32;
pub(crate) const MAX_TRACE: usize = 32;
const _: () = assert!(MAX_DRIVEN_INPUTS == MAX_GATES * 2 + 1);

/// The breadboard on the 480 x 320 canvas: left, top, width and height.
pub(crate) const BOARD: (i16, i16, i16, i16) = (4, 30, 350, 264);
/// Half the size of a gate body.
pub(crate) const GATE_HALF_WIDTH: i16 = 32;
pub(crate) const GATE_HALF_HEIGHT: i16 = 20;
/// How far the two input pins of a gate sit above and below its middle.
pub(crate) const PIN_SPREAD: i16 = 10;
/// The range of gate centres a drag or a new gate may take.
pub(crate) const GATE_X: (i16, i16) = (104, 268);
pub(crate) const GATE_Y: (i16, i16) = (56, 268);
/// Gates closer than this, centre to centre on both axes, overlap.
pub(crate) const GATE_GAP_X: i16 = 2 * GATE_HALF_WIDTH + 4;
pub(crate) const GATE_GAP_Y: i16 = 2 * GATE_HALF_HEIGHT + 4;
/// The pin of each input pad and the pin of Y.
pub(crate) const INPUT_PIN_X: i16 = 52;
pub(crate) const OUTPUT_PIN: (i16, i16) = (320, 162);

/// The pin height of input `index` of `count`, spread around the middle
/// of the board so a one-input task has its input level with Y.
pub(crate) const fn input_y(index: u8, count: u8) -> i16 {
    OUTPUT_PIN.1 + (2 * index as i16 - (count as i16 - 1)) * 37
}

/// Stars for a pass with `gates` gates on a task whose fewest is `par`.
pub(crate) const fn stars_for(gates: u8, par: u8) -> u8 {
    if gates <= par {
        3
    } else if gates <= par + 1 {
        2
    } else {
        1
    }
}

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

    pub(crate) const fn label(self) -> Phrase {
        match self {
            Self::And => crate::tr!(en: "AND", zh: "与门"),
            Self::Or => crate::tr!(en: "OR", zh: "或门"),
            Self::Xor => crate::tr!(en: "XOR", zh: "异或门"),
            Self::Not => crate::tr!(en: "NOT", zh: "非门"),
            Self::Nand => crate::tr!(en: "NAND", zh: "与非门"),
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

    /// The input pins of the gate: NOT has one, the others two.
    pub(crate) const fn inputs(self) -> u8 {
        if matches!(self, Self::Not) { 1 } else { 2 }
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
    /// Stars this pass earned, 0 without a pass.
    pub(crate) stars: u8,
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

/// What the side buttons can do right now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Controls {
    pub(crate) page: CircuitPage,
    pub(crate) scanning: bool,
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
    GateTypes {
        adding: bool,
    },
    Help,
    /// The card after a pass: its stars and the way on.
    Result,
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
