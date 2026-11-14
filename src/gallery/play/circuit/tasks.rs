use super::types::{GateKind, GateSet, SignalSource};

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
    pub(crate) name: &'static str,
    pub(crate) code: &'static str,
    pub(crate) goal: &'static str,
    /// Shown on the status line while the player has nothing else to read.
    pub(crate) hint: &'static str,
    pub(crate) inputs: u8,
    pub(crate) gates: GateSet,
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

pub(crate) static TASKS: [Task; TASK_COUNT] = [
    Task {
        name: "取反",
        code: "Y = NOT A",
        goal: "Y 总是和 A 相反。",
        hint: "点 A 右侧的圆点，再点 NOT 左侧的圆点。NOT 也要接到 Y",
        inputs: 1,
        gates: GateSet::ALL,
        rule: |a, _, _| !a,
        starter: Board {
            gates: &[not(170, 127, OPEN)],
            output: OPEN,
        },
        reference: Board {
            gates: &[not(170, 127, A)],
            output: g(1),
        },
    },
    Task {
        name: "都亮才亮",
        code: "Y = A AND B",
        goal: "A 和 B 都为 1 时，Y 才为 1。",
        hint: "门的类型不对：点 类型 / 删除，换成 AND",
        inputs: 2,
        gates: GateSet::ALL,
        rule: |a, b, _| a && b,
        starter: Board {
            gates: &[or(170, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[and(170, 127, A, B)],
            output: g(1),
        },
    },
    Task {
        name: "A 开 B 关",
        code: "Y = A AND NOT B",
        goal: "A 为 1、B 为 0 时，Y 才为 1。",
        hint: "加一个 NOT，接在 B 和 AND 之间",
        inputs: 2,
        gates: GateSet::ALL,
        rule: |a, b, _| a && !b,
        starter: Board {
            gates: &[and(215, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[and(215, 127, A, g(2)), not(110, 154, B)],
            output: g(1),
        },
    },
    Task {
        name: "相同才亮",
        code: "Y = A XNOR B",
        goal: "A 与 B 相同时，Y 为 1。",
        hint: "XNOR 和 XOR 正好相反",
        inputs: 2,
        gates: GateSet::ALL,
        rule: |a, b, _| a == b,
        starter: EMPTY,
        reference: Board {
            gates: &[xor(110, 127, A, B), not(215, 127, g(1))],
            output: g(2),
        },
    },
    Task {
        name: "NAND → NOT",
        code: "Y = NOT A",
        goal: "Y 总是和 A 相反。",
        hint: "一个输出端可以接多个输入端",
        inputs: 1,
        gates: GateSet::only(GateKind::Nand),
        rule: |a, _, _| !a,
        starter: Board {
            gates: &[nand(170, 127, A, OPEN)],
            output: g(1),
        },
        reference: Board {
            gates: &[nand(170, 127, A, A)],
            output: g(1),
        },
    },
    Task {
        name: "NAND → AND",
        code: "Y = A AND B",
        goal: "A 和 B 都为 1 时，Y 才为 1。",
        hint: "上一题的取反，这里还能用",
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        rule: |a, b, _| a && b,
        starter: Board {
            gates: &[nand(110, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[nand(110, 127, A, B), nand(215, 127, g(1), g(1))],
            output: g(2),
        },
    },
    Task {
        name: "NAND → OR",
        code: "Y = A OR B",
        goal: "A 或 B 有一个为 1，Y 就为 1。",
        hint: "先把 A 和 B 各自取反",
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        rule: |a, b, _| a || b,
        starter: Board {
            gates: &[nand(215, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                nand(215, 127, g(2), g(3)),
                nand(110, 100, A, A),
                nand(110, 154, B, B),
            ],
            output: g(1),
        },
    },
    Task {
        name: "NAND → XOR",
        code: "Y = A XOR B",
        goal: "A 与 B 不同时，Y 为 1。",
        hint: "4 个 NAND 可以完成，第一个已经放好",
        inputs: 2,
        gates: GateSet::only(GateKind::Nand),
        rule: |a, b, _| a != b,
        starter: Board {
            gates: &[nand(100, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                nand(100, 127, A, B),
                nand(172, 94, A, g(1)),
                nand(172, 160, g(1), B),
                nand(243, 127, g(2), g(3)),
            ],
            output: g(4),
        },
    },
    Task {
        name: "奇数个 1",
        code: "Y = A XOR B XOR C",
        goal: "A B C 中有奇数个 1 时，Y 为 1。",
        hint: "XOR 的输出还能再接 XOR",
        inputs: 3,
        gates: GateSet::ALL,
        rule: |a, b, c| a ^ b ^ c,
        starter: Board {
            gates: &[xor(110, 127, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[xor(110, 127, A, B), xor(215, 153, g(1), C)],
            output: g(2),
        },
    },
    Task {
        name: "二选一",
        code: "Y = A ? C : B",
        goal: "A 为 0 选 B，A 为 1 选 C。",
        hint: "A 为 1 选 C 已经接好，再接 A 为 0 选 B",
        inputs: 3,
        gates: GateSet::ALL,
        rule: |a, b, c| if a { c } else { b },
        starter: Board {
            gates: &[and(100, 199, A, C)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                and(100, 199, A, C),
                not(100, 100, A),
                and(172, 118, g(2), B),
                or(243, 153, g(3), g(1)),
            ],
            output: g(4),
        },
    },
    Task {
        name: "多数表决",
        code: "Y = AB + AC + BC",
        goal: "至少两个为 1 时，Y 为 1。",
        hint: "5 个门可以完成，4 个也行",
        inputs: 3,
        gates: GateSet::ALL,
        rule: |a, b, c| u8::from(a) + u8::from(b) + u8::from(c) >= 2,
        starter: Board {
            gates: &[and(105, 94, A, B)],
            output: g(1),
        },
        reference: Board {
            gates: &[
                and(105, 94, A, B),
                and(105, 151, A, C),
                and(105, 208, B, C),
                or(215, 122, g(1), g(2)),
                or(215, 180, g(4), g(3)),
            ],
            output: g(5),
        },
    },
    Task {
        name: "只亮一个",
        code: "Y = ONE-HOT(A,B,C)",
        goal: "A B C 中只有一个为 1 时，Y 为 1。",
        hint: "现在的电路只错一行",
        inputs: 3,
        gates: GateSet::ALL,
        rule: |a, b, c| u8::from(a) + u8::from(b) + u8::from(c) == 1,
        starter: Board {
            gates: &[xor(100, 150, A, B), xor(172, 190, g(1), C)],
            output: g(2),
        },
        reference: Board {
            gates: &[
                xor(100, 150, A, B),
                xor(172, 190, g(1), C),
                nand(100, 94, A, B),
                and(243, 137, g(3), g(2)),
            ],
            output: g(4),
        },
    },
];
