use super::render::{KineticOrbit, KineticWave};
use super::state::{ConsoleMode, ConsoleModel, ConsoleMotion};
use super::style::{AMBER, BACKGROUND, BORDER, CYAN, MUTED, PANEL, TEXT, VIOLET, header_label};
use crate::core::reactive::Signal;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Slider, Text};

#[compose(bind(model, motion))]
pub(super) fn build_widgets(model: ConsoleModel, motion: Signal<ConsoleMotion>) {
    let orbit_motion_key = motion.clone();
    let orbit_motion = motion.clone();
    let wave_motion_key = motion.clone();
    let wave_motion = motion;
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
                    normal_color: ${ if model.paused() { ColorToken::Error } else { ColorToken::Success } },
                    pressed_color: PANEL,
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { model.toggle_paused(); }
                {
                    Text (
                        text: ${ if model.paused() { "HOLD" } else { "LIVE" } },
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
                    View (
                        id: "kinetic_console_orbit_layer",
                        width: 52,
                        height: Dimension::percent(100),
                        render_key: ${ orbit_motion_key.get().orbit_render_key() }
                    ) [
                        KineticOrbit {
                            model: model.clone(),
                            motion: orbit_motion,
                        },
                    ] on Tap { model.cycle_focus(); }
                }
                View (
                    id: "kinetic_console_wave_layer",
                    width: Dimension::percent(100),
                    height: 14,
                    render_key: ${ wave_motion_key.get().wave_render_key() }
                ) [
                    KineticWave {
                        model: model.clone(),
                        motion: wave_motion,
                    },
                    IgnoreHitTest,
                ]
            }
            Row (height: 16, column_gap: 3) {
                Button (
                    id: "kinetic_console_orbit",
                    grow: 1.0,
                    height: 16,
                    padding: Padding::all(2),
                    normal_color: ${ if model.mode() == ConsoleMode::Orbit { CYAN } else { PANEL } },
                    pressed_color: CYAN,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { model.select_mode(ConsoleMode::Orbit); }
                {
                    Text (
                        "ORB",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if model.mode() == ConsoleMode::Orbit { ColorToken::OnPrimary } else { MUTED } },
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
                    normal_color: ${ if model.mode() == ConsoleMode::Flow { VIOLET } else { PANEL } },
                    pressed_color: VIOLET,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { model.select_mode(ConsoleMode::Flow); }
                {
                    Text (
                        "FLOW",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if model.mode() == ConsoleMode::Flow { ColorToken::OnTertiary } else { MUTED } },
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
                    normal_color: ${ if model.mode() == ConsoleMode::Pulse { AMBER } else { PANEL } },
                    pressed_color: AMBER,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 6,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) on Tap { model.select_mode(ConsoleMode::Pulse); }
                {
                    Text (
                        "PLS",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ if model.mode() == ConsoleMode::Pulse { ColorToken::OnPrimary } else { MUTED } },
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
                value: ${ model.intensity() },
                track_color: BORDER,
                fill_color: CYAN,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = old;
                model.set_intensity(*new);
            }
        }
    };
}
