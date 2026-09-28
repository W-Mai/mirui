use alloc::format;

use super::state::{ConsoleAction, ConsoleMode, ConsoleModel, ConsoleState};
use super::style::{BLUE, BORDER, MINT, SURFACE, TEXT, TEXT_MUTED};
use super::visuals::{ActivityPlot, ConsoleBackdrop, OrbitInstrument, SignalMeter};
use crate::core::reactive::Signal;
use crate::prelude::*;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Text};

fn console_signal(cx: &mut crate::ui::UiScope<'_>) -> Signal<ConsoleState> {
    cx.world_mut()
        .resource::<ConsoleModel>()
        .map(ConsoleModel::signal)
        .expect("Orbit Console model must be installed before composition")
}

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

#[compose]
fn compose_orbit_stage() -> Entity {
    let state_signal = console_signal(cx);
    let stage_visual = state_signal.clone();
    let stage_action = state_signal.clone();
    let focus_text = state_signal;

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
            clip_children: true,
            render_key: ${ u64::from(stage_visual.get().revision()) }
        ) [
            OrbitInstrument::new(),
        ] on Tap { ConsoleAction::CycleFocus.publish(&stage_action); }
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
                    match focus_text.get().focused_node {
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

#[compose]
fn compose_signal_card() -> Entity {
    let signal_visual = console_signal(cx);

    ui! {
        Row (
            id: "orbit_console_signal",
            grow: 4.0,
            min_height: 44,
            padding: Padding::all(10),
            align: AlignItems::Center,
            bg_color: SURFACE,
            render_key: ${ u64::from(signal_visual.get().revision()) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            SignalMeter,
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

#[compose]
fn compose_activity_card() -> Entity {
    let activity_visual = console_signal(cx);

    ui! {
        Column (
            id: "orbit_console_activity",
            grow: 5.0,
            min_height: 48,
            padding: Padding::all(10),
            bg_color: SURFACE,
            render_key: ${ u64::from(activity_visual.get().revision()) },
            border_color: BORDER,
            border_width: 1,
            border_radius: 16
        ) [
            ActivityPlot,
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

#[compose]
fn compose_controls() -> Entity {
    let state_signal = cx
        .world_mut()
        .resource::<ConsoleModel>()
        .map(ConsoleModel::signal)
        .expect("Orbit Console model must be installed before composition");
    let state = state_signal.get_untracked();
    let intensity_text = state_signal.clone();
    let (orbit_bg, orbit_fg, orbit_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let (flow_bg, flow_fg, flow_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let (pulse_bg, pulse_fg, pulse_action) = (
        state_signal.clone(),
        state_signal.clone(),
        state_signal.clone(),
    );
    let slider_action = state_signal.clone();
    let (pause_text, pause_action) = (state_signal.clone(), state_signal);

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
                    text: ${ format!("{}", intensity_text.get().intensity) },
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
                    normal_color: ${ ConsoleMode::Orbit.chip_background(orbit_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Orbit.chip_foreground(orbit_fg.get().mode) }
                ) [
                    Text::label("ORBIT"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Orbit).publish(&orbit_action); }
                Button (
                    id: "orbit_console_mode_flow",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Flow.chip_background(flow_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Flow.chip_foreground(flow_fg.get().mode) }
                ) [
                    Text::label("FLOW"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Flow).publish(&flow_action); }
                Button (
                    id: "orbit_console_mode_pulse",
                    size: ButtonSize::Compact,
                    grow: 1.0,
                    height: 20,
                    normal_color: ${ ConsoleMode::Pulse.chip_background(pulse_bg.get().mode) },
                    pressed_color: SURFACE,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 10,
                    font: FontToken::Mono,
                    font_size: 8,
                    text_color: ${ ConsoleMode::Pulse.chip_foreground(pulse_fg.get().mode) }
                ) [
                    Text::label("PULSE"),
                ] on Tap { ConsoleAction::SelectMode(ConsoleMode::Pulse).publish(&pulse_action); }
            }
            Row (grow: 1.0, min_height: 20, align: AlignItems::Center, column_gap: 6) {
                Slider (
                    id: "orbit_console_intensity",
                    grow: 1.0,
                    height: 18,
                    min: Fixed::ZERO,
                    max: Fixed::from_int(100),
                    value: Fixed::from_int(state.intensity as i32),
                    track_color: BORDER,
                    fill_color: MINT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = old;
                    ConsoleAction::SetIntensity(*new).publish(&slider_action);
                }
                Button (
                    id: "orbit_console_pause",
                    size: ButtonSize::Compact,
                    text: ${ if pause_text.get().paused { "RESUME" } else { "PAUSE" } },
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
                ) on Tap { ConsoleAction::TogglePaused.publish(&pause_action); }
            }
        }
    }
}

#[compose]
fn compose_inspector() -> Entity {
    ui! {
        Column (
            id: "orbit_console_inspector",
            grow: 1.0,
            min_width: 170,
            min_height: Dimension::percent(48),
            row_gap: 8
        ) {
            compose_signal_card ()
            compose_activity_card ()
            compose_controls ()
        }
    }
}

#[compose]
fn compose_workspace() -> Entity {
    ui! {
        Row (
            id: "orbit_console_workspace",
            grow: 1.0,
            wrap: FlexWrap::Wrap,
            align: AlignItems::Stretch,
            row_gap: 10,
            column_gap: 12
        ) {
            compose_orbit_stage ()
            compose_inspector ()
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

#[compose]
pub fn build_widgets() {
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
            compose_workspace ()
            compose_status_strip ()
        }
    };
}
