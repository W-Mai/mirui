use super::input::{EchoKeyboardPlugin, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{EchoModalSurface, EchoSurface};
use super::style::{ACCENT, BACKGROUND, HEADER, LINE, MUTED, PANEL, RUST, TEXT};
#[cfg(feature = "persistence")]
use crate::gallery::play::echo::EchoModelHandle;
use crate::gallery::play::echo::{Direction, EchoMessage, EchoModal, EchoModel};
use crate::gallery::play::font::register_play_font;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}

fn action_color(enabled: bool) -> Color {
    if enabled { PANEL } else { HEADER }
}

fn action_text_color(enabled: bool) -> Color {
    if enabled { TEXT } else { MUTED }
}

fn modal_title(modal: EchoModal, complete: bool) -> &'static str {
    if modal == EchoModal::Tapes {
        "ECHO TAPES"
    } else if complete {
        "TWELVE ROOMS RESTORED"
    } else {
        "ARCHIVE RESTORED"
    }
}

fn modal_subtitle(modal: EchoModal, complete: bool) -> &'static str {
    if modal == EchoModal::Tapes {
        "Inspect each route. An echo holds its final cell."
    } else if complete {
        "All twelve rooms are restored."
    } else {
        "Past and present completed the route together."
    }
}

fn primary_label(complete: bool, result_len: u8) -> &'static str {
    if complete {
        "ARCHIVE COMPLETE"
    } else if result_len == 11 {
        "COMPLETE ARCHIVE"
    } else {
        "NEXT ROOM"
    }
}

struct MessageLabel(EchoMessage);

impl core::fmt::Display for MessageLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            EchoMessage::Ready => {
                formatter.write_str("Reach a plate, rewind, and let the past replay the route.")
            }
            EchoMessage::Plate(index) => write!(
                formatter,
                "PLATE {} HELD · RECORD THIS ROUTE",
                char::from(b'A' + index)
            ),
            EchoMessage::Fragment => formatter.write_str("MEMORY FRAGMENT STORED ACROSS REWINDS"),
            EchoMessage::Missing(count) => {
                write!(formatter, "EXIT NEEDS {count} MORE MEMORY FRAGMENTS")
            }
            EchoMessage::BeatLimit => {
                formatter.write_str("48 BEATS USED · RECORD OR RESTART THIS ROUTE")
            }
            EchoMessage::Recorded => {
                formatter.write_str("ECHO RECORDED · IT WILL HOLD THE FINAL CELL")
            }
            EchoMessage::Restarted => {
                formatter.write_str("ROUTE RESTARTED · ECHOES AND MEMORY REMAIN")
            }
            EchoMessage::Cleared => formatter.write_str("ROOM RESET · SEED AND GEOMETRY PRESERVED"),
            EchoMessage::Undone => formatter.write_str("UNDO RESTORED TIME, MEMORY, AND ECHOES"),
            EchoMessage::Won => formatter.write_str("ARCHIVE RESTORED · CONTINUE TO THE NEXT ROOM"),
            EchoMessage::Complete => formatter.write_str("ALL TWELVE ROOMS RESTORED"),
        }
    }
}

struct ResultStats {
    complete: bool,
    steps: u16,
    loops: u16,
}

impl core::fmt::Display for ResultStats {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.complete {
            write!(
                formatter,
                "TOTAL {} STEPS · {} REWINDS",
                self.steps, self.loops
            )
        } else {
            write!(
                formatter,
                "ROOM {} STEPS · {} REWINDS",
                self.steps, self.loops
            )
        }
    }
}

struct TapeLabel {
    index: usize,
    beats: u8,
    solo: bool,
}

impl core::fmt::Display for TapeLabel {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "ECHO {} · {:02} BEATS{}",
            self.index + 1,
            self.beats,
            if self.solo { " · SOLO" } else { "" }
        )
    }
}

#[compose(bind(model))]
fn compose_header(model: EchoModel) -> Entity {
    ui! {
        Text (
            "ECHO WALKER",
            position: Position::Absolute,
            left: 14,
            top: 7,
            width: 120,
            height: 22,
            font_size: 15,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            "ECHO WALKER / 002718",
            position: Position::Absolute,
            left: 123,
            top: 10,
            width: 190,
            height: 16,
            font_size: 8,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("ARCHIVE {:02} / 12", model.level() + 1) },
            text_capacity: 15,
            id: "echo_room",
            position: Position::Absolute,
            left: 14,
            top: 39,
            width: 110,
            height: 16,
            font_size: 9,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("MEMORY {}/{}", model.collected_count(), model.gem_count()) },
            text_capacity: 10,
            id: "echo_memory",
            position: Position::Absolute,
            left: 146,
            top: 39,
            width: 90,
            height: 16,
            font_size: 9,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("ECHOES {}/3", model.ghost_count()) },
            text_capacity: 10,
            id: "echo_ghosts",
            position: Position::Absolute,
            left: 220,
            top: 39,
            width: 70,
            height: 16,
            font_size: 9,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    }
}

#[compose(bind(model))]
fn compose_controls(model: EchoModel) -> Entity {
    ui! {
        Text (
            "THE PRESENT",
            position: Position::Absolute,
            left: 305,
            top: 66,
            width: 150,
            height: 12,
            font_size: 8,
            text_color: ACCENT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{:02}", model.tick()) },
            text_capacity: 2,
            id: "echo_tick",
            position: Position::Absolute,
            left: 305,
            top: 82,
            width: 48,
            height: 38,
            font_size: 29,
            text_color: TEXT,
            paragraph: label()
        )
    };
    ui! {
        Text (
            "/ 48 BEATS",
            position: Position::Absolute,
            left: 351,
            top: 99,
            width: 90,
            height: 14,
            font_size: 9,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            "PAST MOVES ON EVERY BEAT",
            position: Position::Absolute,
            left: 305,
            top: 119,
            width: 161,
            height: 14,
            font_size: 7,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Button (
            "↑",
            position: Position::Absolute,
            left: 354,
            top: 133,
            width: 29,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 15,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Up); }
    };
    ui! {
        Button (
            "←",
            position: Position::Absolute,
            left: 321,
            top: 166,
            width: 29,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 15,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Left); }
    };
    ui! {
        Button (
            "·",
            position: Position::Absolute,
            left: 354,
            top: 166,
            width: 29,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 14,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Wait); }
    };
    ui! {
        Button (
            "→",
            position: Position::Absolute,
            left: 387,
            top: 166,
            width: 29,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 15,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Right); }
    };
    ui! {
        Button (
            "↓",
            position: Position::Absolute,
            left: 354,
            top: 199,
            width: 29,
            height: 29,
            size: ButtonSize::Compact,
            font_size: 15,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.step(Direction::Down); }
    };
    ui! {
        Button (
            "RECORD ECHO",
            id: "echo_rewind",
            position: Position::Absolute,
            left: 305,
            top: 236,
            width: 161,
            height: 27,
            size: ButtonSize::Compact,
            font_size: 9,
            normal_color: ${ action_color(model.can_rewind()) },
            pressed_color: RUST,
            text_color: ${ action_text_color(model.can_rewind()) },
            border_radius: 4
        ) on Tap { model.rewind(); }
    };
    ui! {
        Button (
            "UNDO",
            id: "echo_undo",
            position: Position::Absolute,
            left: 305,
            top: 270,
            width: 48,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 7,
            normal_color: ${ action_color(model.can_undo()) },
            pressed_color: LINE,
            text_color: ${ action_text_color(model.can_undo()) },
            border_radius: 4
        ) on Tap { model.undo(); }
    };
    ui! {
        Button (
            "RETRY",
            id: "echo_restart",
            position: Position::Absolute,
            left: 360,
            top: 270,
            width: 49,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 7,
            normal_color: ${ action_color(model.can_restart()) },
            pressed_color: LINE,
            text_color: ${ action_text_color(model.can_restart()) },
            border_radius: 4
        ) on Tap { model.restart(); }
    };
    ui! {
        Button (
            "TAPES",
            id: "echo_tapes",
            position: Position::Absolute,
            left: 416,
            top: 270,
            width: 50,
            height: 23,
            size: ButtonSize::Compact,
            font_size: 7,
            normal_color: PANEL,
            pressed_color: LINE,
            text_color: TEXT,
            border_radius: 4
        ) on Tap { model.set_modal(EchoModal::Tapes); }
    }
}

#[compose(bind(model))]
fn compose_route_status(model: EchoModel) -> Entity {
    ui! {
        Text (
            "CURRENT ROUTE",
            position: Position::Absolute,
            left: 14,
            top: 281,
            width: 90,
            height: 12,
            font_size: 7,
            text_color: MUTED,
            paragraph: label()
        )
    };
    ui! {
        Text (
            text: ${ format_args!("{} STEPS / {} ECHOES", model.steps(), model.loops()) },
            text_capacity: 22,
            id: "echo_run",
            position: Position::Absolute,
            left: 165,
            top: 281,
            width: 123,
            height: 12,
            font_size: 7,
            text_color: MUTED,
            paragraph: ParagraphStyle::label().with_align(TextAlign::End)
        )
    };
    ui! {
        Text (
            text: ${ MessageLabel(model.message()) },
            text_capacity: 64,
            id: "echo_message",
            position: Position::Absolute,
            left: 14,
            top: 301,
            width: 452,
            height: 14,
            font_size: 7,
            text_color: MUTED,
            paragraph: label()
        )
    }
}

#[compose(bind(model))]
fn compose_modal(model: EchoModel) -> Entity {
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
                text: ${ modal_title(model.modal(), model.complete()) },
                text_capacity: 23,
                id: "echo_modal_title",
                position: Position::Absolute,
                left: 35,
                top: 54,
                width: 310,
                height: 22,
                font_size: 14,
                text_color: TEXT,
                paragraph: label()
            )
            Text (
                text: ${ modal_subtitle(model.modal(), model.complete()) },
                text_capacity: 52,
                id: "echo_modal_subtitle",
                position: Position::Absolute,
                left: 35,
                top: 78,
                width: 390,
                height: 14,
                font_size: 8,
                text_color: MUTED,
                paragraph: label()
            )
            Button (
                "×",
                position: Position::Absolute,
                left: 421,
                top: 50,
                width: 26,
                height: 24,
                size: ButtonSize::Compact,
                font_size: 15,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { model.set_modal(EchoModal::None); }
            Button (
                text: ${ TapeLabel {
                    index: 0,
                    beats: model.ghost_route_lengths()[0],
                    solo: model.peek_ghost() == Some(0),
                } },
                text_capacity: 32,
                id: "echo_tape_0",
                position: Position::Absolute,
                left: 35,
                top: 111,
                width: 412,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Tapes && model.ghost_count() > 0 }
            ) on Tap {
                model.toggle_peek_ghost(0);
            }
            Button (
                text: ${ TapeLabel {
                    index: 1,
                    beats: model.ghost_route_lengths()[1],
                    solo: model.peek_ghost() == Some(1),
                } },
                text_capacity: 32,
                id: "echo_tape_1",
                position: Position::Absolute,
                left: 35,
                top: 150,
                width: 412,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Tapes && model.ghost_count() > 1 }
            ) on Tap {
                model.toggle_peek_ghost(1);
            }
            Button (
                text: ${ TapeLabel {
                    index: 2,
                    beats: model.ghost_route_lengths()[2],
                    solo: model.peek_ghost() == Some(2),
                } },
                text_capacity: 32,
                id: "echo_tape_2",
                position: Position::Absolute,
                left: 35,
                top: 189,
                width: 412,
                height: 32,
                size: ButtonSize::Compact,
                font_size: 9,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Tapes && model.ghost_count() > 2 }
            ) on Tap {
                model.toggle_peek_ghost(2);
            }
            Text (
                text: ${ ResultStats {
                    complete: model.complete(),
                    steps: if model.complete() { model.total_steps() } else { model.steps() },
                    loops: if model.complete() { model.total_loops() } else { u16::from(model.loops()) },
                } },
                text_capacity: 40,
                id: "echo_result_stats",
                position: Position::Absolute,
                left: 35,
                top: 126,
                width: 380,
                height: 30,
                font_size: 20,
                text_color: ACCENT,
                paragraph: label(),
                visible: ${ model.modal() == EchoModal::Result }
            )
            Button (
                text: ${ primary_label(model.complete(), model.result_len()) },
                text_capacity: 16,
                id: "echo_modal_primary",
                position: Position::Absolute,
                left: 35,
                top: 236,
                width: 255,
                height: 36,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: ACCENT,
                pressed_color: RUST,
                text_color: BACKGROUND,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Result }
            ) on Tap { model.continue_archive(); }
            Button (
                "CLEAR ROOM",
                id: "echo_modal_secondary",
                position: Position::Absolute,
                left: 303,
                top: 236,
                width: 144,
                height: 36,
                size: ButtonSize::Compact,
                font_size: 10,
                normal_color: HEADER,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4,
                visible: ${ model.modal() == EchoModal::Tapes }
            ) on Tap { model.clear_room(); }
        }
    }
}

#[compose(bind(model))]
fn build_widgets(model: EchoModel) {
    ui! {
        View (id: "echo_surface", width: 480, height: 320, clip_children: true) [
            EchoSurface {
                model: model.clone(),
            },
            TouchAction::None,
        ] on Tap { surface_gesture(&ctx); }
        {
            compose_header (model)
            compose_controls (model)
            compose_route_status (model)
            compose_modal (model)
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    let model = app.add_model(EchoModel::default());
    #[cfg(feature = "persistence")]
    install_persistence(app, model.clone());
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_plugin(EchoKeyboardPlugin::new(model.clone()));
    app.compose(parent, |cx| build_widgets(cx, model));
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>, model: EchoModelHandle)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::model::ModelHandle;
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let save_model = model.clone();
    let restore_model = model;
    let plugin = PersistencePlugin::new(gallery_storage("mirui_echo_walker.bin"))
        .bytes(
            "echo_walker/replay",
            move |_world| Some(ModelHandle::read(&save_model, EchoModel::encode_replay)),
            move |_world, bytes| {
                let Ok(log) = EchoReplayLog::decode(bytes, ReplayKind::Echo) else {
                    return;
                };
                let Ok(model) = replay_echo(&log) else {
                    return;
                };
                restore_model.restore_replay(model);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
