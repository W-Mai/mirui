use super::render::{KineticOrbit, KineticWave};
use super::state::{ConsoleMode, ConsoleModel, ConsoleMotion};
use super::style::{ACCENT, BACKGROUND, BORDER, HELD, LIVE, PANEL, RAISED, TEXT, header_label};
use crate::gallery::brand::MiruiLogo;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Slider, Text};

#[compose]
pub(super) fn build_widgets(model: model!(ConsoleModel), motion: model!(ConsoleMotion)) {
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
                View (width: 44, height: 15) [
                    MiruiLogo,
                    IgnoreHitTest,
                ]
                View (grow: 1.0, height: 18) [
                    IgnoreHitTest,
                ]
                Button (
                    id: "kinetic_console_status",
                    width: 42,
                    height: 16,
                    padding: Padding::all(2),
                    normal_color: PANEL,
                    pressed_color: RAISED,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 8,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center,
                    column_gap: 3
                ) {
                    View (
                        width: 4,
                        height: 4,
                        bg_color: ${ if model.paused() { HELD } else { LIVE } },
                        border_radius: 2
                    ) [
                        IgnoreHitTest,
                    ]
                    Text (
                        text: ${ if model.paused() { "Hold" } else { "Live" } },
                        width: 27,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: TEXT,
                        paragraph: header_label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { model.toggle_paused(); }
            }
            Column (
                id: "kinetic_console_instrument",
                grow: 1.0,
                min_height: 58,
                bg_color: PANEL,
                border_radius: 4,
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
                        height: Dimension::percent(100)
                    ) [
                        KineticOrbit {
                            model: model.clone(),
                            motion: motion.clone(),
                        },
                    ] on Tap { model.cycle_focus(); }
                }
                View (
                    id: "kinetic_console_wave_layer",
                    width: Dimension::percent(100),
                    height: 14
                ) [
                    KineticWave {
                        model: model.clone(),
                        motion,
                    },
                    IgnoreHitTest,
                ]
            }
            Row (
                height: 16,
                padding: Padding::all(1),
                column_gap: 1,
                bg_color: PANEL,
                border_radius: 5
            ) {
                Button (
                    id: "kinetic_console_orbit",
                    grow: 1.0,
                    height: 14,
                    padding: Padding::all(1),
                    normal_color: ${ ConsoleMode::Orbit.segment_fill(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 4,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) {
                    Text (
                        "Orbit",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ ConsoleMode::Orbit.segment_text(model.mode()) },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { model.select_mode(ConsoleMode::Orbit); }
                Button (
                    id: "kinetic_console_flow",
                    grow: 1.0,
                    height: 14,
                    padding: Padding::all(1),
                    normal_color: ${ ConsoleMode::Flow.segment_fill(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 4,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) {
                    Text (
                        "Flow",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ ConsoleMode::Flow.segment_text(model.mode()) },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { model.select_mode(ConsoleMode::Flow); }
                Button (
                    id: "kinetic_console_pulse",
                    grow: 1.0,
                    height: 14,
                    padding: Padding::all(1),
                    normal_color: ${ ConsoleMode::Pulse.segment_fill(model.mode()) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 4,
                    justify: JustifyContent::Center,
                    align: AlignItems::Center
                ) {
                    Text (
                        "Pulse",
                        grow: 1.0,
                        height: Dimension::percent(100),
                        font_size: 7,
                        text_color: ${ ConsoleMode::Pulse.segment_text(model.mode()) },
                        paragraph: ParagraphStyle::label()
                    ) [
                        IgnoreHitTest,
                    ]
                } on Tap { model.select_mode(ConsoleMode::Pulse); }
            }
            Slider (
                id: "kinetic_console_intensity",
                height: 10,
                min: Fixed::ZERO,
                max: Fixed::from_int(100),
                value: ${ model.intensity() },
                track_color: BORDER,
                fill_color: ACCENT,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = event.old;
                model.set_intensity(event.new);
            }
        }
    };
}
