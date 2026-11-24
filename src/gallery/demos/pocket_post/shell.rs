use super::input::{keyboard, post_tick_system};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{
    QUEUE_TOP, STATION_LEFT, modal_frame, modal_render, queue_slot_left, surface_render,
};
use super::state::{PostModalSurface, PostSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, CONTROL, CONTROL_DISABLED, MISS, MUTED, STATION_COLORS, TEXT,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::font::GameFontPlugin;
use crate::gallery::play::post::{
    MAIN_Y, PostModal, PostModel, SHIFT_COUNT, SHIFTS, STATION_Y, STATIONS, SWITCH_X,
};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

const STATION_LETTERS: [&str; 3] = ["A", "B", "C"];
/// The foot of the board, under the rails.
const FOOT_TOP: i32 = QUEUE_TOP + 2;

/// The one status line, at the foot of the board.
struct StatusLine {
    started: bool,
    finished: bool,
    running: bool,
    missed: u8,
    streak: u8,
}

impl fmt::Display for StatusLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (missed, streak) = (self.missed, self.streak);
        let missed_word = tr!(en: "missed", zh: "错投");
        let streak_word = tr!(en: "streak", zh: "连对");
        if !self.started {
            write!(
                output,
                "{}",
                tr!(en: "▶ to start · tap S1 S2 to switch", zh: "按 ▶ 开始 · 点 S1 S2 拨道岔")
            )
        } else if self.finished {
            let done = tr!(en: "Shift done", zh: "这一班送完了");
            write!(output, "{done} · {missed_word} {missed}")
        } else if self.running {
            write!(output, "{missed_word} {missed} · {streak_word} {streak}")
        } else {
            let paused = tr!(en: "Paused", zh: "已暂停");
            write!(
                output,
                "{paused} · {missed_word} {missed} · {streak_word} {streak}"
            )
        }
    }
}

fn run_label(running: bool) -> &'static str {
    if running { "Ⅱ" } else { "▶" }
}

fn speed_label(speed_x2: u8) -> &'static str {
    match speed_x2 {
        1 => "0.5×",
        3 => "1.5×",
        _ => "1×",
    }
}

fn destination_letter(destination: Option<u8>) -> &'static str {
    destination.map_or("", |station| STATION_LETTERS[usize::from(station)])
}

fn destination_color(destination: Option<u8>) -> Color {
    destination
        .map(|station| STATION_COLORS[usize::from(station)])
        .unwrap_or(TEXT)
}

fn control_color(active: bool, enabled: bool) -> Color {
    if active {
        ACCENT
    } else if enabled {
        CONTROL
    } else {
        CONTROL_DISABLED
    }
}

fn control_text_color(active: bool, enabled: bool) -> Color {
    if active {
        BACKGROUND
    } else if enabled {
        TEXT
    } else {
        Color::rgb(103, 120, 111)
    }
}

fn starred(mask: u8, shift: u8) -> bool {
    mask & (1 << shift) != 0
}

fn tile_text_color(current: bool, starred: bool) -> Color {
    if current {
        BACKGROUND
    } else if starred {
        STATION_COLORS[0]
    } else {
        TEXT
    }
}

struct ShiftLabel(u8);

impl fmt::Display for ShiftLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            output,
            "{:02} {}",
            self.0 + 1,
            SHIFTS[usize::from(self.0)].name
        )
    }
}

/// A shift in the list: name, size and pace, then its star and best score.
struct ShiftTile {
    shift: u8,
    starred: bool,
    best: u16,
}

impl fmt::Display for ShiftTile {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shift = &SHIFTS[usize::from(self.shift)];
        let tenths = u32::from(shift.spawn_period) * 10 / 120;
        let (parcels, whole, tenth) = (shift.parcels.len(), tenths / 10, tenths % 10);
        writeln!(output, "{}", ShiftLabel(self.shift))?;
        match locale() {
            Locale::ZhCn => writeln!(output, "{parcels} 件 · 每 {whole}.{tenth} 秒")?,
            _ => writeln!(output, "{parcels} parcels · every {whole}.{tenth} s")?,
        }
        let star = if self.starred { "★" } else { "☆" };
        if self.best == 0 {
            output.write_str(star)
        } else {
            write!(
                output,
                "{star} {} {}",
                tr!(en: "best", zh: "最高"),
                self.best
            )
        }
    }
}

struct ModalTitle {
    modal: PostModal,
    shift: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.modal {
            PostModal::Shifts => write!(output, "{}", tr!(en: "Shifts", zh: "选择班次")),
            PostModal::Summary => write!(
                output,
                "{} {}",
                SHIFTS[usize::from(self.shift)].name,
                tr!(en: "done", zh: "完成")
            ),
            PostModal::Reset => write!(
                output,
                "{}",
                tr!(en: "Restart this shift?", zh: "重来这一班？")
            ),
            PostModal::None => Ok(()),
        }
    }
}

struct ModalSubtitle {
    modal: PostModal,
    missed: u8,
}

impl fmt::Display for ModalSubtitle {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let missed = self.missed;
        let phrase = match self.modal {
            PostModal::Shifts => tr!(
                en: "Sort every parcel right to earn ★.",
                zh: "全部送对，这一班得 ★。"
            ),
            PostModal::Summary if missed == 0 => {
                tr!(en: "All sorted right: ★ earned.", zh: "全部送对，得 ★。")
            }
            PostModal::Summary => {
                return match locale() {
                    Locale::ZhCn => write!(output, "错投 {missed} 件。全部送对才得 ★。"),
                    _ => write!(output, "{missed} sent astray. Sort all right for ★."),
                };
            }
            PostModal::Reset => tr!(
                en: "Sent parcels and score start over.",
                zh: "已送出的包裹和分数会清零。"
            ),
            PostModal::None => return Ok(()),
        };
        write!(output, "{phrase}")
    }
}

/// What a restart would throw away.
struct ShiftProgress {
    shift: u8,
    sent: u8,
    parcels: u8,
    score: u16,
}

impl fmt::Display for ShiftProgress {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            sent,
            parcels,
            score,
            ..
        } = *self;
        let name = SHIFTS[usize::from(self.shift)].name;
        match locale() {
            Locale::ZhCn => write!(output, "{name} · 已送 {sent} / {parcels} 件 · {score} 分"),
            _ => write!(output, "{name} · sent {sent}/{parcels} · {score} pts"),
        }
    }
}

/// The summary's best score, marked when this shift beat it.
struct BestLine {
    best: u16,
    new_best: bool,
}

impl fmt::Display for BestLine {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = if self.new_best {
            tr!(en: "New best", zh: "新高")
        } else {
            tr!(en: "Best", zh: "最高")
        };
        write!(output, "{label} {:04}", self.best)
    }
}

/// A count of parcels on the summary, right or astray.
struct ParcelCount {
    label: Phrase,
    count: u8,
}

impl fmt::Display for ParcelCount {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match locale() {
            Locale::ZhCn => write!(output, "{} {} 件", self.label, self.count),
            _ => write!(output, "{} {}", self.label, self.count),
        }
    }
}

/// The summary's way forward: the next shift, or the list after the last.
struct NextLabel(u8);

impl fmt::Display for NextLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match SHIFTS.get(usize::from(self.0) + 1) {
            Some(next) => write!(
                output,
                "{} {} →",
                tr!(en: "Next:", zh: "下一班 ·"),
                next.name
            ),
            None => write!(output, "{}", tr!(en: "☰ Shifts", zh: "☰ 选班次")),
        }
    }
}

fn modal_top(modal: PostModal) -> i32 {
    modal_frame(modal).y.to_int()
}

#[compose]
fn compose_header(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "POCKET POST", zh : "口袋邮局"),
            id: "post_title",
            position: Position::Absolute,
            left: 32,
            top: 6,
            width: 70,
            height: 20,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ ShiftLabel(model.shift()) },
            capacity: 24,
            id: "post_shift",
            position: Position::Absolute,
            left: 106,
            top: 6,
            width: 92,
            height: 20,
            font_size: 10,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                format_args!(
                    "{} {}/{}", tr!(en : "Sorted", zh : "送对"), model.delivered(), model
                    .parcel_count()
                )
            },
            capacity: 16,
            id: "post_sorted",
            position: Position::Absolute,
            left: 200,
            top: 6,
            width: 92,
            height: 20,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            text: ${ run_label(model.running()) },
            capacity: 4,
            id: "post_run",
            position: Position::Absolute,
            left: 300,
            top: 2,
            width: 36,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.running() || !model.started(), true) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(model.running() || !model.started(), true) },
            border_radius: 7
        ) on Tap { model.toggle_running(); }
    };
    ui! {
        Button (
            "＋",
            id: "post_send",
            position: Position::Absolute,
            left: 340,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.can_send()) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.can_send()) },
            border_radius: 7
        ) on Tap { model.spawn_manual(); }
    };
    ui! {
        Button (
            text: ${ speed_label(model.speed_x2()) },
            capacity: 8,
            id: "post_speed",
            position: Position::Absolute,
            left: 372,
            top: 2,
            width: 38,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.cycle_speed(); }
    };
    ui! {
        Button (
            "↻",
            id: "post_restart",
            position: Position::Absolute,
            left: 414,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.open_reset(); }
    };
    ui! {
        Button (
            "☰",
            id: "post_shifts",
            position: Position::Absolute,
            left: 446,
            top: 2,
            width: 28,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.open_shifts(); }
    }
}

#[compose]
fn station_labels(station: usize) -> Entity {
    let row = STATION_Y[station];
    ui! {
        Text (
            STATION_LETTERS[station],
            position: Position::Absolute,
            left: STATION_LEFT + 28,
            top: row - 18,
            width: 14,
            height: 14,
            font_size: 10,
            text_color: STATION_COLORS[station],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            STATIONS[station],
            position: Position::Absolute,
            left: STATION_LEFT + 28,
            top: row - 2,
            width: 64,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    }
}

#[compose]
fn compose_route_labels(model: model!(PostModel)) -> Entity {
    ui! {
        View (
            id: "post_route_labels",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            Text (
                tr!(en : "IN", zh : "入口"),
                position: Position::Absolute,
                left: 18,
                top: MAIN_Y - 18,
                width: 30,
                height: 13,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S1",
                position: Position::Absolute,
                left: SWITCH_X[0] - 14,
                top: MAIN_Y + 24,
                width: 28,
                height: 14,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            Text (
                "S2",
                position: Position::Absolute,
                left: SWITCH_X[1] - 14,
                top: MAIN_Y + 24,
                width: 28,
                height: 14,
                font_size: 10,
                text_color: ACCENT,
                paragraph: ParagraphStyle::label()
            )
            station_labels (0)
            station_labels (1)
            station_labels (2)
            Text (
                text: ${ format_args!("{}", model.arrivals() [0]) },
                capacity: 3,
                id: "post_station_0",
                position: Position::Absolute,
                left: STATION_LEFT + 66,
                top: STATION_Y[0] - 18,
                width: 24,
                height: 14,
                font_size: 10,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                text: ${ format_args!("{}", model.arrivals() [1]) },
                capacity: 3,
                id: "post_station_1",
                position: Position::Absolute,
                left: STATION_LEFT + 66,
                top: STATION_Y[1] - 18,
                width: 24,
                height: 14,
                font_size: 10,
                text_color: STATION_COLORS[1],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            Text (
                text: ${ format_args!("{}", model.arrivals() [2]) },
                capacity: 3,
                id: "post_station_2",
                position: Position::Absolute,
                left: STATION_LEFT + 66,
                top: STATION_Y[2] - 18,
                width: 24,
                height: 14,
                font_size: 10,
                text_color: STATION_COLORS[2],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
        }
    }
}

#[compose]
fn compose_foot(model: model!(PostModel)) -> Entity {
    let queue_label = style!(
        position: Position::Absolute, top: FOOT_TOP, width: 17, height: 14,
        paragraph: ParagraphStyle::label()
    );
    ui! {
        View (
            id: "post_foot",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            font_size: 10
        ) {
            Text (
                tr!(en : "Next", zh : "待发"),
                position: Position::Absolute,
                left: 14,
                top: FOOT_TOP,
                width: 34,
                height: 14,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${ destination_letter(model.queued_slots()[0]) },
                capacity: 1,
                id: "post_queue_0",
                visible: ${ model.queued_slots()[0].is_some() },
                style: queue_label.clone(),
                left: queue_slot_left(0) + 1,
                text_color: ${ destination_color(model.queued_slots()[0]) }
            )
            Text (
                text: ${ destination_letter(model.queued_slots()[1]) },
                capacity: 1,
                id: "post_queue_1",
                visible: ${ model.queued_slots()[1].is_some() },
                style: queue_label.clone(),
                left: queue_slot_left(1) + 1,
                text_color: ${ destination_color(model.queued_slots()[1]) }
            )
            Text (
                text: ${ destination_letter(model.queued_slots()[2]) },
                capacity: 1,
                id: "post_queue_2",
                visible: ${ model.queued_slots()[2].is_some() },
                style: queue_label.clone(),
                left: queue_slot_left(2) + 1,
                text_color: ${ destination_color(model.queued_slots()[2]) }
            )
            Text (
                text: ${ destination_letter(model.queued_slots()[3]) },
                capacity: 1,
                id: "post_queue_3",
                visible: ${ model.queued_slots()[3].is_some() },
                style: queue_label.clone(),
                left: queue_slot_left(3) + 1,
                text_color: ${ destination_color(model.queued_slots()[3]) }
            )
            Text (
                text: ${ destination_letter(model.queued_slots()[4]) },
                capacity: 1,
                id: "post_queue_4",
                visible: ${ model.queued_slots()[4].is_some() },
                style: queue_label,
                left: queue_slot_left(4) + 1,
                text_color: ${ destination_color(model.queued_slots()[4]) }
            )
            Text (
                text: ${
                    format_args!(
                        "{} {}/{}", tr!(en : "On rails", zh : "在途"), model.active_len(), model
                        .in_flight()
                    )
                },
                capacity: 16,
                id: "post_in_transit",
                position: Position::Absolute,
                left: 172,
                top: FOOT_TOP,
                width: 76,
                height: 14,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    StatusLine {
                        started: model.started(),
                        finished: model.finished(),
                        running: model.running(),
                        missed: model.missed(),
                        streak: model.streak(),
                    }
                },
                capacity: 48,
                id: "post_status",
                position: Position::Absolute,
                left: 252,
                top: FOOT_TOP,
                width: 214,
                height: 14,
                text_color: ${ if model.missed() > 0 && model.started() { MISS } else { ACCENT } },
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
        }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(PostModel)) {
    ui! {
        View (id: "post_surface", width: 480, height: 320, clip_children: true) [
            PostSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            View (
                id: "post_page",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                visible: ${ model.modal() == PostModal::None }
            ) {
                compose_header (model)
                compose_route_labels (model)
                compose_switches (model)
                compose_foot (model)
            }
            compose_modal (model)
        }
    };
}

/// The two switches are buttons over their discs on the board.
#[compose]
fn compose_switches(model: model!(PostModel)) -> Entity {
    ui! {
        View (
            id: "post_switches",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320
        ) {
            Button (
                "",
                id: "post_switch_0",
                position: Position::Absolute,
                left: SWITCH_X[0] - 23,
                top: MAIN_Y - 23,
                width: 46,
                height: 46,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(170, 208, 231, 56),
                border_radius: 23
            ) on Tap { model.toggle_switch(0); }
            Button (
                "",
                id: "post_switch_1",
                position: Position::Absolute,
                left: SWITCH_X[1] - 23,
                top: MAIN_Y - 23,
                width: 46,
                height: 46,
                size: ButtonSize::Custom,
                normal_color: Color::rgba(0, 0, 0, 0),
                pressed_color: Color::rgba(170, 208, 231, 56),
                border_radius: 23
            ) on Tap { model.toggle_switch(1); }
        }
    }
}

#[compose]
fn shift_tile(model: model!(PostModel), shift: u8, left: i32, top: i32) -> Entity {
    ui! {
        Button (
            text: ${
                ShiftTile {
                    shift,
                    starred: starred(model.starred(), shift),
                    best: model.best()[usize::from(shift)],
                }
            },
            capacity: 64,
            visible: ${ model.modal() == PostModal::Shifts },
            position: Position::Absolute,
            left: left,
            top: top,
            width: 134,
            height: 90,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.shift() == shift, true) },
            pressed_color: ACCENT,
            text_color: ${ tile_text_color(model.shift() == shift, starred(model.starred(), shift)) },
            border_radius: 7
        ) on Tap { model.load_shift(shift); }
    }
}

#[compose]
fn compose_shift_list(model: model!(PostModel)) -> Entity {
    ui! {
        View (
            id: "post_shift_list",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == PostModal::Shifts }
        ) {
            Text (
                text: ${ format_args!("★ {} / {SHIFT_COUNT}", model.starred().count_ones()) },
                capacity: 12,
                id: "post_shift_stars",
                position: Position::Absolute,
                left: 330,
                top: 42,
                width: 88,
                height: 18,
                font_size: 10,
                text_color: STATION_COLORS[0],
                paragraph: ParagraphStyle::label().with_align(TextAlign::End)
            )
            shift_tile (model, 0, 29, 82)
            shift_tile (model, 1, 172, 82)
            shift_tile (model, 2, 315, 82)
            shift_tile (model, 3, 29, 178)
            shift_tile (model, 4, 172, 178)
            shift_tile (model, 5, 315, 178)
            Text (
                tr!(
                    en : "Keys: A S switches · Space run · Enter next · R restart · Esc shifts",
                    zh :
                    "按键：A S 拨道岔 · 空格 开始 · 回车 下一班 · R 重来 · Esc 班次"
                ),
                id: "post_keys",
                position: Position::Absolute,
                left: 29,
                top: 274,
                width: 420,
                height: 14,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
        }
    }
}

#[compose]
fn compose_summary(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            text: ${ format_args!("{:04}", model.score()) },
            capacity: 5,
            id: "post_summary_score",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 42,
            top: 119,
            width: 175,
            height: 48,
            font_size: 35,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            tr!(en : "POINTS", zh : "得分"),
            id: "post_summary_points",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 44,
            top: 166,
            width: 130,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                BestLine {
                    best: model.current_best(),
                    new_best: model.new_best(),
                }
            },
            capacity: 24,
            id: "post_summary_best",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 44,
            top: 182,
            width: 170,
            height: 15,
            font_size: 10,
            text_color: ${ if model.new_best() { ACCENT } else { MUTED } },
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                ParcelCount {
                    label: tr!(en : "Right", zh : "正确"),
                    count: model.delivered(),
                }
            },
            capacity: 20,
            id: "post_summary_correct",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 262,
            top: 121,
            width: 134,
            height: 22,
            font_size: 20,
            text_color: STATION_COLORS[0],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${
                ParcelCount {
                    label: tr!(en : "Astray", zh : "错投"),
                    count: model.missed(),
                }
            },
            capacity: 20,
            id: "post_summary_missed",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 262,
            top: 152,
            width: 134,
            height: 16,
            font_size: 10,
            text_color: ${ if model.missed() > 0 { MISS } else { MUTED } },
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            id: "post_summary_again",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 29,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) [
            Text::label(tr!(en : "↻ Again", zh : "↻ 再来一次")),
        ] on Tap { model.restart(); }
    };
    ui! {
        Button (
            text: ${ NextLabel(model.shift()) },
            capacity: 32,
            id: "post_summary_next",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 245,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ACCENT,
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 7
        ) on Tap { model.next_shift(); }
    }
}

#[compose]
fn compose_reset(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            text: ${
                ShiftProgress {
                    shift: model.shift(),
                    sent: model.delivered() + model.missed(),
                    parcels: model.parcel_count(),
                    score: model.score(),
                }
            },
            capacity: 64,
            id: "post_reset_progress",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 29,
            top: 156,
            width: 420,
            height: 18,
            font_size: 10,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            id: "post_reset_cancel",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 29,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) [
            Text::label(tr!(en : "Keep going", zh : "继续这一班")),
        ] on Tap { model.close_modal(); }
    };
    ui! {
        Button (
            id: "post_reset_confirm",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 245,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ACCENT,
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 7
        ) [
            Text::label(tr!(en : "↻ Restart", zh : "↻ 重来")),
        ] on Tap { model.restart(); }
    }
}

#[compose]
fn compose_modal(model: model!(PostModel)) -> Entity {
    ui! {
        View (
            id: "post_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            clip_children: true,
            visible: ${ model.modal() != PostModal::None }
        ) [
            PostModalSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            Text (
                text: ${
                    ModalTitle {
                        modal: model.modal(),
                        shift: model.shift(),
                    }
                },
                capacity: 32,
                id: "post_modal_title",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 11 },
                width: 290,
                height: 22,
                font_size: 10,
                text_color: TEXT,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                text: ${
                    ModalSubtitle {
                        modal: model.modal(),
                        missed: model.missed(),
                    }
                },
                capacity: 64,
                id: "post_modal_subtitle",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 33 },
                width: 390,
                height: 15,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "×",
                id: "post_modal_close",
                position: Position::Absolute,
                left: 428,
                top: ${ modal_top(model.modal()) + 8 },
                width: 24,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { model.close_modal(); }
            compose_shift_list (model)
            compose_summary (model)
            compose_reset (model)
        } on Tap { }
    }
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    app.add_plugin(GameFontPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(PostModel::default()).finish();
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(post_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
