use super::input::{
    circuit_tick_system, surface_drag_end, surface_drag_move, surface_drag_start, surface_tap,
};
use super::render::{modal_render, surface_render};
use super::state::{CircuitModalSurface, CircuitSurface};
use super::style::{ACCENT, BACKGROUND, GREEN, INK, LINE, MUTED, PANEL, RED};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::circuit::{
    CircuitError, CircuitGate, CircuitModal, CircuitModel, CircuitPage, Controls, Evaluation,
    GateKind, GateSet, MAX_GATES, SET_SIZE, SignalSource, TASK_COUNT, TASKS, TraceSample, TruthRow,
    VerifyResult,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

/// Secondary text on the dark header and tiles.
const DIM: Color = Color::rgb(174, 186, 190);

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn centered() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Center)
}

/// A column of the truth table: one value per line from the top.
fn column_style() -> ParagraphStyle {
    ParagraphStyle::default().with_align(TextAlign::Center)
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        INK
    } else {
        LINE
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active || enabled { BACKGROUND } else { MUTED }
}

fn solved(mask: u16, task: u8) -> bool {
    mask & (1 << task) != 0
}

/// "只用 NAND" for a task that limits the gate kinds; nothing otherwise.
struct OnlyGates(GateSet);

impl core::fmt::Display for OnlyGates {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.is_all() {
            return Ok(());
        }
        formatter.write_str("只用")?;
        for kind in GateKind::ALL
            .into_iter()
            .filter(|kind| self.0.contains(*kind))
        {
            write!(formatter, " {}", kind.label())?;
        }
        Ok(())
    }
}

struct TaskLabel {
    task: u8,
    solved: bool,
}

impl core::fmt::Display for TaskLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "任务 {:02} · {}",
            self.task + 1,
            TASKS[usize::from(self.task)].name
        )?;
        if self.solved {
            formatter.write_str(" ★")?;
        }
        Ok(())
    }
}

struct InputLabel {
    index: u8,
    on: bool,
}

impl core::fmt::Display for InputLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "{} {}",
            char::from(b'A' + self.index),
            u8::from(self.on)
        )
    }
}

struct GateLabel(Option<CircuitGate>);

impl core::fmt::Display for GateLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(gate) = self.0 {
            write!(formatter, "G{}\n{}", gate.id, gate.kind.label())
        } else {
            Ok(())
        }
    }
}

struct OutputLabel(Evaluation);

impl core::fmt::Display for OutputLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.0.complete {
            write!(formatter, "Y\n{}", u8::from(self.0.value))
        } else {
            formatter.write_str("Y\n?")
        }
    }
}

/// Input bits of a row, A first: "0 1 0".
struct Bits {
    inputs: u8,
    count: u8,
}

impl core::fmt::Display for Bits {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for index in 0..self.count {
            if index > 0 {
                formatter.write_str(" ")?;
            }
            write!(formatter, "{}", u8::from(self.inputs & (1 << index) != 0))?;
        }
        Ok(())
    }
}

/// Named input bits of a row: "A=1 B=0".
struct NamedBits {
    inputs: u8,
    count: u8,
}

impl core::fmt::Display for NamedBits {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for index in 0..self.count {
            if index > 0 {
                formatter.write_str(" ")?;
            }
            write!(
                formatter,
                "{}={}",
                char::from(b'A' + index),
                u8::from(self.inputs & (1 << index) != 0)
            )?;
        }
        Ok(())
    }
}

/// The header of the live table. Y sits over the value column of the rows.
fn live_header(count: u8) -> &'static str {
    match count {
        1 => "A         Y / 目标",
        2 => "A B       Y / 目标",
        _ => "A B C    Y / 目标",
    }
}

struct CompactTruthRow {
    row: Option<TruthRow>,
    count: u8,
}

impl core::fmt::Display for CompactTruthRow {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(row) = self.row else {
            return Ok(());
        };
        // Pads every input count to the same width, so the values line up.
        let pad = usize::from(11 - 2 * self.count.min(3));
        write!(
            formatter,
            "{}{:pad$}",
            Bits {
                inputs: row.inputs,
                count: self.count,
            },
            ""
        )?;
        if row.complete {
            write!(formatter, "{}", u8::from(row.actual))?;
        } else {
            formatter.write_str("?")?;
        }
        write!(
            formatter,
            " / {}   {}",
            u8::from(row.expected),
            if row.passes() { "☑" } else { "·" }
        )
    }
}

fn row_color(row: Option<TruthRow>) -> Color {
    if row.is_some_and(TruthRow::passes) {
        GREEN
    } else {
        INK
    }
}

#[derive(Clone, Copy)]
enum Column {
    Input(u8),
    Expected,
    Actual,
    Verdict,
}

/// One column of the truth page, a value per line.
struct TruthColumn {
    rows: [Option<TruthRow>; 8],
    column: Column,
}

impl core::fmt::Display for TruthColumn {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (index, row) in self.rows.iter().flatten().enumerate() {
            if index > 0 {
                formatter.write_str("\n")?;
            }
            match self.column {
                Column::Input(bit) => {
                    write!(formatter, "{}", u8::from(row.inputs & (1 << bit) != 0))?;
                }
                Column::Expected => write!(formatter, "{}", u8::from(row.expected))?,
                Column::Actual if row.complete => write!(formatter, "{}", u8::from(row.actual))?,
                Column::Actual => formatter.write_str("?")?,
                Column::Verdict => {
                    formatter.write_str(if row.passes() { "PASS" } else { "FAIL" })?;
                }
            }
        }
        Ok(())
    }
}

struct LastSample {
    sample: Option<TraceSample>,
    count: u8,
}

impl core::fmt::Display for LastSample {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(sample) = self.sample else {
            return Ok(());
        };
        write!(
            formatter,
            "{} → {}",
            Bits {
                inputs: sample.inputs,
                count: self.count,
            },
            u8::from(sample.output)
        )
    }
}

fn sample_header(count: u8) -> &'static str {
    match count {
        1 => "A → Y",
        2 => "A B → Y",
        _ => "A B C → Y",
    }
}

struct StatusLine {
    notice: Option<CircuitError>,
    verify: Option<VerifyResult>,
    disconnecting: bool,
    pending: SignalSource,
    allowed: GateSet,
    count: u8,
    task: u8,
}

impl core::fmt::Display for StatusLine {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(notice) = self.notice {
            return match notice {
                CircuitError::Cycle => formatter.write_str("不能接成回路"),
                CircuitError::Duplicate => formatter.write_str("这里已经接着这个信号"),
                CircuitError::Full => formatter.write_str("最多 6 个门，先删掉一个"),
                CircuitError::Overlap => formatter.write_str("没有空位，先拖开一个门"),
                CircuitError::NotAllowed => write!(formatter, "本题{}", OnlyGates(self.allowed)),
                CircuitError::InvalidGate => formatter.write_str("先点一个门选中它"),
                CircuitError::InvalidSource | CircuitError::InvalidInput => {
                    formatter.write_str("先点输出端，再点输入端")
                }
            };
        }
        if let Some(result) = self.verify {
            return if result.won {
                write!(formatter, "★ 全部 {} 行通过", result.total)
            } else if !result.wired {
                formatter.write_str("Y 还没接线")
            } else if let Some(row) = result.failure.filter(|row| row.complete) {
                write!(
                    formatter,
                    "{}/{} 行通过 · {} 时 Y 应为 {}",
                    result.passed,
                    result.total,
                    NamedBits {
                        inputs: row.inputs,
                        count: self.count,
                    },
                    u8::from(row.expected)
                )
            } else {
                formatter.write_str("有门的输入端空着")
            };
        }
        formatter.write_str(if self.disconnecting {
            "点一个输入端，拆掉它的线"
        } else if self.pending != SignalSource::None {
            "点一个输入端完成接线"
        } else {
            TASKS[usize::from(self.task)].hint
        })
    }
}

fn status_color(notice: Option<CircuitError>, verify: Option<VerifyResult>) -> Color {
    match (notice, verify) {
        (Some(_), _) => RED,
        (None, Some(result)) if result.won => GREEN,
        (None, Some(_)) => INK,
        (None, None) => MUTED,
    }
}

fn gate_left(positions: [Option<(i16, i16)>; MAX_GATES], index: usize) -> i32 {
    positions[index].map_or(0, |(x, _)| i32::from(x) - 29)
}

/// The label's two lines sit in the gate body, below its header band.
fn gate_top(positions: [Option<(i16, i16)>; MAX_GATES], index: usize) -> i32 {
    positions[index].map_or(0, |(_, y)| i32::from(y) - 8)
}

fn gate_color(gate: Option<CircuitGate>, selected: u8) -> Color {
    if gate.is_some_and(|gate| gate.id == selected) {
        ACCENT
    } else {
        INK
    }
}

fn live_row_top(input_count: u8, index: usize) -> i32 {
    let step = if input_count == 3 { 15 } else { 30 };
    116 + index as i32 * step
}

fn trace_label_top(index: usize, input_count: u8) -> i32 {
    let channel = if index == 3 {
        usize::from(input_count)
    } else {
        index
    };
    83 + channel as i32 * (150 / (i32::from(input_count) + 1))
}

fn add_label(kind: Option<GateKind>) -> &'static str {
    match kind {
        Some(GateKind::And) => "＋ AND",
        Some(GateKind::Or) => "＋ OR",
        Some(GateKind::Xor) => "＋ XOR",
        Some(GateKind::Not) => "＋ NOT",
        Some(GateKind::Nand) => "＋ NAND",
        None => "＋ 逻辑门",
    }
}

fn footer_label(controls: Controls, index: usize) -> &'static str {
    let check = if !controls.won {
        "☑ 验证"
    } else if controls.last_task {
        "选题"
    } else {
        "下一题 →"
    };
    if controls.page == CircuitPage::Trace {
        [
            "单步",
            if controls.scanning {
                "Ⅱ 暂停"
            } else {
                "▶ 扫描"
            },
            "清空",
            check,
            "返回布线",
        ][index]
    } else {
        [
            add_label(controls.single_kind),
            if controls.single_kind.is_some() {
                "删除"
            } else {
                "类型 / 删除"
            },
            if controls.disconnecting {
                "取消断线"
            } else {
                "断开连线"
            },
            "↺ 撤销",
            check,
        ][index]
    }
}

fn footer_enabled(controls: Controls, index: usize) -> bool {
    if controls.page == CircuitPage::Trace {
        index != 2 || controls.can_clear
    } else {
        match index {
            0 => controls.can_add,
            1 => controls.can_edit,
            3 => controls.can_undo,
            _ => true,
        }
    }
}

fn footer_active(controls: Controls, index: usize) -> bool {
    if controls.page == CircuitPage::Trace {
        index == 3 || (index == 1 && controls.scanning)
    } else {
        index == 4 || (index == 2 && controls.disconnecting)
    }
}

fn footer_color(controls: Controls, index: usize) -> Color {
    control_color(
        footer_active(controls, index),
        footer_enabled(controls, index),
    )
}

fn footer_text_color(controls: Controls, index: usize) -> Color {
    control_text_color(
        footer_active(controls, index),
        footer_enabled(controls, index),
    )
}

struct ModalTitle {
    modal: CircuitModal,
    selected: u8,
}

impl core::fmt::Display for ModalTitle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.modal {
            CircuitModal::None => Ok(()),
            CircuitModal::Tasks => formatter.write_str("选择任务"),
            CircuitModal::GateTypes { adding: true } => formatter.write_str("添加逻辑门"),
            CircuitModal::GateTypes { adding: false } => {
                write!(formatter, "修改 G{}", self.selected)
            }
            CircuitModal::Help => formatter.write_str("操作说明"),
        }
    }
}

struct ModalSubtitle {
    modal: CircuitModal,
    allowed: GateSet,
    free: u8,
}

impl core::fmt::Display for ModalSubtitle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let CircuitModal::GateTypes { adding } = self.modal else {
            return Ok(());
        };
        if !self.allowed.is_all() {
            write!(formatter, "本题{}", OnlyGates(self.allowed))?;
            if adding {
                formatter.write_str(" · ")?;
            }
        }
        if adding {
            write!(formatter, "还能再放 {} 个", self.free)?;
        }
        Ok(())
    }
}

const HELP: &str = "连线：点输出端，再点输入端\n拆线：点 断开连线，再点要拆的输入端\n改门：选中门，点 类型 / 删除\n门可以拖动；点 A B C 切换 0 和 1\n真值表每一行都对，才算通过";

struct ModalButtonLabel {
    modal: CircuitModal,
    index: usize,
}

impl core::fmt::Display for ModalButtonLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self.modal {
            CircuitModal::GateTypes { .. } if self.index < GateKind::ALL.len() => {
                GateKind::ALL[self.index].label()
            }
            CircuitModal::GateTypes { adding: false } if self.index == 5 => "删除",
            CircuitModal::Help if self.index == 3 => "重新开始",
            CircuitModal::Help if self.index == 4 => "参考解",
            CircuitModal::Help if self.index == 5 => "明白了",
            _ => "",
        })
    }
}

fn modal_button_visible(modal: CircuitModal, index: usize) -> bool {
    match modal {
        CircuitModal::GateTypes { adding } => {
            index < GateKind::ALL.len() || (!adding && index == 5)
        }
        CircuitModal::Help => index >= 3,
        CircuitModal::Tasks | CircuitModal::None => false,
    }
}

/// Help keeps its buttons below the instructions.
fn lower_row_top(modal: CircuitModal) -> i32 {
    if modal == CircuitModal::Help {
        208
    } else {
        183
    }
}

fn selected_kind(gates: [Option<CircuitGate>; MAX_GATES], selected: u8) -> Option<GateKind> {
    gates
        .into_iter()
        .flatten()
        .find(|gate| gate.id == selected)
        .map(|gate| gate.kind)
}

/// The gate's current kind while editing it, and the close button of help.
fn modal_button_active(modal: CircuitModal, index: usize, kind: Option<GateKind>) -> bool {
    match modal {
        CircuitModal::GateTypes { adding: false } => {
            index < GateKind::ALL.len() && kind == Some(GateKind::ALL[index])
        }
        CircuitModal::Help => index == 5,
        _ => false,
    }
}

fn modal_button_enabled(modal: CircuitModal, index: usize, allowed: GateSet) -> bool {
    !matches!(modal, CircuitModal::GateTypes { .. })
        || index >= GateKind::ALL.len()
        || allowed.contains(GateKind::ALL[index])
}

fn modal_button_color(
    modal: CircuitModal,
    index: usize,
    allowed: GateSet,
    kind: Option<GateKind>,
) -> Color {
    control_color(
        modal_button_active(modal, index, kind),
        modal_button_enabled(modal, index, allowed),
    )
}

fn modal_button_text_color(
    modal: CircuitModal,
    index: usize,
    allowed: GateSet,
    kind: Option<GateKind>,
) -> Color {
    control_text_color(
        modal_button_active(modal, index, kind),
        modal_button_enabled(modal, index, allowed),
    )
}

const TASK_NUMBERS: [&str; TASK_COUNT] = [
    "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12",
];

fn tile_left(task: u8) -> i32 {
    101 + (i32::from(task) % SET_SIZE as i32) * 86
}

fn tile_top(task: u8) -> i32 {
    106 + (i32::from(task) / SET_SIZE as i32) * 53
}

struct SolvedCount(u16);

impl core::fmt::Display for SolvedCount {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "★ {} / {}", self.0.count_ones(), TASK_COUNT)
    }
}

#[compose]
fn compose_header(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            "逻辑工作台",
            position: Position::Absolute,
            left: 24,
            top: 6,
            width: 180,
            height: 20,
            font_size: 14,
            text_color: BACKGROUND,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("NET · {} / 6 GATES", model.gate_len()) },
            capacity: 20,
            id: "circuit_gate_count",
            position: Position::Absolute,
            left: 320,
            top: 8,
            width: 130,
            height: 16,
            font_size: 9,
            text_color: DIM,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Button (
            "?",
            position: Position::Absolute,
            left: 451,
            top: 4,
            width: 24,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 12,
            normal_color: INK,
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(CircuitModal::Help); }
    };
    ui! {
        Button (
            "布线",
            id: "circuit_tab_wire",
            position: Position::Absolute,
            left: 24,
            top: 33,
            width: 96,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == CircuitPage::Wire, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(CircuitPage::Wire); }
    };
    ui! {
        Button (
            "真值",
            id: "circuit_tab_truth",
            position: Position::Absolute,
            left: 124,
            top: 33,
            width: 82,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == CircuitPage::Truth, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(CircuitPage::Truth); }
    };
    ui! {
        Button (
            "时序",
            id: "circuit_tab_trace",
            position: Position::Absolute,
            left: 210,
            top: 33,
            width: 82,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.page() == CircuitPage::Trace, true) },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(CircuitPage::Trace); }
    };
    ui! {
        Button (
            text: ${
                TaskLabel {
                    task: model.task(),
                    solved: solved(model.solved_mask(), model.task()),
                }
            },
            capacity: 40,
            id: "circuit_task",
            position: Position::Absolute,
            left: 310,
            top: 34,
            width: 159,
            height: 22,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: PANEL,
            pressed_color: ACCENT,
            text_color: INK,
            border_radius: 0
        ) on Tap { model.open_modal(CircuitModal::Tasks); }
    }
}

#[compose]
fn compose_wire_inputs(model: model!(CircuitModel)) -> Entity {
    ui! {
        Button (
            text: ${
                InputLabel {
                    index: 0,
                    on: model.input_values()[0],
                }
            },
            capacity: 3,
            id: "circuit_input_0",
            position: Position::Absolute,
            left: 17,
            top: 87,
            width: 30,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(model.input_values()[0], true) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.input_values()[0], true) },
            border_radius: 0
        ) on Tap { model.toggle_input(0); }
    };
    ui! {
        Button (
            text: ${
                InputLabel {
                    index: 1,
                    on: model.input_values()[1],
                }
            },
            capacity: 3,
            id: "circuit_input_1",
            position: Position::Absolute,
            left: 17,
            top: 141,
            width: 30,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(model.input_values()[1], true) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.input_values()[1], true) },
            border_radius: 0,
            visible: ${ model.input_count() >= 2 }
        ) on Tap { model.toggle_input(1); }
    };
    ui! {
        Button (
            text: ${
                InputLabel {
                    index: 2,
                    on: model.input_values()[2],
                }
            },
            capacity: 3,
            id: "circuit_input_2",
            position: Position::Absolute,
            left: 17,
            top: 195,
            width: 30,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(model.input_values()[2], true) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.input_values()[2], true) },
            border_radius: 0,
            visible: ${ model.input_count() == 3 }
        ) on Tap { model.toggle_input(2); }
    }
}

#[compose]
fn compose_wire_gates(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[0]) },
            capacity: 8,
            id: "circuit_gate_0",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 0) },
            top: ${ gate_top(model.gate_positions(), 0) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[0], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[0].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[1]) },
            capacity: 8,
            id: "circuit_gate_1",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 1) },
            top: ${ gate_top(model.gate_positions(), 1) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[1], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[1].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[2]) },
            capacity: 8,
            id: "circuit_gate_2",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 2) },
            top: ${ gate_top(model.gate_positions(), 2) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[2], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[2].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[3]) },
            capacity: 8,
            id: "circuit_gate_3",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 3) },
            top: ${ gate_top(model.gate_positions(), 3) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[3], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[3].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[4]) },
            capacity: 8,
            id: "circuit_gate_4",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 4) },
            top: ${ gate_top(model.gate_positions(), 4) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[4], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[4].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[5]) },
            capacity: 8,
            id: "circuit_gate_5",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 5) },
            top: ${ gate_top(model.gate_positions(), 5) },
            width: 58,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[5], model.selected()) },
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[5].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ OutputLabel(model.evaluation()) },
            capacity: 3,
            id: "circuit_output",
            position: Position::Absolute,
            left: 288,
            top: 139,
            width: 24,
            height: 28,
            font_size: 8,
            text_color: BACKGROUND,
            line_height: 12,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
        )
    }
}

#[compose]
fn compose_wire_live_truth(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            "TRUTH TABLE",
            position: Position::Absolute,
            left: 334,
            top: 72,
            width: 70,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ OnlyGates(model.allowed()) },
            capacity: 32,
            id: "circuit_live_gates",
            position: Position::Absolute,
            left: 404,
            top: 72,
            width: 56,
            height: 12,
            font_size: 8,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End),
            visible: ${ !model.allowed().is_all() }
        )
    };
    ui! {
        Text (
            text: ${ TASKS[usize::from(model.task())].code },
            capacity: 24,
            id: "circuit_live_code",
            position: Position::Absolute,
            left: 334,
            top: 86,
            width: 125,
            height: 13,
            font_size: 9,
            text_color: INK,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ live_header(model.input_count()) },
            capacity: 24,
            position: Position::Absolute,
            left: 334,
            top: 101,
            width: 125,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[0],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_0",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 0) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[0]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[0].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[1],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_1",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 1) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[1]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[1].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[2],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_2",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 2) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[2]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[2].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[3],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_3",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 3) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[3]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[3].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[4],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_4",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 4) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[4]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[4].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[5],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_5",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 5) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[5]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[5].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[6],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_6",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 6) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[6]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[6].is_some() }
        )
    };
    ui! {
        Text (
            text: ${
                CompactTruthRow {
                    row: model.truth_rows()[7],
                    count: model.input_count(),
                }
            },
            capacity: 32,
            id: "circuit_live_7",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 7) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: ${ row_color(model.truth_rows()[7]) },
            paragraph: label_style(),
            visible: ${ model.truth_rows()[7].is_some() }
        )
    }
}

#[compose]
fn compose_status(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            text: ${
                StatusLine {
                    notice: model.notice(),
                    verify: model.verify_result(),
                    disconnecting: model.disconnecting(),
                    pending: model.pending(),
                    allowed: model.allowed(),
                    count: model.input_count(),
                    task: model.task(),
                }
            },
            capacity: 128,
            id: "circuit_wire_status",
            position: Position::Absolute,
            left: 16,
            top: 259,
            width: 447,
            height: 18,
            font_size: 8,
            text_color: ${ status_color(model.notice(), model.verify_result()) },
            paragraph: label_style(),
            visible: ${ model.modal() == CircuitModal::None }
        )
    }
}

#[compose]
fn compose_wire_page(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_wire_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Wire && model.modal() == CircuitModal::None }
        ) {
            compose_wire_inputs (model)
            compose_wire_gates (model)
            compose_wire_live_truth (model)
        }
    }
}

#[compose]
fn compose_truth_table(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            "A",
            position: Position::Absolute,
            left: 20,
            top: 96,
            width: 20,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            "B",
            position: Position::Absolute,
            left: 44,
            top: 96,
            width: 20,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered(),
            visible: ${ model.input_count() >= 2 }
        )
    };
    ui! {
        Text (
            "C",
            position: Position::Absolute,
            left: 68,
            top: 96,
            width: 20,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered(),
            visible: ${ model.input_count() == 3 }
        )
    };
    ui! {
        Text (
            "EXPECT",
            position: Position::Absolute,
            left: 100,
            top: 96,
            width: 50,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            "ACTUAL",
            position: Position::Absolute,
            left: 160,
            top: 96,
            width: 50,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            "RESULT",
            position: Position::Absolute,
            left: 222,
            top: 96,
            width: 50,
            height: 13,
            font_size: 7,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Input(0),
                }
            },
            capacity: 16,
            id: "circuit_truth_a",
            position: Position::Absolute,
            left: 20,
            top: 113,
            width: 20,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style()
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Input(1),
                }
            },
            capacity: 16,
            id: "circuit_truth_b",
            position: Position::Absolute,
            left: 44,
            top: 113,
            width: 20,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style(),
            visible: ${ model.input_count() >= 2 }
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Input(2),
                }
            },
            capacity: 16,
            id: "circuit_truth_c",
            position: Position::Absolute,
            left: 68,
            top: 113,
            width: 20,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style(),
            visible: ${ model.input_count() == 3 }
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Expected,
                }
            },
            capacity: 16,
            id: "circuit_truth_expected",
            position: Position::Absolute,
            left: 100,
            top: 113,
            width: 50,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style()
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Actual,
                }
            },
            capacity: 16,
            id: "circuit_truth_actual",
            position: Position::Absolute,
            left: 160,
            top: 113,
            width: 50,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style()
        )
    };
    ui! {
        Text (
            text: ${
                TruthColumn {
                    rows: model.truth_rows(),
                    column: Column::Verdict,
                }
            },
            capacity: 40,
            id: "circuit_truth_result",
            position: Position::Absolute,
            left: 222,
            top: 113,
            width: 50,
            height: 130,
            font_size: 7,
            text_color: INK,
            line_height: 16,
            paragraph: column_style()
        )
    }
}

#[compose]
fn compose_truth_page(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_truth_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Truth && model.modal() == CircuitModal::None }
        ) {
            Text (
                text: ${ format_args!("TRUTH TABLE · {} ROWS", 1_u8 << model.input_count()) },
                capacity: 24,
                id: "circuit_truth_rows",
                position: Position::Absolute,
                left: 23,
                top: 76,
                width: 250,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            compose_truth_table (model)
            Text (
                "TARGET",
                position: Position::Absolute,
                left: 334,
                top: 76,
                width: 125,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].name },
                capacity: 24,
                id: "circuit_truth_title",
                position: Position::Absolute,
                left: 334,
                top: 96,
                width: 125,
                height: 18,
                font_size: 13,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].code },
                capacity: 24,
                id: "circuit_truth_code",
                position: Position::Absolute,
                left: 334,
                top: 116,
                width: 125,
                height: 13,
                font_size: 9,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].goal },
                capacity: 64,
                id: "circuit_truth_desc",
                position: Position::Absolute,
                left: 334,
                top: 136,
                width: 120,
                height: 54,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
            Text (
                text: ${ OnlyGates(model.allowed()) },
                capacity: 32,
                id: "circuit_truth_gates",
                position: Position::Absolute,
                left: 334,
                top: 206,
                width: 120,
                height: 13,
                font_size: 8,
                text_color: ACCENT,
                paragraph: label_style(),
                visible: ${ !model.allowed().is_all() }
            )
        }
    }
}

#[compose]
fn compose_trace_page(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_trace_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Trace && model.modal() == CircuitModal::None }
        ) {
            Text (
                "A",
                id: "circuit_trace_a",
                position: Position::Absolute,
                left: 22,
                top: ${ trace_label_top(0, model.input_count()) },
                width: 18,
                height: 14,
                font_size: 9,
                text_color: Color::rgb(182, 204, 211),
                paragraph: label_style()
            )
            Text (
                "B",
                id: "circuit_trace_b",
                position: Position::Absolute,
                left: 22,
                top: ${ trace_label_top(1, model.input_count()) },
                width: 18,
                height: 14,
                font_size: 9,
                text_color: Color::rgb(182, 204, 211),
                paragraph: label_style(),
                visible: ${ model.input_count() >= 2 }
            )
            Text (
                "C",
                id: "circuit_trace_c",
                position: Position::Absolute,
                left: 22,
                top: ${ trace_label_top(2, model.input_count()) },
                width: 18,
                height: 14,
                font_size: 9,
                text_color: Color::rgb(182, 204, 211),
                paragraph: label_style(),
                visible: ${ model.input_count() == 3 }
            )
            Text (
                "Y",
                id: "circuit_trace_y",
                position: Position::Absolute,
                left: 22,
                top: ${ trace_label_top(3, model.input_count()) },
                width: 18,
                height: 14,
                font_size: 9,
                text_color: ACCENT,
                paragraph: label_style()
            )
            Text (
                "LAST 32 STEPS",
                position: Position::Absolute,
                left: 22,
                top: 240,
                width: 260,
                height: 12,
                font_size: 7,
                text_color: Color::rgb(156, 174, 183),
                paragraph: label_style()
            )
            Text (
                "SCANNER",
                position: Position::Absolute,
                left: 334,
                top: 78,
                width: 125,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ if model.scanning() { "AUTO ▶" } else { "STEP" } },
                capacity: 12,
                id: "circuit_trace_mode",
                position: Position::Absolute,
                left: 334,
                top: 100,
                width: 125,
                height: 16,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / 32", model.trace_len()) },
                capacity: 7,
                id: "circuit_trace_count",
                position: Position::Absolute,
                left: 334,
                top: 124,
                width: 125,
                height: 28,
                font_size: 18,
                text_color: INK,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                text: ${ sample_header(model.input_count()) },
                capacity: 16,
                position: Position::Absolute,
                left: 334,
                top: 176,
                width: 125,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style(),
                visible: ${ model.last_sample().is_some() }
            )
            Text (
                text: ${
                    LastSample {
                        sample: model.last_sample(),
                        count: model.input_count(),
                    }
                },
                capacity: 16,
                id: "circuit_trace_last",
                position: Position::Absolute,
                left: 334,
                top: 192,
                width: 125,
                height: 14,
                font_size: 9,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.last_sample().is_some() }
            )
        }
    }
}

#[compose]
fn compose_footer(model: model!(CircuitModel)) -> Entity {
    ui! {
        Button (
            text: ${ footer_label(model.controls(), 0) },
            capacity: 32,
            id: "circuit_footer_0",
            position: Position::Absolute,
            left: 7,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ footer_color(model.controls(), 0) },
            pressed_color: ACCENT,
            text_color: ${ footer_text_color(model.controls(), 0) },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.step_trace();
            } else {
                model.start_add_gate();
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.controls(), 1) },
            capacity: 32,
            id: "circuit_footer_1",
            position: Position::Absolute,
            left: 101,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ footer_color(model.controls(), 1) },
            pressed_color: ACCENT,
            text_color: ${ footer_text_color(model.controls(), 1) },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.toggle_scanning();
            } else {
                model.start_edit_gate();
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.controls(), 2) },
            capacity: 32,
            id: "circuit_footer_2",
            position: Position::Absolute,
            left: 195,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ footer_color(model.controls(), 2) },
            pressed_color: ACCENT,
            text_color: ${ footer_text_color(model.controls(), 2) },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.clear_trace();
            } else {
                model.toggle_disconnecting();
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.controls(), 3) },
            capacity: 32,
            id: "circuit_footer_3",
            position: Position::Absolute,
            left: 289,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ footer_color(model.controls(), 3) },
            pressed_color: ACCENT,
            text_color: ${ footer_text_color(model.controls(), 3) },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.verify_or_next();
            } else {
                model.undo();
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.controls(), 4) },
            capacity: 32,
            id: "circuit_footer_4",
            position: Position::Absolute,
            left: 383,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ footer_color(model.controls(), 4) },
            pressed_color: ACCENT,
            text_color: ${ footer_text_color(model.controls(), 4) },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.set_page(CircuitPage::Wire);
            } else {
                model.verify_or_next();
            }
        }
    }
}

/// One tile of the task picker: number, name and a star once solved.
#[compose]
fn compose_task_tile(model: model!(CircuitModel), task: u8) -> Entity {
    ui! {
        Button (
            "",
            position: Position::Absolute,
            left: tile_left(task),
            top: tile_top(task),
            width: 80,
            height: 46,
            size: ButtonSize::Custom,
            normal_color: ${ control_color(model.task() == task, true) },
            pressed_color: ACCENT,
            border_radius: 0
        ) {
            Text (
                TASK_NUMBERS[usize::from(task)],
                position: Position::Absolute,
                left: 7,
                top: 5,
                width: 20,
                height: 12,
                font_size: 8,
                text_color: ${ if model.task() == task { INK } else { DIM } },
                paragraph: label_style()
            )
            Text (
                "★",
                position: Position::Absolute,
                left: 62,
                top: 5,
                width: 12,
                height: 12,
                font_size: 8,
                text_color: ${ if model.task() == task { INK } else { ACCENT } },
                paragraph: label_style(),
                visible: ${ solved(model.solved_mask(), task) }
            )
            Text (
                TASKS[usize::from(task)].name,
                position: Position::Absolute,
                left: 7,
                top: 25,
                width: 70,
                height: 14,
                font_size: 9,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
        } on Tap { model.choose_task(usize::from(task)); }
    }
}

#[compose]
fn compose_task_grid(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_task_grid",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == CircuitModal::Tasks }
        ) {
            Text (
                text: ${ SolvedCount(model.solved_mask()) },
                capacity: 16,
                id: "circuit_task_solved",
                position: Position::Absolute,
                left: 300,
                top: 74,
                width: 106,
                height: 16,
                font_size: 9,
                text_color: DIM,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "入门",
                position: Position::Absolute,
                left: 41,
                top: 123,
                width: 56,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "只用 NAND",
                position: Position::Absolute,
                left: 41,
                top: 176,
                width: 56,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "三输入",
                position: Position::Absolute,
                left: 41,
                top: 229,
                width: 56,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            compose_task_tile (model, 0)
            compose_task_tile (model, 1)
            compose_task_tile (model, 2)
            compose_task_tile (model, 3)
            compose_task_tile (model, 4)
            compose_task_tile (model, 5)
            compose_task_tile (model, 6)
            compose_task_tile (model, 7)
            compose_task_tile (model, 8)
            compose_task_tile (model, 9)
            compose_task_tile (model, 10)
            compose_task_tile (model, 11)
        }
    }
}

#[compose]
fn compose_modal(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            clip_children: true,
            visible: ${ model.modal() != CircuitModal::None }
        ) [
            CircuitModalSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            Text (
                text: ${
                    ModalTitle {
                        modal: model.modal(),
                        selected: model.selected(),
                    }
                },
                capacity: 24,
                id: "circuit_modal_title",
                position: Position::Absolute,
                left: 43,
                top: 72,
                width: 250,
                height: 20,
                font_size: 12,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${
                    ModalSubtitle {
                        modal: model.modal(),
                        allowed: model.allowed(),
                        free: MAX_GATES as u8 - model.gate_len(),
                    }
                },
                capacity: 64,
                id: "circuit_modal_subtitle",
                position: Position::Absolute,
                left: 43,
                top: 105,
                width: 360,
                height: 18,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style(),
                visible: ${ matches!(model.modal(), CircuitModal::GateTypes { .. }) }
            )
            Text (
                HELP,
                id: "circuit_modal_help",
                position: Position::Absolute,
                left: 43,
                top: 108,
                width: 380,
                height: 84,
                font_size: 8,
                text_color: INK,
                line_height: 17,
                paragraph: ParagraphStyle::default(),
                visible: ${ model.modal() == CircuitModal::Help }
            )
            Button (
                "×",
                position: Position::Absolute,
                left: 417,
                top: 69,
                width: 28,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.close_modal(); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 0,
                    }
                },
                capacity: 24,
                id: "circuit_modal_0",
                position: Position::Absolute,
                left: 43,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        0,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        0,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 0) }
            ) on Tap { model.choose_modal_option(0); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 1,
                    }
                },
                capacity: 24,
                id: "circuit_modal_1",
                position: Position::Absolute,
                left: 173,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        1,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        1,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 1) }
            ) on Tap { model.choose_modal_option(1); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 2,
                    }
                },
                capacity: 24,
                id: "circuit_modal_2",
                position: Position::Absolute,
                left: 303,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        2,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        2,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 2) }
            ) on Tap { model.choose_modal_option(2); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 3,
                    }
                },
                capacity: 24,
                id: "circuit_modal_3",
                position: Position::Absolute,
                left: 43,
                top: ${ lower_row_top(model.modal()) },
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        3,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        3,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 3) }
            ) on Tap { model.choose_modal_option(3); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 4,
                    }
                },
                capacity: 24,
                id: "circuit_modal_4",
                position: Position::Absolute,
                left: 173,
                top: ${ lower_row_top(model.modal()) },
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        4,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        4,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 4) }
            ) on Tap { model.choose_modal_option(4); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 5,
                    }
                },
                capacity: 24,
                id: "circuit_modal_5",
                position: Position::Absolute,
                left: 303,
                top: ${ lower_row_top(model.modal()) },
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${
                    modal_button_color(
                        model.modal(),
                        5,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: ${
                    modal_button_text_color(
                        model.modal(),
                        5,
                        model.allowed(),
                        selected_kind(model.gates(), model.selected()),
                    )
                },
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 5) }
            ) on Tap { model.choose_modal_option(5); }
            compose_task_grid (model)
        } on Tap { }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(CircuitModel)) {
    ui! {
        View (id: "circuit_surface", width: 480, height: 320, clip_children: true) [
            CircuitSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            compose_header (model)
            compose_wire_page (model)
            compose_truth_page (model)
            compose_trace_page (model)
            compose_status (model)
            compose_footer (model)
            compose_modal (model)
        } on Tap(|event: event!(Tap), rows: query!(CircuitSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        }) on DragStart(|event: event!(DragStart), rows: query!(CircuitSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_start(surface, rect, event.x, event.y)
        }) on DragMove(|event: event!(DragMove), rows: query!(CircuitSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_move(surface, rect, event.x, event.y)
        }) on DragEnd(|event: event!(DragEnd), rows: query!(CircuitSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_end(surface, rect, event.x, event.y, false)
        }) on DragCancel(|event: event!(DragCancel), rows: query!(CircuitSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_end(surface, rect, event.x, event.y, true)
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(CircuitModel::default()).finish();
    app.add_system(circuit_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
