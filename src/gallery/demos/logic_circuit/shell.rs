use super::input::{
    circuit_tick_system, keyboard, surface_drag_end, surface_drag_move, surface_drag_start,
    surface_tap,
};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{modal_render, pad_top, surface_render, trace_base};
use super::state::{CircuitModalSurface, CircuitSurface};
use super::style::{
    ACCENT, BACKGROUND, DIM, GREEN, HEADER_BUTTON, INK, LINE, LIVE_STEP, LIVE_TOP, MUTED,
    OUTPUT_PAD, PAD_HEIGHT, PAD_LEFT, PAD_WIDTH, PRIMARY_TOP, RED, SCAN_TOP, SIDE_LEFT, SIDE_WIDTH,
    TOOLS_TOP, TRUTH_STEP, TRUTH_TOP,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::circuit::{
    CircuitError, CircuitGate, CircuitModal, CircuitModel, CircuitPage, Controls, Evaluation,
    GateKind, GateSet, MAX_GATES, SET_SIZE, SignalSource, TASK_COUNT, TASKS, TraceSample, TruthRow,
    VerifyResult,
};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

const TITLE: Phrase = tr!(en: "LOGIC BENCH", zh: "逻辑工作台");
const HELP: Phrase = tr!(
    en: "Wire: tap an output, then an input\nUnwire: tap a wired input\nEdit: tap the selected gate; － removes it\nDrag gates to move them; tap A B C to flip them\nEvery row must pass; fewer gates earn more stars\nKeys: 1 2 3 flip inputs · Z undo · Enter check",
    zh: "连线：点输出端，再点输入端\n拆线：点已接线的输入端\n改门：再点一下选中的门；－ 删除它\n门可以拖动；点 A B C 切换 0 和 1\n每一行都对才算通过，门越少星越多\n按键：1 2 3 切换输入 · Z 撤销 · 回车 验证"
);

fn label_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn centered() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Center)
}

fn end_style() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::End)
}

/// A column of a table: one value per line from the top.
fn column_style(align: TextAlign) -> ParagraphStyle {
    ParagraphStyle::default().with_align(align)
}

fn chinese() -> bool {
    locale() == Locale::ZhCn
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

/// The stars of a task, the earned ones filled: `★★☆`.
struct StarRow(u8);

impl fmt::Display for StarRow {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for star in 0..3 {
            out.write_str(if star < self.0 { "★" } else { "☆" })?;
        }
        Ok(())
    }
}

/// "NAND only" for a task that limits the gate kinds; nothing otherwise.
struct OnlyGates(GateSet);

impl fmt::Display for OnlyGates {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_all() {
            return Ok(());
        }
        let locale = locale();
        let zh = locale == Locale::ZhCn;
        if zh {
            out.write_str("只用")?;
        }
        let kinds = GateKind::ALL
            .into_iter()
            .filter(|kind| self.0.contains(*kind));
        for (index, kind) in kinds.enumerate() {
            if index > 0 {
                out.write_str(if zh { "、" } else { "/" })?;
            }
            out.write_str(kind.label().pick(locale))?;
        }
        if !zh {
            out.write_str(" only")?;
        }
        Ok(())
    }
}

/// `☰ 02 · Both On ★★☆` on the task button.
struct TaskLabel {
    task: u8,
    stars: u8,
}

impl fmt::Display for TaskLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "☰ {:02} · {}",
            self.task + 1,
            TASKS[usize::from(self.task)].name
        )?;
        if self.stars > 0 {
            write!(out, " {}", StarRow(self.stars))?;
        }
        Ok(())
    }
}

struct InputLabel {
    index: u8,
    on: bool,
}

impl fmt::Display for InputLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "{} {}",
            char::from(b'A' + self.index),
            u8::from(self.on)
        )
    }
}

struct GateLabel(Option<CircuitGate>);

impl fmt::Display for GateLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(gate) = self.0 {
            write!(out, "G{}\n{}", gate.id, gate.kind.label())
        } else {
            Ok(())
        }
    }
}

struct OutputLabel(Evaluation);

impl fmt::Display for OutputLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.complete {
            write!(out, "Y\n{}", u8::from(self.0.value))
        } else {
            out.write_str("Y\n?")
        }
    }
}

/// Input bits of a row, A first: `0 1 0`.
struct Bits {
    inputs: u8,
    count: u8,
}

impl fmt::Display for Bits {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for index in 0..self.count {
            if index > 0 {
                out.write_str(" ")?;
            }
            write!(out, "{}", u8::from(self.inputs & (1 << index) != 0))?;
        }
        Ok(())
    }
}

/// Named input bits of a row: `A=1 B=0`.
struct NamedBits {
    inputs: u8,
    count: u8,
}

impl fmt::Display for NamedBits {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for index in 0..self.count {
            if index > 0 {
                out.write_str(" ")?;
            }
            write!(
                out,
                "{}={}",
                char::from(b'A' + index),
                u8::from(self.inputs & (1 << index) != 0)
            )?;
        }
        Ok(())
    }
}

/// The input names over a column of bits: `A B C`.
struct InputNames(u8);

impl fmt::Display for InputNames {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for index in 0..self.0 {
            if index > 0 {
                out.write_str(" ")?;
            }
            write!(out, "{}", char::from(b'A' + index))?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
enum Column {
    /// Every input bit of the row.
    Bits(u8),
    /// One input bit.
    Input(u8),
    Expected,
    Actual,
    Verdict,
}

/// One column of a truth table, a value per line.
struct TableColumn {
    rows: [Option<TruthRow>; 8],
    column: Column,
}

impl fmt::Display for TableColumn {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, row) in self.rows.iter().flatten().enumerate() {
            if index > 0 {
                out.write_str("\n")?;
            }
            match self.column {
                Column::Bits(count) => write!(
                    out,
                    "{}",
                    Bits {
                        inputs: row.inputs,
                        count,
                    }
                )?,
                Column::Input(bit) => {
                    write!(out, "{}", u8::from(row.inputs & (1 << bit) != 0))?;
                }
                Column::Expected => write!(out, "{}", u8::from(row.expected))?,
                Column::Actual if row.complete => write!(out, "{}", u8::from(row.actual))?,
                Column::Actual => out.write_str("?")?,
                Column::Verdict if row.passes() => {
                    write!(out, "{}", tr!(en: "PASS", zh: "通过"))?;
                }
                Column::Verdict => write!(out, "{}", tr!(en: "FAIL", zh: "未过"))?,
            }
        }
        Ok(())
    }
}

struct TruthRows(u8);

impl fmt::Display for TruthRows {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "真值表 · {} 行", 1 << self.0)
        } else {
            write!(out, "TRUTH TABLE · {} ROWS", 1 << self.0)
        }
    }
}

struct LastSample {
    sample: Option<TraceSample>,
    count: u8,
}

impl fmt::Display for LastSample {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(sample) = self.sample else {
            return Ok(());
        };
        write!(
            out,
            "{} → {}",
            Bits {
                inputs: sample.inputs,
                count: self.count,
            },
            u8::from(sample.output)
        )
    }
}

struct SampleHeader(u8);

impl fmt::Display for SampleHeader {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{} → Y", InputNames(self.0))
    }
}

/// `Gates 2 · par 2`: the gates on the board and the fewest for three stars.
struct GatesLine {
    gates: u8,
    par: u8,
}

impl fmt::Display for GatesLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "门数 {} · 最少 {}", self.gates, self.par)
        } else {
            write!(out, "Gates {} · par {}", self.gates, self.par)
        }
    }
}

struct StatusLine {
    notice: Option<CircuitError>,
    verify: Option<VerifyResult>,
    pending: bool,
    allowed: GateSet,
    count: u8,
    task: u8,
}

impl fmt::Display for StatusLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(notice) = self.notice {
            let words = match notice {
                CircuitError::Cycle => tr!(en: "That wire would close a loop", zh: "不能接成回路"),
                CircuitError::Duplicate => {
                    tr!(en: "This pin already carries that signal", zh: "这里已经接着这个信号")
                }
                CircuitError::Full => {
                    tr!(en: "Six gates at most: remove one first", zh: "最多 6 个门，先删掉一个")
                }
                CircuitError::Overlap => {
                    tr!(en: "No room: drag a gate aside first", zh: "没有空位，先拖开一个门")
                }
                CircuitError::NotAllowed => {
                    return write!(
                        out,
                        "{}{}",
                        tr!(en: "This task: ", zh: "本题"),
                        OnlyGates(self.allowed)
                    );
                }
                CircuitError::InvalidGate => {
                    tr!(en: "Tap a gate to select it first", zh: "先点一个门选中它")
                }
                CircuitError::InvalidSource | CircuitError::InvalidInput => {
                    tr!(en: "Tap an output, then an input", zh: "先点输出端，再点输入端")
                }
            };
            return write!(out, "{words}");
        }
        if let Some(result) = self.verify {
            if result.won {
                return if chinese() {
                    write!(out, "★ 全部 {} 行通过", result.total)
                } else {
                    write!(out, "★ All {} rows pass", result.total)
                };
            }
            if !result.wired {
                return write!(out, "{}", tr!(en: "Y is not wired yet", zh: "Y 还没接线"));
            }
            let Some(row) = result.failure.filter(|row| row.complete) else {
                return write!(
                    out,
                    "{}",
                    tr!(en: "A gate input is still open", zh: "有门的输入端空着")
                );
            };
            let bits = NamedBits {
                inputs: row.inputs,
                count: self.count,
            };
            let expected = u8::from(row.expected);
            return if chinese() {
                write!(
                    out,
                    "{}/{} 行通过 · {} 时 Y 应为 {}",
                    result.passed, result.total, bits, expected
                )
            } else {
                write!(
                    out,
                    "{}/{} rows pass · at {}, Y should be {}",
                    result.passed, result.total, bits, expected
                )
            };
        }
        if self.pending {
            write!(
                out,
                "{}",
                tr!(en: "Tap an input to finish the wire", zh: "点一个输入端完成接线")
            )
        } else {
            write!(out, "{}", TASKS[usize::from(self.task)].hint)
        }
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
    positions[index].map_or(0, |(x, _)| i32::from(x) - 31)
}

/// The label's two lines sit in the gate body, below its header band.
fn gate_top(positions: [Option<(i16, i16)>; MAX_GATES], index: usize) -> i32 {
    positions[index].map_or(0, |(_, y)| i32::from(y) - 11)
}

fn gate_color(gate: Option<CircuitGate>, selected: u8) -> Color {
    if gate.is_some_and(|gate| gate.id == selected) {
        ACCENT
    } else {
        INK
    }
}

fn trace_label_top(index: usize, input_count: u8) -> i32 {
    let channel = if index == 3 {
        usize::from(input_count)
    } else {
        index
    };
    trace_base(channel, usize::from(input_count) + 1) + 3
}

struct AddLabel(Option<GateKind>);

impl fmt::Display for AddLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(kind) => write!(out, "＋ {}", kind.label()),
            None => write!(out, "＋ {}", tr!(en: "Gate", zh: "门")),
        }
    }
}

/// The main button: check the board, then move on.
struct PrimaryLabel(Controls);

impl fmt::Display for PrimaryLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.0.won {
            write!(out, "{}", tr!(en: "☑ Check", zh: "☑ 验证"))
        } else {
            write!(out, "{}", NextLabel(self.0.last_task))
        }
    }
}

struct NextLabel(bool);

impl fmt::Display for NextLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.0 {
            tr!(en: "Tasks", zh: "选题")
        } else {
            tr!(en: "Next →", zh: "下一题 →")
        };
        write!(out, "{words}")
    }
}

struct ScanLabel(bool);

impl fmt::Display for ScanLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.0 {
            tr!(en: "Ⅱ Pause", zh: "Ⅱ 暂停")
        } else {
            tr!(en: "▶ Scan", zh: "▶ 扫描")
        };
        write!(out, "{words}")
    }
}

struct TraceMode(bool);

impl fmt::Display for TraceMode {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = if self.0 {
            tr!(en: "AUTO ▶", zh: "自动 ▶")
        } else {
            tr!(en: "STEP", zh: "单步")
        };
        write!(out, "{words}")
    }
}

struct ModalTitle {
    modal: CircuitModal,
    selected: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.modal {
            CircuitModal::None | CircuitModal::Result => Ok(()),
            CircuitModal::Tasks => write!(out, "{}", tr!(en: "Choose a task", zh: "选择任务")),
            CircuitModal::GateTypes { adding: true } => {
                write!(out, "{}", tr!(en: "Add a gate", zh: "添加逻辑门"))
            }
            CircuitModal::GateTypes { adding: false } => {
                write!(out, "{} G{}", tr!(en: "Edit", zh: "修改"), self.selected)
            }
            CircuitModal::Help => write!(out, "{}", tr!(en: "How to play", zh: "操作说明")),
        }
    }
}

struct ModalSubtitle {
    modal: CircuitModal,
    allowed: GateSet,
    free: u8,
}

impl fmt::Display for ModalSubtitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let CircuitModal::GateTypes { adding } = self.modal else {
            return Ok(());
        };
        if !self.allowed.is_all() {
            write!(out, "{}", OnlyGates(self.allowed))?;
            if adding {
                out.write_str(" · ")?;
            }
        }
        if adding {
            if chinese() {
                write!(out, "还能再放 {} 个", self.free)?;
            } else {
                write!(out, "room for {} more", self.free)?;
            }
        }
        Ok(())
    }
}

struct ModalButtonLabel {
    modal: CircuitModal,
    index: usize,
}

impl fmt::Display for ModalButtonLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = match self.modal {
            CircuitModal::GateTypes { .. } if self.index < GateKind::ALL.len() => {
                GateKind::ALL[self.index].label()
            }
            CircuitModal::GateTypes { adding: false } if self.index == 5 => {
                tr!(en: "Delete", zh: "删除")
            }
            CircuitModal::Help if self.index == 3 => tr!(en: "Restart", zh: "重新开始"),
            CircuitModal::Help if self.index == 4 => tr!(en: "Solution", zh: "参考解"),
            CircuitModal::Help if self.index == 5 => tr!(en: "Got it", zh: "明白了"),
            _ => return Ok(()),
        };
        write!(out, "{words}")
    }
}

fn modal_button_visible(modal: CircuitModal, index: usize) -> bool {
    match modal {
        CircuitModal::GateTypes { adding } => {
            index < GateKind::ALL.len() || (!adding && index == 5)
        }
        CircuitModal::Help => index >= 3,
        CircuitModal::Tasks | CircuitModal::None | CircuitModal::Result => false,
    }
}

/// Help keeps its buttons below the instructions.
fn lower_row_top(modal: CircuitModal) -> i32 {
    if modal == CircuitModal::Help {
        226
    } else {
        158
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

/// The rows of the task picker, one set each.
const SET_NAMES: [Phrase; 3] = [
    tr!(en: "Basics", zh: "入门"),
    tr!(en: "NAND only", zh: "只用与非门"),
    tr!(en: "3 inputs", zh: "三输入"),
];

fn tile_left(task: u8) -> i32 {
    101 + (i32::from(task) % SET_SIZE as i32) * 86
}

fn tile_top(task: u8) -> i32 {
    90 + (i32::from(task) / SET_SIZE as i32) * 62
}

/// `★ 7 / 36`: the stars of every task together.
struct TotalStars([u8; TASK_COUNT]);

impl fmt::Display for TotalStars {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let earned: u32 = self.0.iter().map(|stars| u32::from(*stars)).sum();
        write!(out, "★ {} / {}", earned, TASK_COUNT * 3)
    }
}

struct ResultTitle(u8);

impl fmt::Display for ResultTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if chinese() {
            write!(out, "任务 {:02} 通过", self.0 + 1)
        } else {
            write!(out, "Task {:02} passed", self.0 + 1)
        }
    }
}

/// Under the stars: the gates used, and the fewest when stars were missed.
struct ResultLine {
    gates: u8,
    par: u8,
    stars: u8,
}

impl fmt::Display for ResultLine {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let plural = if self.gates == 1 { "" } else { "s" };
        match (chinese(), self.stars == 3) {
            (true, true) => write!(out, "用了 {} 个门 · 三星", self.gates),
            (true, false) => write!(out, "用了 {} 个门 · 最少 {} 个", self.gates, self.par),
            (false, true) => write!(out, "{} gate{plural} · full marks", self.gates),
            (false, false) => write!(out, "{} gate{plural} · par is {}", self.gates, self.par),
        }
    }
}

#[compose]
fn compose_header(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            TITLE,
            position: Position::Absolute,
            left: 8,
            top: 4,
            width: 76,
            height: 18,
            font_size: 14,
            text_color: BACKGROUND,
            paragraph: label_style()
        )
    };
    ui! {
        Button (
            tr!(en : "Wire", zh : "布线"),
            id: "circuit_tab_wire",
            position: Position::Absolute,
            left: 88,
            top: 1,
            width: 42,
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
            tr!(en : "Truth", zh : "真值"),
            id: "circuit_tab_truth",
            position: Position::Absolute,
            left: 132,
            top: 1,
            width: 42,
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
            tr!(en : "Trace", zh : "时序"),
            id: "circuit_tab_trace",
            position: Position::Absolute,
            left: 176,
            top: 1,
            width: 42,
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
                    stars: model.stars()[usize::from(model.task())],
                }
            },
            capacity: 48,
            id: "circuit_task",
            position: Position::Absolute,
            left: 226,
            top: 1,
            width: 222,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: HEADER_BUTTON,
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(CircuitModal::Tasks); }
    };
    ui! {
        Button (
            "?",
            id: "circuit_help",
            position: Position::Absolute,
            left: 452,
            top: 1,
            width: 24,
            height: 24,
            size: ButtonSize::Compact,
            font_size: 12,
            normal_color: HEADER_BUTTON,
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.open_modal(CircuitModal::Help); }
    }
}

/// The pad of input `index`: its name and value, flipped by a tap.
#[compose]
fn compose_input_pad(model: model!(CircuitModel), index: u8) -> Entity {
    ui! {
        Button (
            text: ${
                InputLabel {
                    index,
                    on: model.input_values()[usize::from(index)],
                }
            },
            capacity: 4,
            position: Position::Absolute,
            left: PAD_LEFT,
            top: ${ pad_top(index, model.input_count()) },
            width: PAD_WIDTH,
            height: PAD_HEIGHT,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ control_color(model.input_values()[usize::from(index)], true) },
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 0,
            visible: ${ index < model.input_count() }
        ) on Tap { model.toggle_input(index); }
    }
}

/// The name and kind of the gate in slot `index`, inside its body.
#[compose]
fn compose_gate_label(model: model!(CircuitModel), index: usize) -> Entity {
    ui! {
        Text (
            text: ${ GateLabel(model.gates()[index]) },
            capacity: 16,
            position: Position::Absolute,
            left: ${ gate_left(model.gate_positions(), index) },
            top: ${ gate_top(model.gate_positions(), index) },
            width: 62,
            height: 26,
            font_size: 8,
            text_color: ${ gate_color(model.gates()[index], model.selected()) },
            line_height: 12,
            paragraph: column_style(TextAlign::Center),
            visible: ${ model.gates()[index].is_some() }
        )
    }
}

/// The live truth table beside the board, and the gate count under it.
#[compose]
fn compose_live_table(model: model!(CircuitModel)) -> Entity {
    ui! {
        Text (
            text: ${ TASKS[usize::from(model.task())].code },
            capacity: 32,
            id: "circuit_live_code",
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: 34,
            width: SIDE_WIDTH,
            height: 14,
            font_size: 9,
            text_color: INK,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            text: ${ OnlyGates(model.allowed()) },
            capacity: 40,
            id: "circuit_live_gates",
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: 49,
            width: SIDE_WIDTH,
            height: 12,
            font_size: 8,
            text_color: ACCENT,
            paragraph: label_style(),
            visible: ${ !model.allowed().is_all() }
        )
    };
    ui! {
        Text (
            text: ${ InputNames(model.input_count()) },
            capacity: 8,
            position: Position::Absolute,
            left: SIDE_LEFT + 2,
            top: 64,
            width: 44,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
    };
    ui! {
        Text (
            "Y",
            position: Position::Absolute,
            left: SIDE_LEFT + 48,
            top: 64,
            width: 22,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            tr!(en : "Goal", zh : "目标"),
            position: Position::Absolute,
            left: SIDE_LEFT + 72,
            top: 64,
            width: 32,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: centered()
        )
    };
    ui! {
        Text (
            text: ${
                TableColumn {
                    rows: model.truth_rows(),
                    column: Column::Bits(model.input_count()),
                }
            },
            capacity: 64,
            id: "circuit_live_inputs",
            position: Position::Absolute,
            left: SIDE_LEFT + 2,
            top: LIVE_TOP,
            width: 44,
            height: 8 * LIVE_STEP,
            font_size: 8,
            text_color: INK,
            line_height: LIVE_STEP,
            paragraph: column_style(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                TableColumn {
                    rows: model.truth_rows(),
                    column: Column::Actual,
                }
            },
            capacity: 16,
            id: "circuit_live_actual",
            position: Position::Absolute,
            left: SIDE_LEFT + 48,
            top: LIVE_TOP,
            width: 22,
            height: 8 * LIVE_STEP,
            font_size: 8,
            text_color: INK,
            line_height: LIVE_STEP,
            paragraph: column_style(TextAlign::Center)
        )
    };
    ui! {
        Text (
            text: ${
                TableColumn {
                    rows: model.truth_rows(),
                    column: Column::Expected,
                }
            },
            capacity: 16,
            id: "circuit_live_expected",
            position: Position::Absolute,
            left: SIDE_LEFT + 72,
            top: LIVE_TOP,
            width: 32,
            height: 8 * LIVE_STEP,
            font_size: 8,
            text_color: MUTED,
            line_height: LIVE_STEP,
            paragraph: column_style(TextAlign::Center)
        )
    };
    ui! {
        Text (
            text: ${
                GatesLine {
                    gates: model.gate_len(),
                    par: model.par(),
                }
            },
            capacity: 32,
            id: "circuit_gates_line",
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: 210,
            width: SIDE_WIDTH,
            height: 14,
            font_size: 8,
            text_color: ${ if model.gate_len() <= model.par() { GREEN } else { MUTED } },
            paragraph: label_style()
        )
    }
}

/// Add, remove and undo under the live table.
#[compose]
fn compose_board_tools(model: model!(CircuitModel)) -> Entity {
    ui! {
        Button (
            text: ${ AddLabel(model.controls().single_kind) },
            capacity: 24,
            id: "circuit_add",
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: TOOLS_TOP,
            width: 52,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 8,
            normal_color: ${ control_color(false, model.controls().can_add) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.controls().can_add) },
            border_radius: 0
        ) on Tap { model.start_add_gate(); }
    };
    ui! {
        Button (
            "－",
            id: "circuit_remove",
            position: Position::Absolute,
            left: SIDE_LEFT + 54,
            top: TOOLS_TOP,
            width: 26,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.controls().can_edit) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.controls().can_edit) },
            border_radius: 0
        ) on Tap { model.remove_from_tap(); }
    };
    ui! {
        Button (
            "↺",
            id: "circuit_undo",
            position: Position::Absolute,
            left: SIDE_LEFT + 82,
            top: TOOLS_TOP,
            width: 24,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.controls().can_undo) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.controls().can_undo) },
            border_radius: 0
        ) on Tap { model.undo(); }
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
            height: 296,
            visible: ${ model.page() == CircuitPage::Wire && model.modal() == CircuitModal::None }
        ) {
            compose_input_pad (model, 0)
            compose_input_pad (model, 1)
            compose_input_pad (model, 2)
            compose_gate_label (model, 0)
            compose_gate_label (model, 1)
            compose_gate_label (model, 2)
            compose_gate_label (model, 3)
            compose_gate_label (model, 4)
            compose_gate_label (model, 5)
            Text (
                text: ${ OutputLabel(model.shown_output()) },
                capacity: 4,
                id: "circuit_output",
                position: Position::Absolute,
                left: OUTPUT_PAD.0,
                top: OUTPUT_PAD.1 + 5,
                width: OUTPUT_PAD.2,
                height: OUTPUT_PAD.3 - 10,
                font_size: 8,
                text_color: BACKGROUND,
                line_height: 13,
                paragraph: column_style(TextAlign::Center)
            )
            compose_live_table (model)
            compose_board_tools (model)
        }
    }
}

/// One column of the truth page.
/// The columns of the truth page before the verdict: what, left edge, width.
const TRUTH_COLUMNS: [(Column, i32, i32); 5] = [
    (Column::Input(0), 18, 24),
    (Column::Input(1), 46, 24),
    (Column::Input(2), 74, 24),
    (Column::Expected, 118, 64),
    (Column::Actual, 196, 64),
];

/// Column `index` of `TRUTH_COLUMNS`.
#[compose]
fn compose_truth_column(model: model!(CircuitModel), index: usize) -> Entity {
    ui! {
        Text (
            text: ${
                TableColumn {
                    rows: model.truth_rows(),
                    column: TRUTH_COLUMNS[index].0,
                }
            },
            capacity: 64,
            position: Position::Absolute,
            left: TRUTH_COLUMNS[index].1,
            top: TRUTH_TOP,
            width: TRUTH_COLUMNS[index].2,
            height: 8 * TRUTH_STEP,
            font_size: 8,
            text_color: INK,
            line_height: TRUTH_STEP,
            paragraph: column_style(TextAlign::Center),
            visible: ${
                match TRUTH_COLUMNS[index].0 {
                    Column::Input(bit) => bit < model.input_count(),
                    _ => true,
                }
            }
        )
    }
}

/// A column heading of the truth page.
#[compose]
fn compose_truth_heading(
    model: model!(CircuitModel),
    words: Phrase,
    bit: u8,
    left: i32,
    width: i32,
) -> Entity {
    ui! {
        Text (
            words,
            position: Position::Absolute,
            left: left,
            top: 56,
            width: width,
            height: 13,
            font_size: 8,
            text_color: MUTED,
            paragraph: centered(),
            visible: ${ bit < model.input_count() }
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
            height: 296,
            visible: ${ model.page() == CircuitPage::Truth && model.modal() == CircuitModal::None }
        ) {
            Text (
                text: ${ TruthRows(model.input_count()) },
                capacity: 32,
                id: "circuit_truth_rows",
                position: Position::Absolute,
                left: 14,
                top: 36,
                width: 330,
                height: 13,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            compose_truth_heading (model, tr!(en : "A", zh : "A"), 0, 18, 24)
            compose_truth_heading (model, tr!(en : "B", zh : "B"), 1, 46, 24)
            compose_truth_heading (model, tr!(en : "C", zh : "C"), 2, 74, 24)
            compose_truth_heading (model, tr!(en : "EXPECT", zh : "目标"), 0, 118, 64)
            compose_truth_heading (model, tr!(en : "ACTUAL", zh : "实际"), 0, 196, 64)
            compose_truth_heading (model, tr!(en : "RESULT", zh : "结果"), 0, 274, 64)
            compose_truth_column (model, 0)
            compose_truth_column (model, 1)
            compose_truth_column (model, 2)
            compose_truth_column (model, 3)
            compose_truth_column (model, 4)
            Text (
                text: ${
                    TableColumn {
                        rows: model.truth_rows(),
                        column: Column::Verdict,
                    }
                },
                capacity: 64,
                id: "circuit_truth_result",
                position: Position::Absolute,
                left: 274,
                top: TRUTH_TOP,
                width: 64,
                height: 8 * TRUTH_STEP,
                font_size: 8,
                text_color: INK,
                line_height: TRUTH_STEP,
                paragraph: column_style(TextAlign::Center)
            )
            Text (
                tr!(en : "TARGET", zh : "目标"),
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 36,
                width: SIDE_WIDTH,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].name },
                capacity: 32,
                id: "circuit_truth_title",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 52,
                width: SIDE_WIDTH,
                height: 16,
                font_size: 13,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].code },
                capacity: 32,
                id: "circuit_truth_code",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 72,
                width: SIDE_WIDTH,
                height: 14,
                font_size: 9,
                text_color: INK,
                paragraph: label_style()
            )
            Text (
                text: ${ TASKS[usize::from(model.task())].goal },
                capacity: 96,
                id: "circuit_truth_desc",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 92,
                width: SIDE_WIDTH,
                height: 56,
                font_size: 9,
                line_height: 14,
                text_color: MUTED,
                paragraph: ParagraphStyle::default()
            )
            Text (
                text: ${ OnlyGates(model.allowed()) },
                capacity: 40,
                id: "circuit_truth_gates",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 154,
                width: SIDE_WIDTH,
                height: 12,
                font_size: 8,
                text_color: ACCENT,
                paragraph: label_style(),
                visible: ${ !model.allowed().is_all() }
            )
        }
    }
}

/// The name of trace channel `index` (3 is Y), beside its line.
#[compose]
fn compose_trace_label(model: model!(CircuitModel), index: usize, name: &'static str) -> Entity {
    ui! {
        Text (
            name,
            position: Position::Absolute,
            left: 14,
            top: ${ trace_label_top(index, model.input_count()) },
            width: 18,
            height: 14,
            font_size: 9,
            text_color: if index == 3 { ACCENT } else { Color::rgb(182, 204, 211) },
            paragraph: label_style(),
            visible: ${ index == 3 || index < usize::from(model.input_count()) }
        )
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
            height: 296,
            visible: ${ model.page() == CircuitPage::Trace && model.modal() == CircuitModal::None }
        ) {
            compose_trace_label (model, 0, "A")
            compose_trace_label (model, 1, "B")
            compose_trace_label (model, 2, "C")
            compose_trace_label (model, 3, "Y")
            Text (
                tr!(en : "LAST 32 STEPS", zh : "最近 32 步"),
                position: Position::Absolute,
                left: 14,
                top: 274,
                width: 200,
                height: 12,
                font_size: 7,
                text_color: Color::rgb(156, 174, 183),
                paragraph: label_style()
            )
            Text (
                tr!(en : "SCANNER", zh : "扫描"),
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 36,
                width: 50,
                height: 12,
                font_size: 8,
                text_color: MUTED,
                paragraph: label_style()
            )
            Text (
                text: ${ TraceMode(model.scanning()) },
                capacity: 16,
                id: "circuit_trace_mode",
                position: Position::Absolute,
                left: SIDE_LEFT + 50,
                top: 36,
                width: SIDE_WIDTH - 50,
                height: 12,
                font_size: 8,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                text: ${ format_args!("{} / 32", model.trace_len()) },
                capacity: 8,
                id: "circuit_trace_count",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 52,
                width: SIDE_WIDTH,
                height: 28,
                font_size: 18,
                text_color: INK,
                paragraph: end_style()
            )
            Text (
                text: ${ SampleHeader(model.input_count()) },
                capacity: 16,
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: 90,
                width: SIDE_WIDTH,
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
                left: SIDE_LEFT,
                top: 104,
                width: SIDE_WIDTH,
                height: 14,
                font_size: 9,
                text_color: INK,
                paragraph: label_style(),
                visible: ${ model.last_sample().is_some() }
            )
            Button (
                text: ${ ScanLabel(model.scanning()) },
                capacity: 16,
                id: "circuit_scan",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: SCAN_TOP,
                width: SIDE_WIDTH,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(model.scanning(), true) },
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.toggle_scanning(); }
            Button (
                tr!(en : "Step", zh : "单步"),
                id: "circuit_step",
                position: Position::Absolute,
                left: SIDE_LEFT,
                top: TOOLS_TOP,
                width: 52,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.step_trace(); }
            Button (
                tr!(en : "Clear", zh : "清空"),
                id: "circuit_clear",
                position: Position::Absolute,
                left: SIDE_LEFT + 54,
                top: TOOLS_TOP,
                width: 52,
                height: 28,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ${ control_color(false, model.controls().can_clear) },
                pressed_color: ACCENT,
                text_color: ${ control_text_color(false, model.controls().can_clear) },
                border_radius: 0
            ) on Tap { model.clear_trace(); }
        }
    }
}

/// The main button under every page, and the one status line.
#[compose]
fn compose_footer(model: model!(CircuitModel)) -> Entity {
    ui! {
        Button (
            text: ${ PrimaryLabel(model.controls()) },
            capacity: 24,
            id: "circuit_primary",
            position: Position::Absolute,
            left: SIDE_LEFT,
            top: PRIMARY_TOP,
            width: SIDE_WIDTH,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ACCENT,
            pressed_color: INK,
            text_color: BACKGROUND,
            border_radius: 0
        ) on Tap { model.verify_or_next(); }
    };
    ui! {
        Text (
            text: ${
                StatusLine {
                    notice: model.notice(),
                    verify: model.verify_result(),
                    pending: model.pending() != SignalSource::None,
                    allowed: model.allowed(),
                    count: model.input_count(),
                    task: model.task(),
                }
            },
            capacity: 128,
            id: "circuit_status",
            position: Position::Absolute,
            left: 8,
            top: 298,
            width: 464,
            height: 18,
            font_size: 8,
            text_color: ${ status_color(model.notice(), model.verify_result()) },
            paragraph: label_style(),
            visible: ${ model.modal() == CircuitModal::None }
        )
    }
}

/// One tile of the task picker: number, stars once solved, and name.
#[compose]
fn compose_task_tile(model: model!(CircuitModel), task: u8) -> Entity {
    ui! {
        Button (
            "",
            position: Position::Absolute,
            left: tile_left(task),
            top: tile_top(task),
            width: 80,
            height: 54,
            size: ButtonSize::Custom,
            normal_color: ${ control_color(model.task() == task, true) },
            pressed_color: ACCENT,
            border_radius: 0
        ) {
            Text (
                TASK_NUMBERS[usize::from(task)],
                position: Position::Absolute,
                left: 7,
                top: 6,
                width: 20,
                height: 12,
                font_size: 8,
                text_color: ${ if model.task() == task { INK } else { DIM } },
                paragraph: label_style()
            )
            Text (
                text: ${ StarRow(model.stars()[usize::from(task)]) },
                capacity: 12,
                position: Position::Absolute,
                left: 30,
                top: 6,
                width: 44,
                height: 12,
                font_size: 8,
                text_color: ${ if model.task() == task { INK } else { ACCENT } },
                paragraph: end_style(),
                visible: ${ model.stars()[usize::from(task)] > 0 }
            )
            Text (
                TASKS[usize::from(task)].name,
                position: Position::Absolute,
                left: 7,
                top: 31,
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
fn compose_set_name(set: usize) -> Entity {
    ui! {
        Text (
            SET_NAMES[set],
            position: Position::Absolute,
            left: 37,
            top: tile_top((set * SET_SIZE) as u8) + 20,
            width: 60,
            height: 12,
            font_size: 8,
            text_color: MUTED,
            paragraph: label_style()
        )
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
                text: ${ TotalStars(model.stars()) },
                capacity: 16,
                id: "circuit_task_stars",
                position: Position::Absolute,
                left: 300,
                top: 57,
                width: 112,
                height: 16,
                font_size: 9,
                text_color: DIM,
                paragraph: end_style()
            )
            compose_set_name (0)
            compose_set_name (1)
            compose_set_name (2)
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

/// A button of the gate picker or of help, by its place in the 3 x 2 grid.
#[compose]
fn compose_modal_button(model: model!(CircuitModel), index: usize) -> Entity {
    ui! {
        Button (
            text: ${
                ModalButtonLabel {
                    modal: model.modal(),
                    index,
                }
            },
            capacity: 24,
            position: Position::Absolute,
            left: 43 + (index % 3) as i32 * 130,
            top: ${ if index < 3 { 110 } else { lower_row_top(model.modal()) } },
            width: 120,
            height: 40,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${
                modal_button_color(
                    model.modal(),
                    index,
                    model.allowed(),
                    selected_kind(model.gates(), model.selected()),
                )
            },
            pressed_color: ACCENT,
            pressed_text_color: INK,
            text_color: ${
                modal_button_text_color(
                    model.modal(),
                    index,
                    model.allowed(),
                    selected_kind(model.gates(), model.selected()),
                )
            },
            border_radius: 0,
            visible: ${ modal_button_visible(model.modal(), index) }
        ) on Tap { model.choose_modal_option(index); }
    }
}

/// The card after a pass: title, stars (painted), the gate count and the
/// way on.
#[compose]
fn compose_result_card(model: model!(CircuitModel)) -> Entity {
    ui! {
        View (
            id: "circuit_result",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == CircuitModal::Result }
        ) {
            Text (
                text: ${ ResultTitle(model.task()) },
                capacity: 32,
                id: "circuit_result_title",
                position: Position::Absolute,
                left: 82,
                top: 89,
                width: 276,
                height: 18,
                font_size: 10,
                text_color: BACKGROUND,
                paragraph: centered()
            )
            Text (
                text: ${
                    ResultLine {
                        gates: model.gate_len(),
                        par: model.par(),
                        stars: model.verify_result().map_or(0, |result| result.stars),
                    }
                },
                capacity: 48,
                id: "circuit_result_line",
                position: Position::Absolute,
                left: 82,
                top: 168,
                width: 276,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: centered()
            )
            Button (
                tr!(en : "Stay", zh : "留在本题"),
                id: "circuit_result_stay",
                position: Position::Absolute,
                left: 84,
                top: 194,
                width: 104,
                height: 30,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.choose_modal_option(1); }
            Button (
                text: ${ NextLabel(model.controls().last_task) },
                capacity: 24,
                id: "circuit_result_next",
                position: Position::Absolute,
                left: 196,
                top: 194,
                width: 160,
                height: 30,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: ACCENT,
                pressed_color: INK,
                text_color: BACKGROUND,
                border_radius: 0
            ) on Tap { model.choose_modal_option(0); }
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
                capacity: 32,
                id: "circuit_modal_title",
                position: Position::Absolute,
                left: 43,
                top: 55,
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
                top: 88,
                width: 380,
                height: 16,
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
                top: 90,
                width: 396,
                height: 126,
                font_size: 8,
                text_color: INK,
                line_height: 21,
                paragraph: ParagraphStyle::default(),
                visible: ${ model.modal() == CircuitModal::Help }
            )
            Button (
                "×",
                id: "circuit_modal_close",
                position: Position::Absolute,
                left: 421,
                top: 52,
                width: 28,
                height: 26,
                size: ButtonSize::Compact,
                font_size: 13,
                normal_color: INK,
                pressed_color: ACCENT,
                pressed_text_color: INK,
                text_color: BACKGROUND,
                border_radius: 0,
                visible: ${ model.modal() != CircuitModal::Result }
            ) on Tap { model.close_modal(); }
            compose_modal_button (model, 0)
            compose_modal_button (model, 1)
            compose_modal_button (model, 2)
            compose_modal_button (model, 3)
            compose_modal_button (model, 4)
            compose_modal_button (model, 5)
            compose_task_grid (model)
            compose_result_card (model)
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
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(CircuitModel::default()).finish();
    app.add_system(circuit_tick_system::system(model.clone()));
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.compose(parent, build_widgets(model));
}
