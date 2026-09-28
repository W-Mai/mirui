use super::render::{KineticOrbit, KineticWave};
use super::state::{ConsoleAction, ConsoleMode, ConsoleModel};
use super::style::{AMBER, BACKGROUND, BORDER, CYAN, MUTED, PANEL, TEXT, VIOLET, header_label};
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Slider, Text};

fn console_model(cx: &mut crate::ui::UiScope<'_>) -> ConsoleModel {
    cx.world_mut()
        .resource::<ConsoleModel>()
        .cloned()
        .expect("Kinetic Console model")
}

#[compose]
pub fn build_widgets() {
    let model = console_model(cx);
    let status_text = model.paused.clone();
    let status_bg = model.paused.clone();
    let status_action = model.clone();
    let stage_action = model.clone();
    let orbit_bg = model.mode.clone();
    let orbit_fg = model.mode.clone();
    let orbit_action = model.clone();
    let flow_bg = model.mode.clone();
    let flow_fg = model.mode.clone();
    let flow_action = model.clone();
    let pulse_bg = model.mode.clone();
    let pulse_fg = model.mode.clone();
    let pulse_action = model.clone();
    let slider_action = model;

    ui! {
        Column (
            id: "kinetic_console_shell",
            grow: 1.0,
            padding: Padding::all(5),
            row_gap: 4,
            bg_color: BACKGROUND
        ) [
            IgnoreHitTest,
        ] {
            Row (height: 18, align: AlignItems::Center, column_gap: 4) {
                View (width: 4, height: 4, bg_color: CYAN, border_radius: 2) [
                    IgnoreHitTest,
                ]
                Text (
                    "MIRUI",
                    grow: 1.0,
                    height: 18,
                    font_size: 8,
                    text_color: TEXT,
                    paragraph: header_label()
                ) [
                    IgnoreHitTest,
                ]
                Button (
                    id: "kinetic_console_status",
                    width: 38,
                    height: 18,
                    padding: Padding::all(2),
                    normal_color: ${ if status_bg.get() { ColorToken::Error } else { ColorToken::Success } },
                    pressed_color: PANEL,
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::TogglePaused.publish(&status_action); }
                {
                    Text (
                        text: ${ if status_text.get() { "HOLD" } else { "LIVE" } },
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
            }
            Column (
                id: "kinetic_console_instrument",
                grow: 1.0,
                min_height: 58,
                bg_color: PANEL,
                clip_children: true
            ) [
                IgnoreHitTest,
            ] {
                Row (
                    grow: 1.0,
                    justify: JustifyContent::Center,
                    align: AlignItems::Stretch
                ) [
                    IgnoreHitTest,
                ] {
                    KineticOrbit (
                        id: "kinetic_console_orbit_layer",
                        width: 52,
                        height: Dimension::percent(100)
                    ) on Tap { ConsoleAction::CycleFocus.publish(&stage_action); }
                }
                KineticWave (
                    id: "kinetic_console_wave_layer",
                    width: Dimension::percent(100),
                    height: 14
                ) [
                    IgnoreHitTest,
                ]
            }
            Row (height: 16, column_gap: 3) {
                Button (
                    id: "kinetic_console_orbit",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    normal_color: ${ if orbit_bg.get() == ConsoleMode::Orbit { CYAN } else { PANEL } },
                    pressed_color: CYAN,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Orbit).publish(&orbit_action); }
                {
                    Text (
                        "ORB",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if orbit_fg.get() == ConsoleMode::Orbit { ColorToken::OnPrimary } else { MUTED } },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
                Button (
                    id: "kinetic_console_flow",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    normal_color: ${ if flow_bg.get() == ConsoleMode::Flow { VIOLET } else { PANEL } },
                    pressed_color: VIOLET,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Flow).publish(&flow_action); }
                {
                    Text (
                        "FLOW",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if flow_fg.get() == ConsoleMode::Flow { ColorToken::OnTertiary } else { MUTED } },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
                Button (
                    id: "kinetic_console_pulse",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    normal_color: ${ if pulse_bg.get() == ConsoleMode::Pulse { AMBER } else { PANEL } },
                    pressed_color: AMBER,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { ConsoleAction::Select(ConsoleMode::Pulse).publish(&pulse_action); }
                {
                    Text (
                        "PLS",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if pulse_fg.get() == ConsoleMode::Pulse { ColorToken::OnPrimary } else { MUTED } },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                }
            }
            Slider (
                id: "kinetic_console_intensity",
                height: 10,
                min: Fixed::ZERO,
                max: Fixed::from_int(100),
                value: Fixed::from_int(68),
                track_color: BORDER,
                fill_color: CYAN,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = old;
                ConsoleAction::SetIntensity(*new).publish(&slider_action);
            }
        }
    };
}
