use super::geometry::{CurvePaths, path_window_end};
use super::runtime::{CurveMotion, bind_curve_paths};
use super::stage::CurveStage;
use super::state::{CurveModel, CurveStageSize};
use super::style::{
    ACCENT, BACKGROUND, BORDER, MUTED, PANEL, PANEL_ALT, RAISED, ROUTE_LABEL, TEXT, UI,
    bounded_text, mixed_stack, single_line,
};
use crate::gallery::brand::MiruiMark;
use crate::prelude::*;
use crate::ui::IgnoreHitTest;
use crate::ui::widgets::{Button, ParagraphStyle, Slider, Text, TextDirection};

#[compose]
fn compose_header() -> Entity {
    ui! {
        Column (
            id: "curve_text_header",
            width: Dimension::percent(100),
            height: @id(curve_text_document).width {
                if curve_text_document.width < Fixed::from_int(520) { 124 } else { 108 }
            },
            min_height: @id(curve_text_document).width {
                if curve_text_document.width < Fixed::from_int(520) { 124 } else { 108 }
            },
            row_gap: 6
        ) {
            Row (
                width: Dimension::percent(100),
                height: @id(curve_text_document).width {
                    if curve_text_document.width < Fixed::from_int(520) { 82 } else { 66 }
                },
                align: AlignItems::Center,
                column_gap: 12
            ) {
                View (width: 32, height: 32) [
                    MiruiMark,
                ]
                Column (grow: 1.0, min_width: 150, row_gap: 2) {
                    Text (
                        "Kinetic Type",
                        width: Dimension::percent(100),
                        height: 34,
                        font: UI,
                        font_size: 24,
                        text_color: TEXT,
                        paragraph: bounded_text(1)
                    )
                    Text (
                        "One shaped run, one retained path, continuous pose",
                        width: Dimension::percent(100),
                        height: @id(curve_text_document).width {
                            if curve_text_document.width < Fixed::from_int(520) { 46 } else { 30 }
                        },
                        font: UI,
                        font_size: 12,
                        text_color: MUTED,
                        paragraph: bounded_text(2)
                    )
                }
            }
            Text (
                ROUTE_LABEL,
                width: 220,
                min_width: 170,
                max_width: 220,
                height: 28,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 14,
                font: UI,
                font_size: 11,
                text_color: MUTED,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_stage_backdrop(model: model!(CurveModel), paths: CurvePaths) -> Entity {
    let stage = ui! {
        View (
            id: "curve_text_stage",
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) [
            CurveStage { model: model.clone() },
            CurveMotion::default(),
            IgnoreHitTest,
        ] on SizeChanged {
            model
                .set_stage_size(
                    CurveStageSize::from_dimensions(event.new.width, event.new.height),
                );
        }
    };
    ui!(bind_curve_paths(stage, paths, model));
    stage
}

#[compose]
fn compose_stage(model: model!(CurveModel), paths: CurvePaths) -> Entity {
    ui! {
        View (
            id: "curve_text_stage_shell",
            width: Dimension::percent(100),
            height: @id(curve_text_shell).height {
                if curve_text_shell.height < Fixed::from_int(600) {
                    250
                } else if curve_text_shell.height < Fixed::from_int(720) {
                    300
                } else {
                    360
                }
            },
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 12
        ) {
            compose_stage_backdrop (model, paths)
            Text (
                id: "curve_text_primary",
                "MIRUI / BEND SPACE, NOT GLYPHS",
                path: ${ crate::text::TextPath::new(paths.ids[0])
                    .with_range(Fixed::ZERO..path_window_end(model.stage_size().width)) },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font: UI,
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 18_u16 } else { 30_u16 }
                },
                text_color: TEXT,
                paragraph: single_line(TextDirection::LeftToRight)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_multiscript",
                "中文曲线排版 / مرحبا / ตั้ง",
                path: ${ crate::text::TextPath::new(paths.ids[1])
                    .with_range(Fixed::ZERO..path_window_end(model.stage_size().width)) },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font_stack: mixed_stack(),
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 17_u16 } else { 25_u16 }
                },
                text_color: ACCENT,
                paragraph: single_line(TextDirection::Auto)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_caption",
                "PATH REVISION > MEASURE > PLACE > FOUR BACKENDS",
                path: ${ crate::text::TextPath::new(paths.ids[2])
                    .with_range(Fixed::ZERO..path_window_end(model.stage_size().width)) },
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font: UI,
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 10_u16 } else { 14_u16 }
                },
                text_color: MUTED,
                paragraph: single_line(TextDirection::LeftToRight)
            ) [
                IgnoreHitTest,
            ]
        }
    }
}

#[compose]
fn compose_controls(model: model!(CurveModel)) -> Entity {
    ui! {
        Row (
            id: "curve_text_controls",
            width: Dimension::percent(100),
            height: @id(curve_text_stage).width {
                if curve_text_stage.width < Fixed::from_int(500) {
                    146
                } else if curve_text_stage.width < Fixed::from_int(720) {
                    104
                } else {
                    62
                }
            },
            min_height: @id(curve_text_stage).width {
                if curve_text_stage.width < Fixed::from_int(500) {
                    146
                } else if curve_text_stage.width < Fixed::from_int(720) {
                    104
                } else {
                    62
                }
            },
            padding: Padding {
                top: Dimension::px(10),
                right: Dimension::px(12),
                bottom: Dimension::px(10),
                left: Dimension::px(12),
            },
            wrap: FlexWrap::Wrap,
            align: AlignItems::Center,
            row_gap: 8,
            column_gap: 10,
            clip_children: true,
            bg_color: PANEL,
            border_color: BORDER,
            border_width: 1,
            border_radius: 10
        ) {
            Row (grow: 1.0, min_width: 250, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("Amplitude", width: 74, font: UI, font_size: 11, text_color: MUTED)
                Slider (
                    id: "curve_text_amplitude",
                    grow: 1.0,
                    min_width: 110,
                    height: 14,
                    min: Fixed::from_int(24),
                    max: Fixed::from_int(96),
                    value: ${ model.amplitude() },
                    track_color: BORDER,
                    fill_color: ACCENT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = event.old;
                    model.set_amplitude(event.new);
                }
                Text (
                    id: "curve_text_amplitude_value",
                    text: ${ format_args!("{}", model.amplitude().to_int()) },
                    capacity: 3,
                    width: 28,
                    font: UI,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: bounded_text(1)
                )
            }
            Row (grow: 1.0, min_width: 210, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("Speed", width: 46, font: UI, font_size: 11, text_color: MUTED)
                Slider (
                    id: "curve_text_speed",
                    grow: 1.0,
                    min_width: 100,
                    height: 14,
                    min: Fixed::from_int(20),
                    max: Fixed::from_int(120),
                    value: ${ model.speed() },
                    track_color: BORDER,
                    fill_color: ACCENT,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = event.old;
                    model.set_speed(event.new);
                }
                Text (
                    id: "curve_text_speed_value",
                    text: ${ format_args!("{}", model.speed().to_int()) },
                    capacity: 3,
                    width: 30,
                    font: UI,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: bounded_text(1)
                )
            }
            Row (grow: 1.0, min_width: 190, height: 34, column_gap: 8) {
                Button (
                    id: "curve_text_direction",
                    text: ${ if model.reversed() { "Reverse" } else { "Forward" } },
                    grow: 1.0,
                    min_width: 92,
                    height: 34,
                    normal_color: PANEL_ALT,
                    pressed_color: RAISED,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 8,
                    font: UI,
                    font_size: 12,
                    text_color: TEXT
                ) on Tap { model.toggle_direction(); }
                Button (
                    id: "curve_text_pause",
                    text: ${ if model.paused() { "Resume" } else { "Pause" } },
                    grow: 1.0,
                    min_width: 82,
                    height: 34,
                    normal_color: PANEL_ALT,
                    pressed_color: RAISED,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 8,
                    font: UI,
                    font_size: 12,
                    text_color: TEXT
                ) on Tap { model.toggle_paused(); }
            }
        }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(CurveModel), paths: CurvePaths) {
    //~focus-start
    ui! {
        Scroll (
            id: "curve_text_shell",
            grow: 1.0,
            clip_children: true,
            bg_color: BACKGROUND
        ) {
            Column (
                id: "curve_text_document",
                width: Dimension::percent(100),
                height: Dimension::Content,
                min_height: Dimension::percent(100),
                padding: Padding::all(22),
                row_gap: 12
            ) {
                compose_header ()
                compose_stage (model, paths)
                compose_controls (model)
            }
        }
    };
    //~focus-end
}
