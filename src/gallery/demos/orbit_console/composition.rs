use super::ConsoleMode;
use super::state::ConsoleState;
use super::style::{BLUE, BORDER, MINT, SURFACE, TEXT, TEXT_MUTED};
use super::visuals::{ActivityPlot, ConsoleBackdrop, OrbitInstrument, SignalMeter};
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Text};

#[compose]
fn compose_header() -> Entity {
    ui! {
        Row (
            height: 54,
            align: AlignItems::Center,
            column_gap: 10
        ) {
            View (
                width: 40,
                height: 40,
                bg_color: MINT,
                border_radius: 12
            ) {
                View (
                    position: Position::Absolute,
                    left: 9,
                    top: 9,
                    width: 22,
                    height: 22,
                    border_color: ColorToken::OnPrimary,
                    border_width: 2,
                    border_radius: 11
                )
                View (
                    position: Position::Absolute,
                    left: 18,
                    top: 5,
                    width: 4,
                    height: 30,
                    bg_color: ColorToken::OnPrimary,
                    border_radius: 2
                )
            }
            Column (grow: 1.0, justify: JustifyContent::Center, row_gap: 2) {
                Text (
                    "mirui : ORBIT CONSOLE",
                    font: FontToken::Heading,
                    font_size: 20,
                    text_color: TEXT
                )
                Text (
                    "fixed point graphics instrument",
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT_MUTED
                )
            }
        }
    }
}

#[compose(bind(model))]
fn compose_orbit_stage(model: ConsoleState) -> Entity {
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
            border_radius: 20,
            clip_children: true
        ) [
            OrbitInstrument::new(model.clone()),
        ] on Tap { model.cycle_focus(); }
        {
            Row (height: 24, align: AlignItems::Center, column_gap: 8) {
                Text (
                    "ORBIT FIELD : 03 NODES",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 10,
                    text_color: TEXT_MUTED
                )
                Text (
                    "LIVE VECTOR",
                    width: 92,
                    height: 22,
                    bg_color: SURFACE,
                    border_color: MINT,
                    border_width: 1,
                    border_radius: 11,
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: MINT,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (grow: 1.0)
            Text (
                text: ${
                    match model.focused_node() {
                        0 => "NODE 01 : ACTIVE",
                        1 => "NODE 02 : ACTIVE",
                        _ => "NODE 03 : ACTIVE",
                    }
                },
                id: "orbit_console_focus",
                height: 20,
                font: FontToken::Mono,
                font_size: 10,
                text_color: MINT
            )
        }
    }
}

#[compose(bind(model))]
fn compose_signal_card(model: ConsoleState) -> Entity {
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
            border_radius: 16
        ) [
            SignalMeter {
                model: model.clone(),
            },
        ] {
            Column (grow: 1.0, row_gap: 2) {
                Text (
                    "SIGNAL",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: TEXT_MUTED
                )
                Text (
                    "98 PCT",
                    font: FontToken::Heading,
                    font_size: 20,
                    text_color: TEXT
                )
            }
            View (width: 48)
        }
    }
}

#[compose(bind(model))]
fn compose_activity_card(model: ConsoleState) -> Entity {
    ui! {
        Column (
            id: "orbit_console_activity",
            grow: 5.0,
            min_height: 48,
            padding: Padding::all(10),
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            ActivityPlot {
                model: model.clone(),
            },
        ] {
            Row (height: 18, align: AlignItems::Center) {
                Text (
                    "ACTIVITY : LIVE",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: TEXT_MUTED
                )
                Text (
                    "12.4",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: BLUE
                )
            }
        }
    }
}

#[compose(bind(model))]
fn compose_controls(model: ConsoleState) -> Entity {
    ui! {
        Column (
            id: "orbit_console_controls",
            grow: 4.0,
            min_height: 64,
            padding: Padding::all(10),
            row_gap: 4,
            bg_color: SURFACE,
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) {
            Row (height: 16, align: AlignItems::Center) {
                Text (
                    "CONTROL · INTENSITY",
                    grow: 1.0,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: TEXT_MUTED
                )
                Text (
                    text: ${ format_args!("{}", model.intensity()) },
                    text_capacity: 3,
                    id: "orbit_console_intensity_value",
                    font: FontToken::Mono,
                    font_size: 9,
                    text_color: MINT
                )
            }
            Row (height: 20, column_gap: 4) {
                Button (
                    id: "orbit_console_mode_orbit",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Orbit.chip_background(model.mode()) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Orbit.chip_foreground(model.mode()) }
                ) [
                    Text::label("ORBIT"),
                ] on Tap { model.select_mode(ConsoleMode::Orbit); }
                Button (
                    id: "orbit_console_mode_flow",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Flow.chip_background(model.mode()) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Flow.chip_foreground(model.mode()) }
                ) [
                    Text::label("FLOW"),
                ] on Tap { model.select_mode(ConsoleMode::Flow); }
                Button (
                    id: "orbit_console_mode_pulse",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Pulse.chip_background(model.mode()) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Pulse.chip_foreground(model.mode()) }
                ) [
                    Text::label("PULSE"),
                ] on Tap { model.select_mode(ConsoleMode::Pulse); }
            }
            Row (grow: 1.0, min_height: 20, align: AlignItems::Center, column_gap: 6) {
                Slider (
                    id: "orbit_console_intensity",
                    grow: 1.0,
                    height: 18,
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    value: ${ Fixed::from_int(i32::from(model.intensity())) },
                    track_color: BORDER,
                    fill_color: MINT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = old;
                    model.set_intensity(*new);
                }
                Button (
                    id: "orbit_console_pause",
                    size: ButtonSize::Compact,
                    text: ${ if model.paused() { "RESUME" } else { "PAUSE" } },
                    width: 54,
                    height: 20,
                    normal_color: SURFACE,
                    pressed_color: MINT,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: TEXT
                ) on Tap { model.toggle_paused(); }
            }
        }
    }
}

#[compose(bind(model))]
fn compose_inspector(model: ConsoleState) -> Entity {
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

#[compose(bind(model))]
fn compose_workspace(model: ConsoleState) -> Entity {
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
            border_radius: 10
        ) {
            Text ("16.67 ms", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text ("Q24.8", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text ("CPU", grow: 1.0, font: FontToken::Mono, font_size: 9, text_color: TEXT)
            Text (
                "CONNECTED",
                width: 72,
                font: FontToken::Mono,
                font_size: 8,
                text_color: MINT,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose(bind(model))]
pub(super) fn build_widgets(model: ConsoleState) {
    ui! {
        Column (
            id: "orbit_console_shell",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            padding: Padding::all(14),
            row_gap: 10,
            clip_children: true
        ) [
            ConsoleBackdrop::new(),
        ] {
            compose_header ()
            compose_workspace (model)
            compose_status_strip ()
        }
    };
}
