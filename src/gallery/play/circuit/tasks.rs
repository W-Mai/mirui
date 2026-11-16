use super::types::{GateKind, GateSet, SignalSource};
use crate::core::i18n::Phrase;
use crate::tr;

pub(crate) const TASK_COUNT: usize = 12;
/// Tasks per row of the picker. Each row is one set: the board, then NAND
/// only, then three inputs.
pub(crate) const SET_SIZE: usize = 4;

/// One gate of a prepared board. Gates name each other by their 1-based
/// place in the list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateSpec {
    pub(crate) kind: GateKind,
    pub(crate) x: i16,
    pub(crate) y: i16,
    pub(crate) a: SignalSource,
    pub(crate) b: SignalSource,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Board {
    pub(crate) gates: &'static [GateSpec],
    pub(crate) output: SignalSource,
}

pub(crate) struct Task {
    pub(crate) name: Phrase,
    /// The rule as a formula, over the truth table.
    pub(crate) code: Phrase,
    pub(crate) goal: Phrase,
    /// Shown on the status line while the player has nothing else to read.
    pub(crate) hint: Phrase,
    pub(crate) inputs: u8,
    pub(crate) gates: GateSet,
    /// The fewest gates that pass: three stars at this count.
    pub(crate) par: u8,
    /// Y for inputs A, B and C. Inputs the task does not have read 0.
    pub(crate) rule: fn(bool, bool, bool) -> bool,
    pub(crate) starter: Board,
    pub(crate) reference: Board,
}

const A: SignalSource = SignalSource::Input(0);
const B: SignalSource = SignalSource::Input(1);
const C: SignalSource = SignalSource::Input(2);
const OPEN: SignalSource = SignalSource::None;

const fn g(id: u8) -> SignalSource {
    SignalSource::Gate(id)
}

const fn gate(kind: GateKind, x: i16, y: i16, a: SignalSource, b: SignalSource) -> GateSpec {
    GateSpec { kind, x, y, a, b }
}

const fn not(x: i16, y: i16, a: SignalSource) -> GateSpec {
    gate(GateKind::Not, x, y, a, OPEN)
}

const fn and(x: i16, y: i16, a: SignalSource, b: SignalSource) -> GateSpec {
    gate(GateKind::And, x, y, a, b)
}

const fn or(x: i16, y: i16, a: SignalSource, b: SignalSource) -> GateSpec {
    gate(GateKind::Or, x, y, a, b)
}

const fn xor(x: i16, y: i16, a: SignalSource, b: SignalSource) -> GateSpec {
    gate(GateKind::Xor, x, y, a, b)
}

const fn nand(x: i16, y: i16, a: SignalSource, b: SignalSource) -> GateSpec {
    gate(GateKind::Nand, x, y, a, b)
}

const EMPTY: Board = Board {
    gates: &[],
    output: OPEN,
};

const NOT_A: Phrase = tr!(en: "Y = NOT A", zh: "Y = 非 A");
const NOT_A_GOAL: Phrase = tr!(en: "Y is always the opposite of A.", zh: "Y 总是和 A 相反。");
const A_AND_B: Phrase = tr!(en: "Y = A AND B", zh: "Y = A 与 B");
const A_AND_B_GOAL: Phrase = tr!(
    en: "Y is 1 only when A and B are both 1.",
    zh: "A 和 B 都为 1 时，Y 才为 1。"
);

pub(crate) static TASKS: [Task; TASK_COUNT] = [
    Task {
        name: tr!(en: "Invert", zh: "取反"),
        code: NOT_A,
        goal: NOT_A_GOAL,
        hint: tr!(
            en: "Tap the dot right of A, then the left dot of NOT. Then wire NOT to Y.",
            zh: "点 A 右侧的圆点，再点非门左侧的圆点。非门也要接到 Y"
        ),
        inputs: 1,
        gates: GateSet::ALL,
        par: 1,
        rule: |a, _, _| !a,
        starter: Board {
            gates: &[not(188, 162, OPEN)],
            output: OPEN,
        },
        reference: Board {
            gates: &[not(188, 162, A)],
            output: g(1),
        },
    },
    Task {
        name: tr!(en: "Both On", zh: "都亮才亮"),
        code: A_AND_B,
        goal: A_AND_B_GOAL,
        hint: tr!(
            en: "Wrong gate kind: tap the gate and pick AND.",
            zh: "门的类型不对：点这个门，换成与门"
        ),
        inputs: 2,
        gates: GateSet::ALL,
        par: 1,
        rule: |a, b, _| a && b,
        starter: Board {
            gates: &[or(188, 162, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[and(188, 162, A, B)],
            output: g(1),
        },
    },
    Task {
        name: tr!(en: "A Not B", zh: "A 开 B 关"),
        code: tr!(en: "Y = A AND NOT B", zh: "Y = A 与 非 B"),
        goal: tr!(
            en: "Y is 1 only when A is 1 and B is 0.",
            zh: "A 为 1、B 为 0 时，Y 才为 1。"
        ),
        hint: tr!(
            en: "Add a NOT between B and the AND.",
            zh: "加一个非门，接在 B 和与门之间"
        ),
        inputs: 2,
        gates: GateSet::ALL,
        par: 2,
        rule: |a, b, _| a && !b,
        starter: Board {
            gates: &[and(240, 162, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[and(240, 162, A, g(2)), not(132, 199, B)],
            output: g(1),
        },
    },
    Task {
        name: tr!(en: "Same", zh: "相同才亮"),
        code: tr!(en: "Y = A XNOR B", zh: "Y = A 同或 B"),
        goal: tr!(en: "Y is 1 when A equals B.", zh: "A 与 B 相同时，Y 为 1。"),
        hint: tr!(en: "XNOR is XOR turned over.", zh: "同或正好是异或取反"),
        inputs: 2,
        gates: GateSet::ALL,
        par: 2,
        rule: |a, b, _| a == b,
        starter: EMPTY,
        reference: Board {
            gates: &[xor(132, 162, A, B), not(240, 162, g(1))],
            output: g(2),
        },
    },
    Task {
        name: tr!(en: "NAND → NOT", zh: "与非 → 非"),
        code: NOT_A,
        goal: NOT_A_GOAL,
        hint: tr!(
            en: "One output can feed several inputs.",
            zh: "一个输出端可以接多个输入端"
        ),
        inputs: 1,
        gates: GateSet::only(GateKind::Nand),
        par: 1,
        rule: |a, _, _| !a,
        starter: Board {
            gates: &[nand(188, 162, A, OPEN)],
            output: g(1),
        },
        reference: Board {
            gates: &[nand(188, 162, A, A)],
            output: g(1),
        },
    },
    Task {
        name: tr!(en: "NAND → AND", zh: "与非 → 与"),
        code: A_AND_B,
        goal: A_AND_B_GOAL,
        hint: tr!(
            en: "The inverter from the last task still works.",
            zh: "上一题的取反，这里还能用"
        ),
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        par: 2,
        rule: |a, b, _| a && b,
        starter: Board {
            gates: &[nand(132, 162, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[nand(132, 162, A, B), nand(240, 162, g(1), g(1))],
            output: g(2),
        },
    },
    Task {
        name: tr!(en: "NAND → OR", zh: "与非 → 或"),
        code: tr!(en: "Y = A OR B", zh: "Y = A 或 B"),
        goal: tr!(
            en: "Y is 1 when A or B is 1.",
            zh: "A 或 B 有一个为 1，Y 就为 1。"
        ),
        hint: tr!(en: "Invert A and B first.", zh: "先把 A 和 B 各自取反"),
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        par: 3,
        rule: |a, b, _| a || b,
        starter: Board {
            gates: &[nand(240, 162, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                nand(240, 162, g(2), g(3)),
                nand(132, 125, A, A),
                nand(132, 199, B, B),
            ],
            output: g(1),
        },
    },
    Task {
        name: tr!(en: "NAND → XOR", zh: "与非 → 异或"),
        code: tr!(en: "Y = A XOR B", zh: "Y = A 异或 B"),
        goal: tr!(en: "Y is 1 when A and B differ.", zh: "A 与 B 不同时，Y 为 1。"),
        hint: tr!(
            en: "Four NANDs do it. The first one is in place.",
            zh: "4 个与非门可以完成，第一个已经放好"
        ),
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        par: 4,
        rule: |a, b, _| a != b,
        starter: Board {
            gates: &[nand(108, 162, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                nand(108, 162, A, B),
                nand(188, 112, A, g(1)),
                nand(188, 212, g(1), B),
                nand(268, 162, g(2), g(3)),
            ],
            output: g(4),
        },
    },
    Task {
        name: tr!(en: "Odd Ones", zh: "奇数个 1"),
        code: tr!(en: "Y = A XOR B XOR C", zh: "Y = A 异或 B 异或 C"),
        goal: tr!(
            en: "Y is 1 when an odd number of A B C are 1.",
            zh: "A B C 中有奇数个 1 时，Y 为 1。"
        ),
        hint: tr!(
            en: "An XOR output can feed another XOR.",
            zh: "异或门的输出还能再接异或门"
        ),
        inputs: 3,
        gates: GateSet::ALL,
        par: 2,
        rule: |a, b, c| a ^ b ^ c,
        starter: Board {
            gates: &[xor(132, 125, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[xor(132, 125, A, B), xor(240, 162, g(1), C)],
            output: g(2),
        },
    },
    Task {
        name: tr!(en: "Pick One", zh: "二选一"),
        code: tr!(en: "Y = A ? C : B", zh: "Y = A ? C : B"),
        goal: tr!(
            en: "A = 0 picks B, A = 1 picks C.",
            zh: "A 为 0 选 B，A 为 1 选 C。"
        ),
        hint: tr!(
            en: "A = 1 picking C is wired. Now wire A = 0 picking B.",
            zh: "A 为 1 选 C 已经接好，再接 A 为 0 选 B"
        ),
        inputs: 3,
        gates: GateSet::ALL,
        par: 3,
        rule: |a, b, c| if a { c } else { b },
        starter: Board {
            gates: &[and(108, 236, A, C)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                and(108, 236, A, C),
                not(108, 88, A),
                and(188, 125, g(2), B),
                or(268, 162, g(3), g(1)),
            ],
            output: g(4),
        },
    },
    Task {
        name: tr!(en: "Majority", zh: "多数表决"),
        code: tr!(en: "Y = AB + AC + BC", zh: "Y = AB + AC + BC"),
        goal: tr!(
            en: "Y is 1 when at least two are 1.",
            zh: "至少两个为 1 时，Y 为 1。"
        ),
        hint: tr!(
            en: "Five gates do it. Four also work.",
            zh: "5 个门可以完成，4 个也行"
        ),
        inputs: 3,
        gates: GateSet::ALL,
        par: 4,
        rule: |a, b, c| u8::from(a) + u8::from(b) + u8::from(c) >= 2,
        starter: Board {
            gates: &[and(108, 88, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                and(108, 88, A, B),
                and(108, 162, A, C),
                and(108, 236, B, C),
                or(188, 125, g(1), g(2)),
                or(268, 162, g(4), g(3)),
            ],
            output: g(5),
        },
    },
    Task {
        name: tr!(en: "Only One", zh: "只亮一个"),
        code: tr!(en: "Y = ONE-HOT(A,B,C)", zh: "Y = 独热(A,B,C)"),
        goal: tr!(
            en: "Y is 1 when exactly one of A B C is 1.",
            zh: "A B C 中只有一个为 1 时，Y 为 1。"
        ),
        hint: tr!(
            en: "The circuit here is wrong in one row only.",
            zh: "现在的电路只错一行"
        ),
        inputs: 3,
        gates: GateSet::ALL,
        par: 4,
        rule: |a, b, c| u8::from(a) + u8::from(b) + u8::from(c) == 1,
        starter: Board {
            gates: &[xor(108, 125, A, B), xor(188, 199, g(1), C)],
            output: g(2),
        },
        reference: Board {
            gates: &[
                xor(108, 125, A, B),
                xor(188, 199, g(1), C),
                nand(188, 88, A, B),
                and(268, 162, g(3), g(2)),
            ],
            output: g(4),
        },
    },
];
