use super::input::{keyboard, post_tick_system, surface_tap};
use super::render::{modal_frame, modal_render, surface_render};
use super::state::{PostModalSurface, PostSurface};
use super::style::{
    ACCENT, BACKGROUND, BOARD, CONTROL, CONTROL_DISABLED, MUTED, STATION_COLORS, TEXT,
};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::gallery::play::font::PlayFontPlugin;
use crate::gallery::play::post::{PostModal, PostModel, SHIFT_COUNT, SHIFTS};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn status_label(finished: bool, running: bool, started: bool) -> &'static str {
    if finished {
        "这一班送完了"
    } else if running {
        "投递中"
    } else if started {
        "已暂停"
    } else {
        "按 ▶ 开始，点 S1 S2 拨道岔"
    }
}

fn run_label(finished: bool, running: bool, started: bool) -> &'static str {
    if running {
        "Ⅱ 暂停"
    } else if finished {
        "↻ 再来一次"
    } else if started {
        "▶ 继续"
    } else {
        "▶ 开始"
    }
}

fn speed_label(speed_x2: u8) -> &'static str {
    match speed_x2 {
        1 => "0.5× 速度",
        3 => "1.5× 速度",
        _ => "1× 速度",
    }
}

fn destination_letter(destination: Option<u8>) -> &'static str {
    match destination {
        Some(0) => "A",
        Some(1) => "B",
        Some(_) => "C",
        None => "",
    }
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
        writeln!(output, "{}", ShiftLabel(self.shift))?;
        writeln!(
            output,
            "{} 件 · 每 {}.{} 秒",
            shift.parcels.len(),
            tenths / 10,
            tenths % 10
        )?;
        let star = if self.starred { "★" } else { "☆" };
        if self.best == 0 {
            output.write_str(star)
        } else {
            write!(output, "{star} 最高 {}", self.best)
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
            PostModal::Shifts => output.write_str("选择班次"),
            PostModal::Summary => write!(output, "{} 完成", SHIFTS[usize::from(self.shift)].name),
            PostModal::Reset => output.write_str("重来这一班？"),
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
        match self.modal {
            PostModal::Shifts => output.write_str("全部送对，这一班得 ★。"),
            PostModal::Summary if self.missed == 0 => output.write_str("全部送对，得 ★。"),
            PostModal::Summary => write!(output, "错投 {} 件。全部送对才得 ★。", self.missed),
            PostModal::Reset => output.write_str("已送出的包裹和分数会清零。"),
            PostModal::None => Ok(()),
        }
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
        write!(
            output,
            "{} · 已送 {} / {} 件 · {} 分",
            SHIFTS[usize::from(self.shift)].name,
            self.sent,
            self.parcels,
            self.score
        )
    }
}

/// The summary's way forward: the next shift, or the list after the last.
struct NextLabel(u8);

impl fmt::Display for NextLabel {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match SHIFTS.get(usize::from(self.0) + 1) {
            Some(next) => write!(output, "下一班 · {} →", next.name),
            None => output.write_str("选班次"),
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
            "POCKET POST",
            position: Position::Absolute,
            left: 35,
            top: 7,
            width: 110,
            height: 22,
            font_size: 14,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ ShiftLabel(model.shift()) },
            capacity: 16,
            id: "post_shift",
            position: Position::Absolute,
            left: 150,
            top: 9,
            width: 140,
            height: 18,
            font_size: 9,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("SORTED {} / {}", model.delivered(), model.parcel_count()) },
            capacity: 14,
            id: "post_sorted",
            position: Position::Absolute,
            left: 306,
            top: 9,
            width: 158,
            height: 18,
            font_size: 9,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ status_label(model.finished(), model.running(), model.started()) },
            capacity: 39,
            id: "post_status",
            position: Position::Absolute,
            left: 16,
            top: 42,
            width: 282,
            height: 17,
            font_size: 10,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("错投 {}   连对 {}", model.missed(), model.streak()) },
            capacity: 23,
            id: "post_misses",
            position: Position::Absolute,
            left: 304,
            top: 43,
            width: 160,
            height: 15,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose]
fn compose_route_summary(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            "IN",
            position: Position::Absolute,
            left: 19,
            top: 142,
            width: 28,
            height: 13,
            font_size: 8,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            "S1",
            position: Position::Absolute,
            left: 137,
            top: 181,
            width: 28,
            height: 14,
            font_size: 8,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            "S2",
            position: Position::Absolute,
            left: 251,
            top: 181,
            width: 28,
            height: 14,
            font_size: 8,
            text_color: ACCENT,
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            "A",
            position: Position::Absolute,
            left: 393,
            top: 75,
            width: 18,
            height: 14,
            font_size: 9,
            text_color: STATION_COLORS[0],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            "薄荷港",
            position: Position::Absolute,
            left: 393,
            top: 90,
            width: 48,
            height: 12,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_a_count()) },
            capacity: 3,
            id: "post_station_0",
            position: Position::Absolute,
            left: 432,
            top: 80,
            width: 15,
            height: 18,
            font_size: 13,
            text_color: STATION_COLORS[0],
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            "B",
            position: Position::Absolute,
            left: 393,
            top: 143,
            width: 18,
            height: 14,
            font_size: 9,
            text_color: STATION_COLORS[1],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            "紫藤站",
            position: Position::Absolute,
            left: 393,
            top: 158,
            width: 48,
            height: 12,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_b_count()) },
            capacity: 3,
            id: "post_station_1",
            position: Position::Absolute,
            left: 432,
            top: 148,
            width: 15,
            height: 18,
            font_size: 13,
            text_color: STATION_COLORS[1],
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            "C",
            position: Position::Absolute,
            left: 393,
            top: 211,
            width: 18,
            height: 14,
            font_size: 9,
            text_color: STATION_COLORS[2],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            "日落湾",
            position: Position::Absolute,
            left: 393,
            top: 226,
            width: 48,
            height: 12,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{}", model.station_c_count()) },
            capacity: 3,
            id: "post_station_2",
            position: Position::Absolute,
            left: 432,
            top: 216,
            width: 15,
            height: 18,
            font_size: 13,
            text_color: STATION_COLORS[2],
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose]
fn compose_dispatch_queue(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            "待发",
            position: Position::Absolute,
            left: 23,
            top: 243,
            width: 28,
            height: 13,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[0]) },
            capacity: 1,
            id: "post_queue_0",
            visible: ${ model.queued_slots()[0].is_some() },
            position: Position::Absolute,
            left: 47,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[0]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[1]) },
            capacity: 1,
            id: "post_queue_1",
            visible: ${ model.queued_slots()[1].is_some() },
            position: Position::Absolute,
            left: 72,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[1]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[2]) },
            capacity: 1,
            id: "post_queue_2",
            visible: ${ model.queued_slots()[2].is_some() },
            position: Position::Absolute,
            left: 97,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[2]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[3]) },
            capacity: 1,
            id: "post_queue_3",
            visible: ${ model.queued_slots()[3].is_some() },
            position: Position::Absolute,
            left: 122,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[3]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ destination_letter(model.queued_slots()[4]) },
            capacity: 1,
            id: "post_queue_4",
            visible: ${ model.queued_slots()[4].is_some() },
            position: Position::Absolute,
            left: 147,
            top: 241,
            width: 17,
            height: 15,
            font_size: 8,
            text_color: ${ destination_color(model.queued_slots()[4]) },
            paragraph: ParagraphStyle::label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} / {} 在途", model.active_len(), model.in_flight()) },
            capacity: 12,
            id: "post_in_transit",
            position: Position::Absolute,
            left: 218,
            top: 243,
            width: 86,
            height: 13,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Button (
            "班次",
            position: Position::Absolute,
            left: 387,
            top: 248,
            width: 66,
            height: 20,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: Color::rgb(49, 72, 76),
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 6
        ) on Tap { model.open_shifts(); }
    }
}

#[compose]
fn compose_footer(model: model!(PostModel)) -> Entity {
    ui! {
        Button (
            text: ${ run_label(model.finished(), model.running(), model.started()) },
            capacity: 20,
            id: "post_run",
            position: Position::Absolute,
            left: 12,
            top: 288,
            width: 129,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(model.running() || !model.started(), model.modal() == PostModal::None) },
            pressed_color: ACCENT,
            text_color: ${
                control_text_color(
                    model.running() || !model.started(),
                    model.modal() == PostModal::None,
                )
            },
            border_radius: 7
        ) on Tap { model.toggle_running(); }
    };
    ui! {
        Button (
            "加发一件",
            id: "post_send",
            position: Position::Absolute,
            left: 148,
            top: 288,
            width: 113,
            height: 26,
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
            capacity: 12,
            id: "post_speed",
            position: Position::Absolute,
            left: 268,
            top: 288,
            width: 95,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ control_color(false, model.modal() == PostModal::None) },
            pressed_color: ACCENT,
            text_color: ${ control_text_color(false, model.modal() == PostModal::None) },
            border_radius: 7
        ) on Tap { model.cycle_speed(); }
    };
    ui! {
        Button (
            "↻ 重来",
            position: Position::Absolute,
            left: 370,
            top: 288,
            width: 98,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.open_reset(); }
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
                compose_route_summary (model)
                compose_dispatch_queue (model)
                compose_footer (model)
            }
            compose_modal (model)
        } on Tap(|event: event!(Tap), rows: query!(PostSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        })
    };
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
            height: 92,
            size: ButtonSize::Compact,
            font_size: 11,
            normal_color: ${ control_color(model.shift() == shift, true) },
            pressed_color: ACCENT,
            text_color: ${ tile_text_color(model.shift() == shift, starred(model.starred(), shift)) },
            border_radius: 7
        ) on Tap { model.load_shift(shift); }
    }
}

#[compose]
fn compose_shift_stars(model: model!(PostModel)) -> Entity {
    ui! {
        Text (
            text: ${ format_args!("★ {} / {SHIFT_COUNT}", model.starred().count_ones()) },
            capacity: 10,
            id: "post_shift_stars",
            visible: ${ model.modal() == PostModal::Shifts },
            position: Position::Absolute,
            left: 330,
            top: 46,
            width: 88,
            height: 18,
            font_size: 10,
            text_color: STATION_COLORS[0],
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
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
            "POST POINTS",
            id: "post_summary_points",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 44,
            top: 166,
            width: 130,
            height: 13,
            font_size: 8,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("最高 {:04}", model.current_best()) },
            capacity: 12,
            id: "post_summary_best",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 44,
            top: 182,
            width: 130,
            height: 15,
            font_size: 9,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("正确 {} 件", model.delivered()) },
            capacity: 14,
            id: "post_summary_correct",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 270,
            top: 121,
            width: 150,
            height: 22,
            font_size: 20,
            text_color: STATION_COLORS[0],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Text (
            text: ${ format_args!("错投 {} 件", model.missed()) },
            capacity: 14,
            id: "post_summary_missed",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 270,
            top: 150,
            width: 150,
            height: 20,
            font_size: 12,
            text_color: STATION_COLORS[2],
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            "↻ 再来一次",
            id: "post_summary_again",
            visible: ${ model.modal() == PostModal::Summary },
            position: Position::Absolute,
            left: 29,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 11,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.restart(); }
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
            font_size: 11,
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
            capacity: 48,
            id: "post_reset_progress",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 29,
            top: 156,
            width: 420,
            height: 18,
            font_size: 11,
            text_color: TEXT,
            paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
        )
    };
    ui! {
        Button (
            "继续这一班",
            id: "post_reset_cancel",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 29,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 11,
            normal_color: CONTROL,
            pressed_color: ACCENT,
            pressed_text_color: BOARD,
            text_color: TEXT,
            border_radius: 7
        ) on Tap { model.close_modal(); }
    };
    ui! {
        Button (
            "↻ 重来",
            id: "post_reset_confirm",
            visible: ${ model.modal() == PostModal::Reset },
            position: Position::Absolute,
            left: 245,
            top: 217,
            width: 204,
            height: 34,
            size: ButtonSize::Compact,
            font_size: 11,
            normal_color: ACCENT,
            pressed_color: ACCENT,
            text_color: BACKGROUND,
            border_radius: 7
        ) on Tap { model.restart(); }
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
                capacity: 24,
                id: "post_modal_title",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 11 },
                width: 290,
                height: 22,
                font_size: 13,
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
                capacity: 48,
                id: "post_modal_subtitle",
                position: Position::Absolute,
                left: 29,
                top: ${ modal_top(model.modal()) + 31 },
                width: 390,
                height: 15,
                font_size: 8,
                text_color: MUTED,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Button (
                "×",
                position: Position::Absolute,
                left: 428,
                top: ${ modal_top(model.modal()) + 8 },
                width: 24,
                height: 22,
                size: ButtonSize::Compact,
                font_size: 12,
                normal_color: CONTROL,
                pressed_color: ACCENT,
                pressed_text_color: BOARD,
                text_color: TEXT,
                border_radius: 6
            ) on Tap { model.close_modal(); }
            compose_shift_stars (model)
            shift_tile (model, 0, 29, 90)
            shift_tile (model, 1, 172, 90)
            shift_tile (model, 2, 315, 90)
            shift_tile (model, 3, 29, 189)
            shift_tile (model, 4, 172, 189)
            shift_tile (model, 5, 315, 189)
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
    app.add_plugin(PlayFontPlugin);
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    let model = app.add_model(PostModel::default()).finish();
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(post_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}
