use super::input::{EchoKeyboardPlugin, surface_gesture};
use super::render::{modal_view, surface_view};
use super::state::{EchoModalSurface, EchoNodes, EchoSurface};
use super::style::{ACCENT, BACKGROUND, HEADER, LINE, MUTED, PANEL, RUST, TEXT};
use crate::gallery::play::echo::{Direction, EchoCommand, EchoModal, EchoModel};
use crate::gallery::play::font::register_play_font;
#[cfg(feature = "persistence")]
use crate::gallery::play::storage::{EchoReplayLog, ReplayKind, replay_echo};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Text, TextAlign};

fn label() -> ParagraphStyle {
    ParagraphStyle::label().with_align(TextAlign::Start)
}
#[compose]
fn build_widgets() {
    ui! {
        EchoSurface (id: "echo_surface", width: 480, height: 320, clip_children: true) [
            TouchAction::None,
        ] on Tap { surface_gesture(ctx.world, ctx.entity, ctx.event); }
        {
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
            Text (
                "ARCHIVE 01 / 12",
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
            Text (
                "MEMORY 0/2",
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
            Text (
                "ECHOES 0/3",
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
            Text (
                "00",
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
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Up)); }
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
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Left)); }
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
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Wait)); }
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
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Right)); }
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
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Step(Direction::Down)); }
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
                normal_color: ACCENT,
                pressed_color: RUST,
                text_color: BACKGROUND,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Rewind); }
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
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Undo); }
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
                normal_color: PANEL,
                pressed_color: LINE,
                text_color: TEXT,
                border_radius: 4
            ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Restart); }
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
            ) on Tap { EchoNodes::update(ctx.world, |model| model.set_modal(EchoModal::Tapes)); }
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
            Text (
                "0 STEPS / 0 ECHOES",
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
            Text (
                "Reach a plate, rewind, and let the past replay the route.",
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
            EchoModalSurface (
                id: "echo_modal",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 480,
                height: 320,
                clip_children: true
            ) {
                Text (
                    "ECHO TAPES",
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
                    "Inspect each fixed route.",
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
                ) on Tap { EchoNodes::update(ctx.world, |model| model.set_modal(EchoModal::None)); }
                Button (
                    "ECHO 1",
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
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(0) { None } else { Some(0) },
                                )
                        },
                    );
                }
                Button (
                    "ECHO 2",
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
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(1) { None } else { Some(1) },
                                )
                        },
                    );
                }
                Button (
                    "ECHO 3",
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
                    border_radius: 4
                ) on Tap {
                    EchoNodes::update(
                        ctx.world,
                        |model| {
                            model
                                .set_peek_ghost(
                                    if model.peek_ghost() == Some(2) { None } else { Some(2) },
                                )
                        },
                    );
                }
                Text (
                    "ROOM 0 STEPS · 0 REWINDS",
                    id: "echo_result_stats",
                    position: Position::Absolute,
                    left: 35,
                    top: 126,
                    width: 380,
                    height: 30,
                    font_size: 20,
                    text_color: ACCENT,
                    paragraph: label()
                )
                Button (
                    "NEXT ROOM",
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
                    border_radius: 4
                ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Continue); }
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
                    border_radius: 4
                ) on Tap { EchoNodes::dispatch(ctx.world, EchoCommand::Clear); }
            }
        }
    };
}

pub(super) fn setup_app<B, F>(app: &mut App<B, F>, parent: Entity)
where
    B: Surface,
    F: RendererFactory<B>,
{
    register_play_font(&mut app.world);
    app.world.insert_resource(EchoModel::default());
    #[cfg(feature = "persistence")]
    app.world
        .insert_resource(EchoReplayLog::new(ReplayKind::Echo, 2718));
    #[cfg(feature = "persistence")]
    install_persistence(app);
    app.with_widget(surface_view()).with_widget(modal_view());
    app.add_plugin(EchoKeyboardPlugin);
    app.compose(parent, build_widgets);
    let find = |id| app.world.find_by_id(id).expect("Echo Walker node");
    let nodes = EchoNodes {
        surface: find("echo_surface"),
        room: find("echo_room"),
        memory: find("echo_memory"),
        ghosts: find("echo_ghosts"),
        tick: find("echo_tick"),
        run: find("echo_run"),
        message: find("echo_message"),
        actions: [
            find("echo_rewind"),
            find("echo_undo"),
            find("echo_restart"),
            find("echo_tapes"),
        ],
        modal: find("echo_modal"),
        modal_title: find("echo_modal_title"),
        modal_subtitle: find("echo_modal_subtitle"),
        modal_primary: find("echo_modal_primary"),
        modal_secondary: find("echo_modal_secondary"),
        tape_rows: [
            find("echo_tape_0"),
            find("echo_tape_1"),
            find("echo_tape_2"),
        ],
        result_stats: find("echo_result_stats"),
    };
    app.world.insert_resource(nodes);
    EchoNodes::sync(&mut app.world);
}

#[cfg(feature = "persistence")]
fn install_persistence<B, F>(app: &mut App<B, F>)
where
    B: Surface,
    F: RendererFactory<B>,
{
    use crate::core::persistence::PersistencePlugin;
    use crate::gallery::play::storage::gallery_storage;

    let plugin = PersistencePlugin::new(gallery_storage("mirui_echo_walker.bin"))
        .bytes(
            "echo_walker/replay",
            |world| {
                world
                    .resource::<EchoReplayLog>()
                    .map(|log| log.encode_vec())
            },
            |world, bytes| {
                let Ok(log) = EchoReplayLog::decode(bytes, ReplayKind::Echo) else {
                    return;
                };
                let Ok(model) = replay_echo(&log) else {
                    return;
                };
                world.insert_resource(log);
                world.insert_resource(model);
            },
        )
        .autosave_every_ms(1000);
    app.add_plugin(plugin);
}
