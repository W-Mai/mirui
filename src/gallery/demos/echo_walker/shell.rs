use super::input::{echo_tick_system, keyboard, surface_drag_move, surface_tap};
#[cfg(feature = "persistence")]
use super::persistence::install_persistence;
use super::render::{LEGEND_X, modal_render, surface_render};
use super::state::{EchoModalSurface, EchoSurface};
use super::style::{ACCENT, BACKGROUND, ECHO_COLORS, HEADER, LINE, MUTED, PANEL, RUST, TEXT};
#[cfg(feature = "std")]
use crate::app::plugins::StdInstantClockPlugin;
use crate::core::i18n::{Locale, Phrase, locale};
use crate::gallery::play::echo::{Direction, EchoMessage, EchoModal, EchoModel};
use crate::gallery::play::font::PlayFontPlugin;
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::tr;
use crate::ui::ComputedRect;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};
use core::fmt;

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn action_color(enabled: bool) -> Color {
    if enabled { PANEL } else { HEADER }
}

fn action_text_color(enabled: bool) -> Color {
    if enabled { TEXT } else { MUTED }
}

fn chinese() -> bool {
    locale() == Locale::ZhCn
}

/// `count` followed by the singular or plural English noun, or by the
/// Chinese one.
struct Count(u16, &'static str, &'static str, &'static str);

impl fmt::Display for Count {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(count, one, many, zh) = *self;
        if chinese() {
            write!(out, "{count} {zh}")
        } else {
            write!(out, "{count} {}", if count == 1 { one } else { many })
        }
    }
}

struct ModalTitle {
    modal: EchoModal,
    complete: bool,
    level: u8,
}

impl fmt::Display for ModalTitle {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let room = self.level + 1;
        match (self.modal, self.complete, chinese()) {
            (EchoModal::Menu, _, _) => out.write_str(tr!(en: "HOW TO PLAY", zh: "规则").get()),
            (_, true, _) => {
                out.write_str(tr!(en: "ALL 12 ROOMS SOLVED", zh: "12 间密室全部完成").get())
            }
            (_, false, true) => write!(out, "密室 {room:02} 完成"),
            (_, false, false) => write!(out, "ROOM {room:02} SOLVED"),
        }
    }
}

fn modal_subtitle(complete: bool) -> Phrase {
    if complete {
        tr!(en: "Totals for all 12 rooms.", zh: "12 间密室的总计。")
    } else {
        tr!(en: "Exit reached with every memory.", zh: "带着全部片段到达出口。")
    }
}

fn primary_label(result_len: u8) -> Phrase {
    if result_len == 11 {
        tr!(en: "FINISH", zh: "完成")
    } else {
        tr!(en: "NEXT ROOM ▶", zh: "下一间 ▶")
    }
}

struct MessageLabel(EchoMessage);

impl fmt::Display for MessageLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let zh = chinese();
        let phrase = match self.0 {
            EchoMessage::Ready => tr!(
                en: "Stand on a plate, then record an echo to hold it.",
                zh: "站上踏板，再录下回响替你压住。"
            ),
            EchoMessage::Plate(index) => {
                let plate = char::from(b'A' + index);
                return if zh {
                    write!(out, "踏板 {plate} 已压下 · 录下回响让它一直压着")
                } else {
                    write!(
                        out,
                        "Plate {plate} is down · record an echo to keep it there"
                    )
                };
            }
            EchoMessage::Fragment => tr!(
                en: "Memory found · you keep it after an echo or a retry",
                zh: "找到片段 · 录回响或重走后仍保留"
            ),
            EchoMessage::Missing(count) => {
                return if zh {
                    write!(out, "出口未开 · 还缺 {count} 个片段")
                } else {
                    write!(
                        out,
                        "Exit locked · {}",
                        Count(count.into(), "memory left", "memories left", "")
                    )
                };
            }
            EchoMessage::BeatLimit => tr!(
                en: "Out of beats · record an echo or retry",
                zh: "步数用完 · 录下回响或重走"
            ),
            EchoMessage::Recorded => tr!(
                en: "Echo recorded · it walks your route as you move",
                zh: "回响已录下 · 你走一步，它也重走一步"
            ),
            EchoMessage::Restarted => tr!(
                en: "Back to the start · echoes and memories kept",
                zh: "回到起点 · 回响和片段都保留"
            ),
            EchoMessage::Cleared => tr!(
                en: "Room reset · echoes and memories cleared",
                zh: "密室已重置 · 回响和片段已清空"
            ),
            EchoMessage::Undone => tr!(en: "Undone", zh: "已撤销"),
            EchoMessage::Won => tr!(en: "Exit reached · room solved", zh: "到达出口 · 密室完成"),
            EchoMessage::Complete => tr!(en: "All 12 rooms solved", zh: "12 间密室全部完成"),
            EchoMessage::DoorShut(index) => {
                let door = char::from(b'A' + index);
                return if zh {
                    write!(out, "门 {door} 关着 · 要有人站在踏板 {door} 上")
                } else {
                    write!(
                        out,
                        "Door {door} is shut · someone must stand on plate {door}"
                    )
                };
            }
            EchoMessage::ClockShut => tr!(
                en: "Timed door is shut · wait a beat or two",
                zh: "定时门关着 · 等一两步再走"
            ),
            EchoMessage::NoRoute => tr!(
                en: "Walk first · an echo replays the route you take",
                zh: "先走几步 · 回响会重走你走过的路线"
            ),
            EchoMessage::TapesFull => tr!(
                en: "All 3 echoes recorded · undo or reset the room",
                zh: "3 个回响都已录下 · 可以撤销或重置密室"
            ),
        };
        out.write_str(phrase.get())
    }
}

struct ResultStats {
    complete: bool,
    steps: u16,
    loops: u16,
}

impl fmt::Display for ResultStats {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.complete {
            write!(out, "{} ", tr!(en: "TOTAL", zh: "合计"))?;
        }
        write!(
            out,
            "{} · {}",
            Count(self.steps, "STEP", "STEPS", "步"),
            Count(self.loops, "ECHO", "ECHOES", "个回响")
        )
    }
}

struct RoomLabel(u8);

impl fmt::Display for RoomLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "{} {:02} / 12",
            tr!(en: "ROOM", zh: "密室"),
            self.0 + 1
        )
    }
}

struct TapeLabel {
    beats: u8,
    solo: bool,
}

impl fmt::Display for TapeLabel {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            out,
            "{} {}",
            if self.solo { "◉" } else { "●" },
            Count(self.beats.into(), "BEAT", "BEATS", "步")
        )
    }
}

fn tape_fill(solo: bool) -> Color {
    if solo { LINE } else { HEADER }
}

#[compose]
fn compose_header(model: model!(EchoModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "ECHO WALKER", zh : "回响行者"),
            position: Position::Absolute,
            left: 12,
            top: 7,
            width: 104,
            height: 14,
            font_size: 10,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ RoomLabel(model.level()) },
            capacity: 16,
            id: "echo_room",
            position: Position::Absolute,
            left: 124,
            top: 7,
            width: 100,
            height: 14,
            font_size: 10,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${
                format_args!(
                    "{} {}/{}", tr!(en : "MEMORY", zh : "片段"), model.collected_count(), model
                    .gem_count()
                )
            },
            capacity: 16,
            id: "echo_memory",
            position: Position::Absolute,
            left: 330,
            top: 7,
            width: 142,
            height: 14,
            font_size: 10,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose]
fn compose_controls(model: model!(EchoModel)) -> Entity {
    ui! {
        Text (
            tr!(en : "BEAT", zh : "时间"),
            position: Position::Absolute,
            left: 392,
            top: 36,
            width: 80,
            height: 12,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{:02}", model.tick()) },
            capacity: 2,
            id: "echo_tick",
            position: Position::Absolute,
            left: 392,
            top: 50,
            width: 30,
            height: 24,
            font_size: 20,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            tr!(en : "/ 48", zh : "/ 48 步"),
            position: Position::Absolute,
            left: 424,
            top: 60,
            width: 48,
            height: 12,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Button (
            tr!(en : "● REC ECHO", zh : "● 录下回响"),
            id: "echo_rewind",
            position: Position::Absolute,
            left: 392,
            top: 82,
            width: 80,
            height: 30,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ action_color(model.can_rewind()) },
            pressed_color: RUST,
            text_color: ${ if model.can_rewind() { ACCENT } else { MUTED } },
            border_radius: 4
        ) on Tap { model.rewind(); }
    };
    ui! {
        Button (
            tr!(en : "· WAIT", zh : "· 原地等"),
            id: "echo_wait",
            position: Position::Absolute,
            left: 392,
            top: 116,
            width: 80,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Wait); }
    };
    ui! {
        Button (
            "↺",
            id: "echo_undo",
            position: Position::Absolute,
            left: 392,
            top: 148,
            width: 25,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ action_color(model.can_undo()) },
            pressed_color: LINE,
            text_color: ${ action_text_color(model.can_undo()) },
            border_radius: 4
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "↻",
            id: "echo_restart",
            position: Position::Absolute,
            left: 419,
            top: 148,
            width: 26,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ action_color(model.can_restart()) },
            pressed_color: LINE,
            text_color: ${ action_text_color(model.can_restart()) },
            border_radius: 4
        ) on Tap { model.restart(); }
    };
    ui! {
        Button (
            "?",
            id: "echo_menu",
            position: Position::Absolute,
            left: 447,
            top: 148,
            width: 25,
            height: 28,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.set_modal(EchoModal::Menu); }
    }
}

#[compose]
fn compose_tapes(model: model!(EchoModel)) -> Entity {
    ui! {
        Text (
            text: ${ format_args!("{} {}/3", tr!(en : "ECHOES", zh : "回响"), model.ghost_count()) },
            capacity: 16,
            id: "echo_ghosts",
            position: Position::Absolute,
            left: 392,
            top: 188,
            width: 80,
            height: 12,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            tr!(en : "None yet", zh : "还没有"),
            id: "echo_tapes_empty",
            position: Position::Absolute,
            left: 392,
            top: 208,
            width: 80,
            height: 12,
            font_size: 10,
            text_color: LINE,
            paragraph: label(),
            visible: ${ model.ghost_count() == 0 }
        )
    };
    ui! {
        Button (
            text: ${
                TapeLabel {
                    beats: model.ghost_route_lengths()[0],
                    solo: model.peek_ghost() == Some(0),
                }
            },
            capacity: 16,
            id: "echo_tape_0",
            position: Position::Absolute,
            left: 392,
            top: 204,
            width: 80,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tape_fill(model.peek_ghost() == Some(0)) },
            pressed_color: LINE,
            text_color: ECHO_COLORS[0],
            border_radius: 4,
            visible: ${ model.ghost_count() > 0 }
        ) on Tap { model.toggle_peek_ghost(0); }
    };
    ui! {
        Button (
            text: ${
                TapeLabel {
                    beats: model.ghost_route_lengths()[1],
                    solo: model.peek_ghost() == Some(1),
                }
            },
            capacity: 16,
            id: "echo_tape_1",
            position: Position::Absolute,
            left: 392,
            top: 234,
            width: 80,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tape_fill(model.peek_ghost() == Some(1)) },
            pressed_color: LINE,
            text_color: ECHO_COLORS[1],
            border_radius: 4,
            visible: ${ model.ghost_count() > 1 }
        ) on Tap { model.toggle_peek_ghost(1); }
    };
    ui! {
        Button (
            text: ${
                TapeLabel {
                    beats: model.ghost_route_lengths()[2],
                    solo: model.peek_ghost() == Some(2),
                }
            },
            capacity: 16,
            id: "echo_tape_2",
            position: Position::Absolute,
            left: 392,
            top: 264,
            width: 80,
            height: 26,
            size: ButtonSize::Compact,
            font_size: 10,
            normal_color: ${ tape_fill(model.peek_ghost() == Some(2)) },
            pressed_color: LINE,
            text_color: ECHO_COLORS[2],
            border_radius: 4,
            visible: ${ model.ghost_count() > 2 }
        ) on Tap { model.toggle_peek_ghost(2); }
    }
}

#[compose]
fn compose_status(model: model!(EchoModel)) -> Entity {
    ui! {
        Text (
            text: ${ MessageLabel(model.message()) },
            capacity: 96,
            id: "echo_message",
            position: Position::Absolute,
            left: 12,
            top: 306,
            width: 370,
            height: 13,
            font_size: 10,
            text_color: MUTED,
            paragraph: label()
        )
    }
}

#[compose]
fn compose_menu(model: model!(EchoModel)) -> Entity {
    ui! {
        View (
            id: "echo_menu_body",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            visible: ${ model.modal() == EchoModal::Menu }
        ) {
            Text (
                tr!(en : "PLATE", zh : "踏板"),
                position: Position::Absolute,
                left: LEGEND_X[0],
                top: 112,
                width: 56,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                tr!(en : "DOOR", zh : "门"),
                position: Position::Absolute,
                left: LEGEND_X[1],
                top: 112,
                width: 56,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                tr!(en : "TIMED", zh : "定时门"),
                position: Position::Absolute,
                left: LEGEND_X[2],
                top: 112,
                width: 56,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                tr!(en : "MEMORY", zh : "片段"),
                position: Position::Absolute,
                left: LEGEND_X[3],
                top: 112,
                width: 56,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                tr!(en : "EXIT", zh : "出口"),
                position: Position::Absolute,
                left: LEGEND_X[4],
                top: 112,
                width: 56,
                height: 12,
                font_size: 10,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
            Text (
                tr!(
                    en : "A door stays open while someone stands on its plate.", zh :
                    "有人站在踏板上时，对应的门才会打开。"
                ),
                position: Position::Absolute,
                left: 52,
                top: 136,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "REC ECHO sends you back to the start; your route replays.", zh :
                    "录下回响：你回到起点，回响重走这段路线。"
                ),
                position: Position::Absolute,
                left: 52,
                top: 152,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Pick up every memory, then reach the exit. 48 beats a run.", zh :
                    "收集所有片段，再走到出口。每次最多 48 步。"
                ),
                position: Position::Absolute,
                left: 52,
                top: 168,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Timed doors open for 2 beats, then shut for 2.", zh :
                    "定时门开 2 步，关 2 步。"
                ),
                position: Position::Absolute,
                left: 52,
                top: 184,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Swipe or tap beside you to move; tap yourself to wait.", zh :
                    "滑动或点相邻格移动，点自己原地等一步。"
                ),
                position: Position::Absolute,
                left: 52,
                top: 206,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: MUTED,
                paragraph: label()
            )
            Text (
                tr!(
                    en : "Keys: arrows/WASD · Space wait · E echo · Z undo · R retry", zh :
                    "按键：方向键/WASD · 空格 等待 · E 回响 · Z 撤销 · R 重走"
                ),
                position: Position::Absolute,
                left: 52,
                top: 222,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: MUTED,
                paragraph: label()
            )
            Button (
                tr!(en : "RESET ROOM", zh : "重置密室"),
                id: "echo_modal_secondary",
                position: Position::Absolute,
                left: 52,
                top: 250,
                width: 132,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { model.clear_room(); }
        }
    }
}

#[compose]
fn compose_modal(model: model!(EchoModel)) -> Entity {
    ui! {
        View (
            id: "echo_modal",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 320,
            clip_children: true,
            visible: ${ model.modal() != EchoModal::None }
        ) [
            EchoModalSurface {
                model: model.clone(),
            },
        ] {
            Text (
                text: ${
                    ModalTitle {
                        modal: model.modal(),
                        complete: model.complete(),
                        level: model.level(),
                    }
                },
                capacity: 32,
                id: "echo_modal_title",
                position: Position::Absolute,
                left: 52,
                top: 48,
                width: 340,
                height: 24,
                font_size: 20,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                text: ${ modal_subtitle(model.complete()) },
                capacity: 64,
                id: "echo_modal_subtitle",
                position: Position::Absolute,
                left: 52,
                top: 82,
                width: 376,
                height: 13,
                font_size: 10,
                text_color: MUTED,
                paragraph: label(),
                visible: ${ model.modal() == EchoModal::Result }
            )
            Button (
                "×",
                id: "echo_modal_close",
                position: Position::Absolute,
                left: 412,
                top: 46,
                width: 26,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { model.set_modal(EchoModal::None); }
            Text (
                text: ${
                    ResultStats {
                        complete: model.complete(),
                        steps: if model.complete() { model.total_steps() } else { model.steps() },
                        loops: if model.complete() {
                            model.total_loops()
                        } else {
                            u16::from(model.loops())
                        },
                    }
                },
                capacity: 48,
                id: "echo_result_stats",
                position: Position::Absolute,
                left: 52,
                top: 122,
                width: 376,
                height: 24,
                font_size: 20,
                text_color: ACCENT,
                paragraph: label(),
                visible: ${ model.modal() == EchoModal::Result }
            )
            Button (
                text: ${ primary_label(model.result_len()) },
                capacity: 16,
                id: "echo_modal_primary",
                position: Position::Absolute,
                left: 52,
                top: 246,
                width: 220,
                height: 34,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: RUST,
                text_color: BACKGROUND,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Result && !model.complete() }
            ) on Tap { model.continue_archive(); }
            compose_menu (model)
        }
    }
}

#[compose]
fn build_widgets(model: model!(EchoModel)) {
    ui! {
        View (id: "echo_surface", width: 480, height: 320, clip_children: true) [
            EchoSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] {
            compose_header (model)
            compose_controls (model)
            compose_tapes (model)
            compose_status (model)
            compose_modal (model)
        } on Tap(|event: event!(Tap), rows: query!(EchoSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_tap(surface, rect, event.x, event.y)
        }) on DragStart(|event: event!(DragStart), rows: query!(EchoSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface.model.begin_swipe();
            true
        }) on DragMove(|event: event!(DragMove), rows: query!(EchoSurface, ComputedRect)| {
            let Some((surface, rect)) = rows.get(event.target) else {
                return false;
            };
            let rect = rect.0;
            surface_drag_move(surface, rect, event.dx, event.dy)
        }) on DragEnd(|event: event!(DragEnd), rows: query!(EchoSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface.model.end_swipe();
            true
        }) on DragCancel(|event: event!(DragCancel), rows: query!(EchoSurface)| {
            let Some(surface) = rows.get(event.target) else {
                return false;
            };
            surface.model.end_swipe();
            true
        })
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    app.add_plugin(PlayFontPlugin);
    #[cfg(feature = "std")]
    app.add_plugin(StdInstantClockPlugin);
    #[cfg(feature = "persistence")]
    install_persistence(app);
    let model = app.add_model(EchoModel::default()).finish();
    app.with_widget(surface_render::view())
        .with_widget(modal_render::view());
    app.on_event(Key::EVENT, keyboard::callback(model.clone()));
    app.add_system(echo_tick_system::system(model.clone()));
    app.compose(parent, build_widgets(model));
}

#[cfg(test)]
mod tests {
    use super::{Count, EchoMessage, MessageLabel, ResultStats};
    use alloc::format;

    #[test]
    fn counts_use_the_singular_for_one() {
        let count = |value| format!("{}", Count(value, "STEP", "STEPS", "步"));
        assert_eq!(count(1), "1 STEP");
        assert_eq!(count(2), "2 STEPS");
        let stats = format!(
            "{}",
            ResultStats {
                complete: false,
                steps: 1,
                loops: 2
            }
        );
        assert_eq!(stats, "1 STEP · 2 ECHOES");
        let missing = |count| format!("{}", MessageLabel(EchoMessage::Missing(count)));
        assert_eq!(missing(1), "Exit locked · 1 memory left");
        assert_eq!(missing(3), "Exit locked · 3 memories left");
    }
}
