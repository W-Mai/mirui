use super::input::{circuit_tick_system, surface_gesture};
use super::render::{modal_render, surface_render};
use super::state::{CircuitModalSurface, CircuitSurface};
use super::style::{ACCENT, BACKGROUND, GREEN, INK, LINE, MUTED, PANEL};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::circuit::{
    CircuitGate, CircuitModal, CircuitModel, CircuitPage, Evaluation, GateKind, MAX_GATES,
    SignalSource, TruthRow, VerifyResult,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
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

fn task_name(task: u8) -> &'static str {
    match task {
        0 => "不同才亮",
        1 => "多数表决",
        _ => "二选一",
    }
}

fn target_code(task: u8) -> &'static str {
    match task {
        0 => "Y = A XOR B",
        1 => "Y = AB + AC + BC",
        _ => "Y = A ? C : B",
    }
}

fn task_description(task: u8) -> &'static str {
    match task {
        0 => "A 与 B 不同时，Y 才为 1。",
        1 => "三个输入中至少两个为 1 时，Y 为 1。",
        _ => "A 为 0 选 B，A 为 1 选 C。",
    }
}

struct TaskLabel(u8);

impl core::fmt::Display for TaskLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "任务 0{} · {}", self.0 + 1, task_name(self.0))
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

struct CompactTruthRow {
    row: Option<TruthRow>,
    count: u8,
}

impl core::fmt::Display for CompactTruthRow {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(row) = self.row else {
            return Ok(());
        };
        let a = u8::from(row.inputs & 1 != 0);
        let b = u8::from(row.inputs & 2 != 0);
        let c = u8::from(row.inputs & 4 != 0);
        if self.count == 2 {
            write!(
                formatter,
                "{a}  {b}       {} / {}   {}",
                u8::from(row.actual),
                u8::from(row.expected),
                if row.passes() { "✓" } else { "·" }
            )
        } else {
            write!(
                formatter,
                "{a} {b} {c}     {} / {}   {}",
                u8::from(row.actual),
                u8::from(row.expected),
                if row.passes() { "✓" } else { "·" }
            )
        }
    }
}

struct ExpandedTruthRow {
    row: Option<TruthRow>,
    count: u8,
}

impl core::fmt::Display for ExpandedTruthRow {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(row) = self.row else {
            return Ok(());
        };
        let a = u8::from(row.inputs & 1 != 0);
        let b = u8::from(row.inputs & 2 != 0);
        let c = u8::from(row.inputs & 4 != 0);
        if self.count == 2 {
            write!(
                formatter,
                "{a}      {b}          {}          {}        {}",
                u8::from(row.expected),
                u8::from(row.actual),
                if row.passes() { "PASS" } else { "FAIL" }
            )
        } else {
            write!(
                formatter,
                "{a}   {b}   {c}       {}          {}       {}",
                u8::from(row.expected),
                u8::from(row.actual),
                if row.passes() { "PASS" } else { "FAIL" }
            )
        }
    }
}

struct WireStatus {
    verify: Option<VerifyResult>,
    disconnecting: bool,
    pending: SignalSource,
}

impl core::fmt::Display for WireStatus {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(result) = self.verify {
            if result.won {
                formatter.write_str("✓ 逻辑验证通过，可以选择下一项任务。")
            } else {
                write!(
                    formatter,
                    "通过 {}/{} 组，请检查真值表。",
                    result.passed, result.total
                )
            }
        } else if self.disconnecting {
            formatter.write_str("断线模式：点输入端取消连线。")
        } else if self.pending != SignalSource::None {
            formatter.write_str("正在接线：选择一个输入端。")
        } else {
            formatter.write_str("点输出端，再点输入端；拖动模块改变位置。")
        }
    }
}

fn gate_left(positions: [Option<(i16, i16)>; MAX_GATES], index: usize) -> i32 {
    positions[index].map_or(0, |(x, _)| i32::from(x) - 29)
}

fn gate_top(positions: [Option<(i16, i16)>; MAX_GATES], index: usize) -> i32 {
    positions[index].map_or(0, |(_, y)| i32::from(y) - 14)
}

fn gate_color(gate: Option<CircuitGate>, selected: u8) -> Color {
    if gate.is_some_and(|gate| gate.id == selected) {
        ACCENT
    } else {
        INK
    }
}

fn live_row_top(input_count: u8, index: usize) -> i32 {
    let step = if input_count == 2 { 30 } else { 16 };
    115 + index as i32 * step
}

fn trace_label_top(index: usize, input_count: u8) -> i32 {
    let channel = if index == 3 {
        usize::from(input_count)
    } else {
        index
    };
    83 + channel as i32 * (150 / (i32::from(input_count) + 1))
}

fn footer_label(
    page: CircuitPage,
    index: usize,
    scanning: bool,
    disconnecting: bool,
) -> &'static str {
    if page == CircuitPage::Trace {
        [
            "单步",
            if scanning { "暂停" } else { "扫描" },
            "清空记录",
            "验证",
            "返回布线",
        ][index]
    } else {
        [
            "＋ 逻辑门",
            "类型 / 删除",
            if disconnecting {
                "取消断线"
            } else {
                "断开连线"
            },
            "撤销",
            "✓ 验证",
        ][index]
    }
}

fn footer_enabled(
    page: CircuitPage,
    index: usize,
    trace_len: u8,
    gate_len: u8,
    selected: u8,
    history_len: u8,
) -> bool {
    if page == CircuitPage::Trace {
        index != 2 || trace_len != 0
    } else {
        match index {
            0 => usize::from(gate_len) < MAX_GATES,
            1 => selected != 0,
            3 => history_len != 0,
            _ => true,
        }
    }
}

fn footer_active(page: CircuitPage, index: usize, scanning: bool, disconnecting: bool) -> bool {
    (page == CircuitPage::Trace && index == 1 && scanning)
        || (page != CircuitPage::Trace && index == 2 && disconnecting)
        || index == 4
}

fn modal_title(modal: CircuitModal) -> &'static str {
    match modal {
        CircuitModal::None => "",
        CircuitModal::Tasks => "选择逻辑任务",
        CircuitModal::GateTypes { adding: true } => "添加逻辑门",
        CircuitModal::GateTypes { adding: false } => "修改逻辑门",
        CircuitModal::Help => "逻辑工作台 / 操作手册",
    }
}

fn modal_subtitle(modal: CircuitModal) -> &'static str {
    match modal {
        CircuitModal::None => "",
        CircuitModal::Tasks => "切换会清空当前网络、时序和撤销记录。",
        CircuitModal::GateTypes { adding: true } => "选择一个组合逻辑门；最多放置 6 个。",
        CircuitModal::GateTypes { adding: false } => {
            "改变类型会保留可用输入；NOT 只保留第一个输入。"
        }
        CircuitModal::Help => "输出端 → 输入端接线。真值表枚举全部输入；时序页记录离散状态。",
    }
}

struct ModalButtonLabel {
    modal: CircuitModal,
    index: usize,
}

impl core::fmt::Display for ModalButtonLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.modal {
            CircuitModal::Tasks if self.index % 2 == 1 => formatter.write_str("示范布局"),
            CircuitModal::Tasks => write!(formatter, "任务 0{}", self.index / 2 + 1),
            CircuitModal::GateTypes { .. } if self.index < GateKind::ALL.len() => {
                formatter.write_str(GateKind::ALL[self.index].label())
            }
            CircuitModal::GateTypes { adding: false } if self.index == 5 => {
                formatter.write_str("删除选中门")
            }
            CircuitModal::Help if self.index == 0 => formatter.write_str("明白了"),
            _ => Ok(()),
        }
    }
}

fn modal_button_visible(modal: CircuitModal, index: usize) -> bool {
    match modal {
        CircuitModal::Tasks => index < 6,
        CircuitModal::GateTypes { adding } => {
            index < GateKind::ALL.len() || (!adding && index == 5)
        }
        CircuitModal::Help => index == 0,
        CircuitModal::None => false,
    }
}

fn modal_button_active(modal: CircuitModal, index: usize, task: u8) -> bool {
    matches!(modal, CircuitModal::Tasks) && index % 2 == 0 && index / 2 == usize::from(task)
        || matches!(modal, CircuitModal::Help) && index == 0
}

#[compose(bind(model))]
fn compose_header(model: CircuitModel) -> Entity {
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
            text: ${ format_args!("NET / {}:6 GATES", model.gate_len()) },
            text_capacity: 15,
            id: "circuit_gate_count",
            position: Position::Absolute,
            left: 320,
            top: 8,
            width: 130,
            height: 16,
            font_size: 9,
            text_color: Color::rgb(174, 186, 190),
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
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.set_page(CircuitPage::Trace); }
    };
    ui! {
        Button (
            text: ${ TaskLabel(model.task()) },
            text_capacity: 32,
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

#[compose(bind(model))]
fn compose_wire_inputs(model: CircuitModel) -> Entity {
    ui! {
        Button (
            text: ${
                InputLabel {
                    index: 0,
                    on: model.input_values()[0],
                }
            },
            text_capacity: 3,
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
            text_capacity: 3,
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
            border_radius: 0
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
            text_capacity: 3,
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

#[compose(bind(model))]
fn compose_wire_gates(model: CircuitModel) -> Entity {
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[0]) },
            text_capacity: 8,
            id: "circuit_gate_0",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 0) },
            top: ${ gate_top(model.gate_positions(), 0) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[0], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[0].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[1]) },
            text_capacity: 8,
            id: "circuit_gate_1",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 1) },
            top: ${ gate_top(model.gate_positions(), 1) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[1], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[1].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[2]) },
            text_capacity: 8,
            id: "circuit_gate_2",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 2) },
            top: ${ gate_top(model.gate_positions(), 2) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[2], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[2].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[3]) },
            text_capacity: 8,
            id: "circuit_gate_3",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 3) },
            top: ${ gate_top(model.gate_positions(), 3) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[3], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[3].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[4]) },
            text_capacity: 8,
            id: "circuit_gate_4",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 4) },
            top: ${ gate_top(model.gate_positions(), 4) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[4], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[4].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[5]) },
            text_capacity: 8,
            id: "circuit_gate_5",
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), 5) },
            top: ${ gate_top(model.gate_positions(), 5) },
            width: 58,
            height: 28,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[5], model.selected()) },
            line_height: 9,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center),
            visible: ${ model.gates()[5].is_some() }
        )
    };
    ui! {
        Text (
            text: ${ OutputLabel(model.evaluation()) },
            text_capacity: 3,
            id: "circuit_output",
            position: Position::Absolute,
            left: 288,
            top: 139,
            width: 24,
            height: 28,
            font_size: 8,
            text_color: BACKGROUND,
            line_height: 10,
            paragraph: ParagraphStyle::default().with_align(TextAlign::Center)
        )
    }
}

#[compose(bind(model))]
fn compose_wire_live_truth(model: CircuitModel) -> Entity {
    ui! {
        Text (
            "LIVE TRUTH TABLE",
            position: Position::Absolute,
            left: 334,
            top: 76,
            width: 125,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ target_code(model.task()) },
            text_capacity: 16,
            id: "circuit_live_code",
            position: Position::Absolute,
            left: 334,
            top: 94,
            width: 125,
            height: 13,
            font_size: 9,
            text_color: INK,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            "A B C → Y / 目标",
            position: Position::Absolute,
            left: 334,
            top: 108,
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
            text_capacity: 32,
            id: "circuit_live_0",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 0) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_1",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 1) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_2",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 2) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_3",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 3) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_4",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 4) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_5",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 5) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_6",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 6) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
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
            text_capacity: 32,
            id: "circuit_live_7",
            position: Position::Absolute,
            left: 335,
            top: ${ live_row_top(model.input_count(), 7) },
            width: 126,
            height: 13,
            font_size: 7,
            text_color: GREEN,
            paragraph: label_style(),
            visible: ${ model.truth_rows()[7].is_some() }
        )
    }
}

#[compose(bind(model))]
fn compose_wire_status(model: CircuitModel) -> Entity {
    ui! {
        Text (
            text: ${
                WireStatus {
                    verify: model.verify_result(),
                    disconnecting: model.disconnecting(),
                    pending: model.pending(),
                }
            },
            text_capacity: 128,
            id: "circuit_wire_status",
            position: Position::Absolute,
            left: 16,
            top: 259,
            width: 447,
            height: 18,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    }
}

#[compose(bind(model))]
fn compose_wire_page(model: CircuitModel) -> Entity {
    ui! {
        View (
            id: "circuit_wire_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Wire }
        ) {
            compose_wire_inputs (model)
            compose_wire_gates (model)
            compose_wire_live_truth (model)
            compose_wire_status (model)
        }
    }
}

#[compose(bind(model))]
fn compose_truth_page(model: CircuitModel) -> Entity {
    ui! {
        View (
            id: "circuit_truth_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Truth }
        ) {
            Text (
                "EXHAUSTIVE INPUT TEST",
                position: Position::Absolute,
                left: 23,
                top: 77,
                width: 250,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                "A   B   C      EXPECT     ACTUAL     RESULT",
                position: Position::Absolute,
                left: 27,
                top: 98,
                width: 265,
                height: 13,
                font_size: 7,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[0],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_0",
                position: Position::Absolute,
                left: 27,
                top: 115,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[0].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[1],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_1",
                position: Position::Absolute,
                left: 27,
                top: 131,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[1].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[2],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_2",
                position: Position::Absolute,
                left: 27,
                top: 147,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[2].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[3],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_3",
                position: Position::Absolute,
                left: 27,
                top: 163,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[3].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[4],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_4",
                position: Position::Absolute,
                left: 27,
                top: 179,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[4].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[5],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_5",
                position: Position::Absolute,
                left: 27,
                top: 195,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[5].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[6],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_6",
                position: Position::Absolute,
                left: 27,
                top: 211,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[6].is_some() }
            )
            Text (
                text: ${
                    ExpandedTruthRow {
                        row: model.truth_rows()[7],
                        count: model.input_count(),
                    }
                },
                text_capacity: 48,
                id: "circuit_truth_7",
                position: Position::Absolute,
                left: 27,
                top: 227,
                width: 265,
                height: 16,
                font_size: 7,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.truth_rows()[7].is_some() }
            )
            Text (
                "TARGET SPECIFICATION",
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
                text: ${ task_name(model.task()) },
                text_capacity: 24,
                id: "circuit_truth_title",
                position: Position::Absolute,
                left: 334,
                top: 100,
                width: 125,
                height: 22,
                font_size: 13,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ task_description(model.task()) },
                text_capacity: 64,
                id: "circuit_truth_desc",
                position: Position::Absolute,
                left: 334,
                top: 132,
                width: 120,
                height: 54,
                font_size: 9,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
            Text (
                "未连接输入按 0 求值，但不会通过完整性验证。",
                position: Position::Absolute,
                left: 334,
                top: 190,
                width: 120,
                height: 48,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
        }
    }
}

#[compose(bind(model))]
fn compose_trace_page(model: CircuitModel) -> Entity {
    ui! {
        View (
            id: "circuit_trace_page",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 282,
            visible: ${ model.page() == CircuitPage::Trace }
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
                paragraph: label_style()
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
                "LAST 32 SAMPLES / LOGICAL TIME",
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
                "SIGNAL SCANNER",
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
                text: ${ if model.scanning() { "AUTO / ON" } else { "STEP / READY" } },
                text_capacity: 12,
                id: "circuit_trace_mode",
                position: Position::Absolute,
                left: 334,
                top: 103,
                width: 125,
                height: 16,
                font_size: 10,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ format_args!("{} / 32", model.trace_len()) },
                text_capacity: 7,
                id: "circuit_trace_count",
                position: Position::Absolute,
                left: 334,
                top: 127,
                width: 125,
                height: 28,
                font_size: 18,
                text_color: INK,
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                "扫描按顺序遍历输入组合，记录 A/B/C/Y。它不模拟传播延迟。",
                position: Position::Absolute,
                left: 334,
                top: 164,
                width: 120,
                height: 70,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
        }
    }
}

#[compose(bind(model))]
fn compose_footer(model: CircuitModel) -> Entity {
    ui! {
        Button (
            text: ${ footer_label(model.page(), 0, model.scanning(), model.disconnecting()) },
            text_capacity: 32,
            id: "circuit_footer_0",
            position: Position::Absolute,
            left: 7,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${
                control_color(
                    footer_active(model.page(), 0, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        0,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    footer_active(model.page(), 0, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        0,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.step_trace();
            } else {
                model
                    .open_modal(CircuitModal::GateTypes {
                        adding: true,
                    });
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.page(), 1, model.scanning(), model.disconnecting()) },
            text_capacity: 32,
            id: "circuit_footer_1",
            position: Position::Absolute,
            left: 101,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${
                control_color(
                    footer_active(model.page(), 1, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        1,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    footer_active(model.page(), 1, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        1,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.toggle_scanning();
            } else {
                model
                    .open_modal(CircuitModal::GateTypes {
                        adding: false,
                    });
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.page(), 2, model.scanning(), model.disconnecting()) },
            text_capacity: 32,
            id: "circuit_footer_2",
            position: Position::Absolute,
            left: 195,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${
                control_color(
                    footer_active(model.page(), 2, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        2,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    footer_active(model.page(), 2, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        2,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
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
            text: ${ footer_label(model.page(), 3, model.scanning(), model.disconnecting()) },
            text_capacity: 32,
            id: "circuit_footer_3",
            position: Position::Absolute,
            left: 289,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${
                control_color(
                    footer_active(model.page(), 3, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        3,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    footer_active(model.page(), 3, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        3,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.verify();
            } else {
                model.undo();
            }
        }
    };
    ui! {
        Button (
            text: ${ footer_label(model.page(), 4, model.scanning(), model.disconnecting()) },
            text_capacity: 32,
            id: "circuit_footer_4",
            position: Position::Absolute,
            left: 383,
            top: 287,
            width: 90,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${
                control_color(
                    footer_active(model.page(), 4, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        4,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    footer_active(model.page(), 4, model.scanning(), model.disconnecting()),
                    footer_enabled(
                        model.page(),
                        4,
                        model.trace_len(),
                        model.gate_len(),
                        model.selected(),
                        model.history_len(),
                    ),
                )
            },
            border_radius: 0
        ) on Tap {
            if model.page() == CircuitPage::Trace {
                model.set_page(CircuitPage::Wire);
            } else {
                model.verify();
            }
        }
    }
}

#[compose(bind(model))]
fn compose_modal(model: CircuitModel) -> Entity {
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
        ] on Tap { }
        {
            Text (
                text: ${ modal_title(model.modal()) },
                text_capacity: 48,
                id: "circuit_modal_title",
                position: Position::Absolute,
                left: 43,
                top: 72,
                width: 330,
                height: 20,
                font_size: 12,
                text_color: BACKGROUND,
                paragraph: label_style()
            )
            Text (
                text: ${ modal_subtitle(model.modal()) },
                text_capacity: 128,
                id: "circuit_modal_subtitle",
                position: Position::Absolute,
                left: 43,
                top: 105,
                width: 360,
                height: 18,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
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
                text_capacity: 24,
                id: "circuit_modal_0",
                position: Position::Absolute,
                left: 43,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 0, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 0) }
            ) on Tap { let _ = model.select_modal_option(0); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 1,
                    }
                },
                text_capacity: 24,
                id: "circuit_modal_1",
                position: Position::Absolute,
                left: 173,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 1, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 1) }
            ) on Tap { let _ = model.select_modal_option(1); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 2,
                    }
                },
                text_capacity: 24,
                id: "circuit_modal_2",
                position: Position::Absolute,
                left: 303,
                top: 128,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 2, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 2) }
            ) on Tap { let _ = model.select_modal_option(2); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 3,
                    }
                },
                text_capacity: 24,
                id: "circuit_modal_3",
                position: Position::Absolute,
                left: 43,
                top: 183,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 3, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 3) }
            ) on Tap { let _ = model.select_modal_option(3); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 4,
                    }
                },
                text_capacity: 24,
                id: "circuit_modal_4",
                position: Position::Absolute,
                left: 173,
                top: 183,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 4, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 4) }
            ) on Tap { let _ = model.select_modal_option(4); }
            Button (
                text: ${
                    ModalButtonLabel {
                        modal: model.modal(),
                        index: 5,
                    }
                },
                text_capacity: 24,
                id: "circuit_modal_5",
                position: Position::Absolute,
                left: 303,
                top: 183,
                width: 120,
                height: 42,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(modal_button_active(model.modal(), 5, model.task()), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ modal_button_visible(model.modal(), 5) }
            ) on Tap { let _ = model.select_modal_option(5); }
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: CircuitModel) {
    ui! {
        View (id: "circuit_surface", width: 480, height: 320, clip_children: true) [
            CircuitSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); } on DragStart { surface_gesture(&ctx); } on DragMove { surface_gesture(&ctx); } on DragEnd { surface_gesture(&ctx); } on DragCancel { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_wire_page (model)
            compose_truth_page (model)
            compose_trace_page (model)
            compose_footer (model)
            compose_modal (model)
        }
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
    let model = app.add_model(CircuitModel::default());
    app.add_system(circuit_tick_system::system(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}
