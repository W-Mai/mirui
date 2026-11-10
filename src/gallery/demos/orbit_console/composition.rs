use super::ConsoleMode;
use super::state::ConsoleState;
use super::style::{ACCENT, BG, BORDER, OK, RAISED, SURFACE, TEXT, TEXT_MUTED};
use super::visuals::{ActivityPlot, OrbitInstrument, SignalMeter};
use crate::gallery::brand::MiruiMark;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Text};

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            height: 44,
            align: AlignItems::Center,
            column_gap: 10
        ) {
            View (width: 28, height: 28) [
                MiruiMark,
            ]
            Column (grow: 1.0, justify: JustifyContent::Center, row_gap: 1) {
                Text (
                    "Orbit Console",
                    font: FontToken::Heading,
                    font_size: 17,
                    text_color: TEXT
                )
                Text (
                    "Fixed-point graphics instrument",
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT_MUTED
                )
            }
        }
    }
}

#[compose]
fn compose_live_badge() -> Entity {
    ui! {
        Row (
            width: 92,
            height: 22,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            column_gap: 6,
            bg_color: BG,
            border_color: BORDER,
            border_width: 1,
            border_radius: 11
        ) {
            View (width: 6, height: 6, bg_color: OK, border_radius: 3)
            Text (
                "Live vector",
                font: FontToken::Mono,
                font_size: 10,
                text_color: TEXT_MUTED,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_orbit_stage(model: model!(ConsoleState)) -> Entity {
    ui! {
        Column (
            id: "orbit_console_stage",
            grow: 2.0,
            min_width: 260,
            min_height: Dimension::percent(48),
            padding: Padding::all(12),
            row_gap: 6,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 12,
            clip_children: true
        ) [
            OrbitInstrument {
                model: model.clone(),
            },
        ] {
            Row (height: 24, align: AlignItems::Center, column_gap: 8) {
                Text (
                    "Orbit field",
                    width: 84,
                    font: FontToken::Heading,
                    font_size: 13,
                    text_color: TEXT
                )
                Text (
                    "3 nodes",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT_MUTED
                )
                compose_live_badge ()
            }
            View (grow: 1.0)
            Text (
                text: ${
                    match model.focused_node() {
                        0 => "Tracking node 1",
                        1 => "Tracking node 2",
                        _ => "Tracking node 3",
                    }
                },
                id: "orbit_console_focus",
                height: 20,
                font: FontToken::Mono,
                font_size: 11,
                text_color: TEXT
            )
        } on Tap { model.cycle_focus(); }
    }
}

#[compose]
fn compose_signal_card(model: model!(ConsoleState)) -> Entity {
    ui! {
        Row (
            id: "orbit_console_signal",
            grow: 4.0,
            min_height: 44,
            padding: Padding::all(10),
            align: AlignItems::Center,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) [
            SignalMeter {
                model: model.clone(),
            },
        ] {
            Column (grow: 1.0, row_gap: 2) {
                Text (
                    "Signal",
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT_MUTED
                )
                Text (
                    "98%",
                    font: FontToken::Heading,
                    font_size: 22,
                    text_color: TEXT
                )
            }
            View (width: 48)
        }
    }
}

#[compose]
fn compose_activity_card(model: model!(ConsoleState)) -> Entity {
    ui! {
        Column (
            id: "orbit_console_activity",
            grow: 5.0,
            min_height: 48,
            padding: Padding::all(10),
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) [
            ActivityPlot {
                model: model.clone(),
            },
        ] {
            Row (height: 18, align: AlignItems::Center) {
                Text (
                    "Activity",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT_MUTED
                )
                Text (
                    "12.4 k/s",
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT
                )
            }
        }
    }
}

#[compose]
fn compose_controls(model: model!(ConsoleState)) -> Entity {
    ui! {
        Column (
            id: "orbit_console_controls",
            grow: 4.0,
            min_height: 96,
            padding: Padding::all(10),
            row_gap: 6,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) {
            Row (height: 16, align: AlignItems::Center) {
                Text (
                    "Intensity",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT_MUTED
                )
                Text (
                    text: ${ format_args!("{}", model.intensity()) },
                    capacity: 3,
                    id: "orbit_console_intensity_value",
                    font: FontToken::Mono,
                    font_size: 11,
                    text_color: TEXT
                )
            }
            Row (
                height: 24,
                padding: Padding::all(2),
                column_gap: 2,
                bg_color: BG,
                border_radius: 7
            ) {
                Button (
                    id: "orbit_console_mode_orbit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Orbit.chip_background(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: ${ ConsoleMode::Orbit.chip_foreground(model.mode()) }
                ) [
                    Text::label("Orbit"),
                ] on Tap { model.select_mode(ConsoleMode::Orbit); }
                Button (
                    id: "orbit_console_mode_flow",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Flow.chip_background(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: ${ ConsoleMode::Flow.chip_foreground(model.mode()) }
                ) [
                    Text::label("Flow"),
                ] on Tap { model.select_mode(ConsoleMode::Flow); }
                Button (
                    id: "orbit_console_mode_pulse",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Pulse.chip_background(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: ${ ConsoleMode::Pulse.chip_foreground(model.mode()) }
                ) [
                    Text::label("Pulse"),
                ] on Tap { model.select_mode(ConsoleMode::Pulse); }
            }
            Row (grow: 1.0, min_height: 22, align: AlignItems::Center, column_gap: 8) {
                Slider (
                    id: "orbit_console_intensity",
                    grow: 1.0,
                    height: 18,
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    value: ${ Fixed::from_int(i32::from(model.intensity())) },
                    track_color: BORDER,
                    fill_color: ACCENT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = event.old;
                    model.set_intensity(event.new);
                }
                Button (
                    id: "orbit_console_pause",
                    size: ButtonSize::Compact,
                    text: ${ if model.paused() { "Resume" } else { "Pause" } },
                    width: 60,
                    height: 22,
                    normal_color: SURFACE,
                    pressed_color: RAISED,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT
                ) on Tap { model.toggle_paused(); }
            }
        }
    }
}

#[compose]
fn compose_inspector(model: model!(ConsoleState)) -> Entity {
    ui! {
        Column (
            id: "orbit_console_inspector",
            grow: 1.0,
            min_width: 170,
            min_height: Dimension::percent(48),
            row_gap: 8
        ) {
            compose_signal_card (model)
            compose_activity_card (model)
            compose_controls (model)
        }
    }
}

#[compose]
fn compose_workspace(model: model!(ConsoleState)) -> Entity {
    ui! {
        Row (
            id: "orbit_console_workspace",
            grow: 1.0,
            wrap: FlexWrap::Wrap,
            align: AlignItems::Stretch,
            row_gap: 10,
            column_gap: 12
        ) {
            compose_orbit_stage (model)
            compose_inspector (model)
        }
    }
}

#[compose]
fn compose_status_strip() -> Entity {
    ui! {
        Row (
            height: 28,
            padding: Padding::all(6),
            align: AlignItems::Center,
            column_gap: 8,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 8
        ) {
            Text (
                "16.67 ms",
                grow: 1.0,
                font: FontToken::Mono,
                font_size: 10,
                text_color: TEXT_MUTED
            )
            Text (
                "Q24.8",
                grow: 1.0,
                font: FontToken::Mono,
                font_size: 10,
                text_color: TEXT_MUTED
            )
            Text ("CPU", grow: 1.0, font: FontToken::Mono, font_size: 10, text_color: TEXT_MUTED)
            Row (
                width: 84,
                height: 16,
                align: AlignItems::Center,
                justify: JustifyContent::FlexEnd,
                column_gap: 6
            ) {
                View (width: 6, height: 6, bg_color: OK, border_radius: 3)
                Text (
                    "Connected",
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT_MUTED,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(ConsoleState)) {
    ui! {
        Column (
            id: "orbit_console_shell",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            padding: Padding::all(14),
            row_gap: 10,
            bg_color: BG,
            clip_children: true
        ) {
            compose_header ()
            compose_workspace (model)
            compose_status_strip ()
        }
    };
}
