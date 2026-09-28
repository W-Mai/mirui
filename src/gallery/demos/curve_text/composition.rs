use alloc::format;

use super::geometry::CurvePaths;
use super::runtime::{bind_curve_paths, bind_stage_layout};
use super::stage::CurveStage;
use super::state::{CurveAction, CurveModel, CurveNodes};
use super::style::{
    BACKGROUND, BORDER, CYAN, GOLD, MUTED, PANEL, PANEL_ALT, ROUTE_LABEL, TEXT, UI, VIOLET,
    bounded_text, mixed_stack, single_line,
};
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
                View (width: 8, height: 38, bg_color: CYAN, border_radius: 4)
                Column (grow: 1.0, min_width: 150, row_gap: 2) {
                    Text (
                        "KINETIC TYPE",
                        width: Dimension::percent(100),
                        height: 34,
                        font: UI,
                        font_size: 25,
                        text_color: TEXT,
                        paragraph: bounded_text(1)
                    )
                    Text (
                        "One shaped run / one retained path / continuous pose",
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
                width: 248,
                min_width: 170,
                max_width: 248,
                height: 30,
                bg_color: PANEL_ALT,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15,
                font: UI,
                font_size: 10,
                text_color: CYAN,
                paragraph: ParagraphStyle::label()
            )
        }
    }
}

#[compose]
fn compose_stage(paths: CurvePaths) -> Entity {
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
            border_radius: 18
        ) {
            CurveStage (
                id: "curve_text_stage",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_primary",
                "MIRUI / BEND SPACE, NOT GLYPHS",
                path: crate::text::TextPath::new(paths.ids[0])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
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
                path: crate::text::TextPath::new(paths.ids[1])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font_stack: mixed_stack(),
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 17_u16 } else { 25_u16 }
                },
                text_color: CYAN,
                paragraph: single_line(TextDirection::Auto)
            ) [
                IgnoreHitTest,
            ]
            Text (
                id: "curve_text_caption",
                "PATH REVISION > MEASURE > PLACE > FOUR BACKENDS",
                path: crate::text::TextPath::new(paths.ids[2])
                    .with_range(Fixed::ZERO..Fixed::from_int(820)),
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100),
                font: UI,
                font_size: @id(curve_text_stage).width {
                    if curve_text_stage.width < Fixed::from_int(520) { 10_u16 } else { 14_u16 }
                },
                text_color: GOLD,
                paragraph: single_line(TextDirection::LeftToRight)
            ) [
                IgnoreHitTest,
            ]
        }
    }
}

#[compose]
fn compose_controls() -> Entity {
    let model = cx
        .world_mut()
        .resource::<CurveModel>()
        .cloned()
        .expect("Curve Text model");
    let amplitude_value = model.amplitude;
    let speed_value = model.speed;
    let direction_label = model.reversed;
    let paused_label = model.paused;

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
            border_radius: 14
        ) {
            Row (grow: 1.0, min_width: 250, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("AMPLITUDE", width: 74, font: UI, font_size: 10, text_color: MUTED)
                Slider (
                    id: "curve_text_amplitude",
                    grow: 1.0,
                    min_width: 110,
                    height: 14,
                    min: Fixed::from_int(24),
                    max: Fixed::from_int(96),
                    value: Fixed::from_int(68),
                    track_color: BORDER,
                    fill_color: CYAN,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = old;
                    CurveAction::SetAmplitude(*new).publish(ctx.world);
                }
                Text (
                    text: ${ format!("{}", amplitude_value.get().to_int()) },
                    width: 28,
                    font: UI,
                    font_size: 11,
                    text_color: TEXT,
                    paragraph: bounded_text(1)
                )
            }
            Row (grow: 1.0, min_width: 210, height: 34, align: AlignItems::Center, column_gap: 8) {
                Text ("SPEED", width: 46, font: UI, font_size: 10, text_color: MUTED)
                Slider (
                    id: "curve_text_speed",
                    grow: 1.0,
                    min_width: 100,
                    height: 14,
                    min: Fixed::from_int(20),
                    max: Fixed::from_int(120),
                    value: Fixed::from_int(64),
                    track_color: BORDER,
                    fill_color: VIOLET,
                    thumb_color: TEXT
                ) on ValueChanged {
                    let _ = old;
                    CurveAction::SetSpeed(*new).publish(ctx.world);
                }
                Text (
                    text: ${ format!("{}", speed_value.get().to_int()) },
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
                    text: ${ if direction_label.get() { "REVERSE" } else { "FORWARD" } },
                    grow: 1.0,
                    min_width: 92,
                    height: 34,
                    normal_color: PANEL_ALT,
                    pressed_color: VIOLET,
                    border_color: VIOLET,
                    border_width: 1,
                    border_radius: 10,
                    font: UI,
                    font_size: 10,
                    text_color: VIOLET
                ) on Tap { CurveAction::ToggleDirection.publish(ctx.world); }
                Button (
                    id: "curve_text_pause",
                    text: ${ if paused_label.get() { "RESUME" } else { "PAUSE" } },
                    grow: 1.0,
                    min_width: 82,
                    height: 34,
                    normal_color: PANEL_ALT,
                    pressed_color: CYAN,
                    border_color: CYAN,
                    border_width: 1,
                    border_radius: 10,
                    font: UI,
                    font_size: 10,
                    text_color: CYAN
                ) on Tap { CurveAction::TogglePaused.publish(ctx.world); }
            }
        }
    }
}

#[compose]
pub(super) fn build_widgets(paths: CurvePaths) {
    let model = cx
        .world_mut()
        .resource::<CurveModel>()
        .cloned()
        .expect("Curve Text model");
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
                compose_stage (paths)
                compose_controls ()
            }
        }
    };
    let nodes = CurveNodes {
        stage: cx
            .world_mut()
            .find_by_id("curve_text_stage")
            .expect("Curve Text stage"),
        primary: cx
            .world_mut()
            .find_by_id("curve_text_primary")
            .expect("Curve Text primary text"),
        multiscript: cx
            .world_mut()
            .find_by_id("curve_text_multiscript")
            .expect("Curve Text multiscript text"),
        caption: cx
            .world_mut()
            .find_by_id("curve_text_caption")
            .expect("Curve Text caption"),
    };
    bind_curve_paths(cx, nodes.stage, paths, &model);
    bind_stage_layout(cx, nodes, paths, model);
    cx.world_mut().insert_resource(nodes);
    //~focus-end
}
