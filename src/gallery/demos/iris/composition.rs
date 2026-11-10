use super::lens::IrisLens;
use super::meters::{
    DialMeter, ExposureMeter, FOCUS_MARKS, FocusScale, Histogram, ShutterRing, focus_track,
};
use super::state::{
    CaptureMode, ExposureMode, FocusMode, IrisLayout, IrisState, MAX_STOP, third_label,
};
use super::style::{BG, BORDER, MUTED, OK, PANEL, RAISED, SEE, TEXT, label};
use crate::gallery::brand::MiruiMark;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::{Button, ButtonSize, Slider, Text, TextAlign};

/// Full stops printed under the aperture slider.
const APERTURE_MARKS: [&str; 8] = ["1.4", "2", "2.8", "4", "5.6", "8", "11", "16"];

fn lens_fit(width: Fixed, height: Fixed, inset: i32) -> Fixed {
    (width.min(height) - Fixed::from_int(inset)).max(Fixed::from_int(24))
}

/// Side of the lens on a round face.
fn dial_lens(width: Fixed, height: Fixed) -> Fixed {
    width.min(height) * 66 / 100
}

/// Space between the lens and the face edge, above or below it.
fn dial_margin(width: Fixed, height: Fixed) -> Fixed {
    ((height - dial_lens(width, height)) / 2).max(Fixed::ZERO)
}

fn focus_label(mark: i32) -> &'static str {
    match mark {
        50 => "0.5",
        100 => "1",
        200 => "2",
        500 => "5",
        _ => "INF",
    }
}

fn focus_left(mark: i32) -> Dimension {
    // Labels start a little before their tick so they read as centred on it.
    let position = focus_track(Fixed::from_int(mark)) * 100 - Fixed::from_int(2);
    Dimension::Percent(position.max(Fixed::ZERO).min(Fixed::from_int(95)))
}

/// Portrait stage height: a square lens view that leaves room for the controls.
fn portrait_stage(width: Fixed, height: Fixed) -> Fixed {
    (width - Fixed::from_int(32)).min(height * 40 / 100)
}

fn segment_fill(selected: bool) -> ColorToken {
    if selected { RAISED } else { BG }
}

fn segment_text(selected: bool) -> ColorToken {
    if selected { TEXT } else { MUTED }
}

/// File name prefix and extension of the latest capture.
fn file_parts(mode: CaptureMode) -> (&'static str, &'static str) {
    match mode {
        CaptureMode::Photo => ("IMG", "ARW"),
        CaptureMode::Video => ("CLIP", "MP4"),
    }
}

/// The saved notice drops in from above its stage, which clips it while parked.
#[compose]
fn compose_notice(model: model!(IrisState), width: i32) -> Entity {
    ui! {
        Row (
            position: Position::Absolute,
            left: 0,
            top: 14,
            width: Dimension::percent(100),
            height: 30,
            justify: JustifyContent::Center,
            transform: ${ Transform::translate(Fixed::ZERO, model.notice()) }
        ) {
            Row (
                width: width,
                height: 30,
                padding: Padding::all(6),
                column_gap: 6,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                bg_color: BG,
                border_color: BORDER,
                border_width: 1,
                border_radius: 15
            ) {
                Text ("✓", font_size: 12, text_color: OK, paragraph: label())
                Text ("Saved", font_size: 12, text_color: TEXT, paragraph: label())
                Text (
                    text: ${
                        let (prefix, extension) = file_parts(model.capture_mode());
                        format_args!("{prefix}_{:04}.{extension}", model.shots())
                    },
                    capacity: 13,
                    font_size: 12,
                    text_color: MUTED,
                    paragraph: label()
                )
            }
        }
    }
}

#[compose]
fn compose_brand(compact: bool) -> Entity {
    ui! {
        Row (
            width: if compact { 150 } else { 170 },
            height: 28,
            align: AlignItems::Center,
            column_gap: 8
        ) {
            View (width: if compact { 22 } else { 24 }, height: if compact { 22 } else { 24 }) [
                MiruiMark,
            ]
            Text (
                "Iris",
                font_size: if compact { 16 } else { 17 },
                text_color: TEXT,
                paragraph: label()
            )
            Text ("35mm f/1.4", font_size: 12, text_color: MUTED, paragraph: label())
        }
    }
}

#[compose]
fn compose_status(compact: bool) -> Entity {
    ui! {
        Row (
            width: if compact { 86 } else { 250 },
            height: 28,
            align: AlignItems::Center,
            justify: JustifyContent::FlexEnd,
            column_gap: 6
        ) {
            View (width: 7, height: 7, bg_color: OK, border_radius: 4)
            Text ("Connected", font_size: 12, text_color: MUTED, paragraph: label())
            if !compact {
                View (width: 14)
                Text ("RAW", font_size: 12, text_color: MUTED, paragraph: label())
                View (width: 14)
                Text ("Battery 82%", font_size: 12, text_color: MUTED, paragraph: label())
            }
        }
    }
}

/// The P / A / S / M dial as a segmented control.
#[compose]
fn compose_exposure_modes(model: model!(IrisState)) -> Entity {
    ui! {
        Row (
            width: 98,
            height: 24,
            padding: Padding::all(2),
            column_gap: 2,
            bg_color: BG,
            border_radius: 7
        ) {
            walk ExposureMode::ALL with mode {
                Button (
                    text: mode.label(),
                    size: ButtonSize::Compact,
                    width: 22,
                    height: 20,
                    normal_color: ${ segment_fill(model.exposure_mode() == mode) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font_size: 11,
                    text_color: ${ segment_text(model.exposure_mode() == mode) }
                ) on Tap { model.select_exposure(mode); }
            }
        }
    }
}

#[compose]
fn compose_focus_modes(model: model!(IrisState)) -> Entity {
    ui! {
        Row (
            width: 78,
            height: 24,
            padding: Padding::all(2),
            column_gap: 2,
            bg_color: BG,
            border_radius: 7
        ) {
            walk FocusMode::ALL with mode {
                Button (
                    text: mode.label(),
                    size: ButtonSize::Compact,
                    width: if mode == FocusMode::Single { 40 } else { 32 },
                    height: 20,
                    normal_color: ${ segment_fill(model.focus_mode() == mode) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font_size: 11,
                    text_color: ${ segment_text(model.focus_mode() == mode) }
                ) on Tap { model.select_focus(mode); }
            }
        }
    }
}

#[compose]
fn compose_capture_modes(model: model!(IrisState)) -> Entity {
    ui! {
        Row (
            width: 118,
            height: 24,
            padding: Padding::all(2),
            column_gap: 2,
            bg_color: BG,
            border_radius: 7
        ) {
            walk CaptureMode::ALL with mode {
                Button (
                    text: mode.label(),
                    size: ButtonSize::Compact,
                    width: 56,
                    height: 20,
                    normal_color: ${ segment_fill(model.capture_mode() == mode) },
                    pressed_color: RAISED,
                    border_width: 0,
                    border_radius: 5,
                    font_size: 11,
                    text_color: ${ segment_text(model.capture_mode() == mode) }
                ) on Tap { model.select_capture(mode); }
            }
        }
    }
}

#[compose]
fn compose_value(name: &'static str, value: &'static str, size: u16) -> Entity {
    ui! {
        Column (grow: 1.0, row_gap: 4) {
            Text (name, font_size: 11, text_color: MUTED, paragraph: label())
            Text (value, font_size: size, text_color: TEXT, paragraph: label())
        }
    }
}

#[compose]
fn compose_values(model: model!(IrisState), size: u16) -> Entity {
    ui! {
        Row (width: Dimension::percent(100), height: i32::from(size) + 22, column_gap: 8) {
            Column (grow: 1.0, row_gap: 4) {
                Text ("Aperture", font_size: 11, text_color: MUTED, paragraph: label())
                Text (
                    text: ${ format_args!("f/{}", third_label(model.aperture_third())) },
                    capacity: 6,
                    font_size: size,
                    text_color: TEXT,
                    paragraph: label()
                )
            }
            compose_value ("Shutter", "1/250", size)
            compose_value ("ISO", "100", size)
        }
    }
}

#[compose]
fn compose_meter(model: model!(IrisState)) -> Entity {
    ui! {
        Column (width: Dimension::percent(100), height: 44, row_gap: 2) {
            View (width: Dimension::percent(100), height: 30) [
                ExposureMeter {
                    model: model.clone(),
                },
            ]
            Row (height: 12, justify: JustifyContent::SpaceBetween, padding: Padding::all(2)) {
                Text ("-3", font_size: 10, text_color: MUTED, paragraph: label())
                Text ("0", font_size: 10, text_color: MUTED, paragraph: label())
                Text ("+3", font_size: 10, text_color: MUTED, paragraph: label())
            }
        }
    }
}

#[compose]
fn compose_divider() -> Entity {
    ui! {
        View (width: Dimension::percent(100), height: 1, bg_color: BORDER)
    }
}

/// Readouts and, when the window is tall enough, the level meter.
#[compose]
fn compose_exposure(model: model!(IrisState)) -> Entity {
    ui! {
        Column (
            width: Dimension::percent(100),
            height: ${ if model.roomy() { 164 } else { 100 } },
            padding: Padding::all(14),
            row_gap: 12
        ) {
            Row (height: 24, align: AlignItems::Center) {
                Text ("Exposure", grow: 1.0, font_size: 13, text_color: TEXT, paragraph: label())
                compose_exposure_modes (model)
            }
            compose_values (model, 22)
            if ${ model.roomy() } {
                compose_meter (model)
            }
        }
    }
}

#[compose]
fn compose_aperture(model: model!(IrisState)) -> Entity {
    ui! {
        Column (width: Dimension::percent(100), height: 94, padding: Padding::all(14), row_gap: 8) {
            Row (height: 20, align: AlignItems::Center) {
                Text ("Aperture", grow: 1.0, font_size: 13, text_color: TEXT, paragraph: label())
                Text (
                    text: ${ format_args!("f/{}", third_label(model.aperture_third())) },
                    capacity: 6,
                    font_size: 13,
                    text_color: MUTED,
                    paragraph: label()
                )
            }
            Slider (
                width: Dimension::percent(100),
                height: 18,
                min: Fixed::ZERO,
                max: Fixed::from_int(MAX_STOP),
                value: ${ model.stop() },
                track_color: BORDER,
                fill_color: SEE,
                thumb_color: TEXT
            ) on ValueChanged {
                let _ = event.old;
                model.set_stop(event.new);
            }
            Row (
                width: Dimension::percent(100),
                height: 12,
                justify: JustifyContent::SpaceBetween
            ) {
                walk APERTURE_MARKS with mark {
                    Text (mark, font_size: 10, text_color: MUTED, paragraph: label())
                }
            }
        }
    }
}

/// Fills the height its parent leaves for it.
#[compose]
fn compose_histogram(model: model!(IrisState)) -> Entity {
    ui! {
        Column (
            width: Dimension::percent(100),
            grow: 1.0,
            min_height: 64,
            padding: Padding::all(14),
            row_gap: 10
        ) {
            Row (height: 20, align: AlignItems::Center) {
                Text ("Histogram", grow: 1.0, font_size: 13, text_color: TEXT, paragraph: label())
                Text ("Luminance", font_size: 11, text_color: MUTED, paragraph: label())
            }
            View (width: Dimension::percent(100), grow: 1.0) [
                Histogram { model: model.clone() },
            ]
        }
    }
}

#[compose]
fn compose_focus(model: model!(IrisState)) -> Entity {
    ui! {
        Column (width: Dimension::percent(100), height: 98, padding: Padding::all(14), row_gap: 8) {
            Row (height: 24, align: AlignItems::Center, column_gap: 10) {
                Text ("Focus", grow: 1.0, font_size: 13, text_color: TEXT, paragraph: label())
                Text (
                    text: ${
                        format_args!(
                            "{}.{} m", model.focus().to_int(), (model.focus().fract() * 10).to_int()
                        )
                    },
                    capacity: 8,
                    font_size: 12,
                    text_color: MUTED,
                    paragraph: label()
                )
                compose_focus_modes (model)
            }
            View (width: Dimension::percent(100), height: 16) [
                FocusScale { model: model.clone() },
            ]
            View (width: Dimension::percent(100), height: 12) {
                walk FOCUS_MARKS with mark {
                    Text (
                        focus_label(mark),
                        position: Position::Absolute,
                        left: focus_left(mark),
                        top: 0,
                        font_size: 10,
                        text_color: MUTED,
                        paragraph: label()
                    )
                }
            }
        }
    }
}

#[compose]
fn compose_capture(model: model!(IrisState), size: i32) -> Entity {
    ui! {
        Row (
            width: Dimension::percent(100),
            height: size + 16,
            padding: Padding::all(8),
            align: AlignItems::Center,
            justify: JustifyContent::SpaceBetween
        ) {
            compose_capture_modes (model)
            View (width: size, height: size) [
                ShutterRing {
                    model: model.clone(),
                },
            ] on Tap { model.capture(); }
            Column (width: 92, align: AlignItems::FlexEnd, row_gap: 2) {
                Text (
                    text: ${
                        match model.capture_mode() {
                            CaptureMode::Photo => "1,284",
                            CaptureMode::Video => "42 min",
                        }
                    },
                    font_size: 14,
                    text_color: TEXT,
                    paragraph: label()
                )
                Text (
                    text: ${
                        match model.capture_mode() {
                            CaptureMode::Photo => "shots left",
                            CaptureMode::Video => "recording left",
                        }
                    },
                    font_size: 11,
                    text_color: MUTED,
                    paragraph: label()
                )
            }
        }
    }
}

#[compose]
fn compose_micro(model: model!(IrisState)) -> Entity {
    ui! {
        Column (
            id: "iris_micro",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            padding: Padding::all(4),
            row_gap: 2,
            align: AlignItems::Center
        ) {
            View (
                id: "iris_lens_micro",
                width: @(id(iris_micro).width, id(iris_micro).height) {
                    lens_fit(
                        iris_micro.width - Fixed::from_int(8),
                        iris_micro.height - Fixed::from_int(44),
                        0,
                    )
                },
                height: @(id(iris_micro).width, id(iris_micro).height) {
                    lens_fit(
                        iris_micro.width - Fixed::from_int(8),
                        iris_micro.height - Fixed::from_int(44),
                        0,
                    )
                }
            ) [
                IrisLens::new(model.clone()),
            ] on Tap { model.capture(); }
            Row (
                width: Dimension::percent(100),
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::SpaceBetween,
                padding: Padding::all(3)
            ) {
                Text (
                    text: ${ format_args!("f/{}", third_label(model.aperture_third())) },
                    capacity: 6,
                    font_size: 20,
                    text_color: TEXT,
                    paragraph: label()
                )
                Column (align: AlignItems::FlexEnd, row_gap: 1) {
                    Text ("1/250", font_size: 10, text_color: TEXT, paragraph: label())
                    Text ("ISO 100", font_size: 10, text_color: MUTED, paragraph: label())
                }
            }
        }
    }
}

#[compose]
fn compose_dial(model: model!(IrisState)) -> Entity {
    ui! {
        View (
            id: "iris_dial",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            align: AlignItems::Center,
            justify: JustifyContent::Center
        ) [
            DialMeter {
                model: model.clone(),
            },
        ] {
            View (
                id: "iris_lens_dial",
                width: @(id(iris_dial).width, id(iris_dial).height) {
                    dial_lens(iris_dial.width, iris_dial.height)
                },
                height: @(id(iris_dial).width, id(iris_dial).height) {
                    dial_lens(iris_dial.width, iris_dial.height)
                }
            ) [
                IrisLens::new(model.clone()),
            ] on Tap { model.capture(); }
            Text (
                text: ${ format_args!("f/{}", third_label(model.aperture_third())) },
                capacity: 6,
                position: Position::Absolute,
                left: 0,
                top: @(id(iris_dial).width, id(iris_dial).height) {
                    dial_margin(iris_dial.width, iris_dial.height) / 2 - Fixed::from_int(12)
                },
                width: Dimension::percent(100),
                height: Dimension::Content,
                font_size: 18,
                text_color: TEXT,
                paragraph: label().with_align(TextAlign::Center)
            )
            Text (
                "1/250 · ISO 100",
                position: Position::Absolute,
                left: 0,
                top: @(id(iris_dial).width, id(iris_dial).height) {
                    iris_dial.height - dial_margin(iris_dial.width, iris_dial.height) / 2
                        - Fixed::from_int(16)
                },
                width: Dimension::percent(100),
                height: Dimension::Content,
                font_size: 10,
                text_color: MUTED,
                paragraph: label().with_align(TextAlign::Center)
            )
        }
    }
}

#[compose]
fn compose_portrait(model: model!(IrisState)) -> Entity {
    ui! {
        Column (
            id: "iris_portrait",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            padding: Padding::all(16),
            row_gap: 12
        ) {
            Row (
                width: Dimension::percent(100),
                height: 32,
                align: AlignItems::Center,
                justify: JustifyContent::SpaceBetween
            ) {
                compose_brand (true)
                compose_status (true)
            }
            View (
                id: "iris_portrait_stage",
                width: Dimension::percent(100),
                height: @(id(iris_portrait).width, id(iris_portrait).height) {
                    portrait_stage(iris_portrait.width, iris_portrait.height)
                },
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                View (
                    id: "iris_lens_portrait",
                    width: @(id(iris_portrait).width, id(iris_portrait).height) {
                        portrait_stage(iris_portrait.width, iris_portrait.height) - Fixed::from_int(36)
                    },
                    height: @(id(iris_portrait).width, id(iris_portrait).height) {
                        portrait_stage(iris_portrait.width, iris_portrait.height) - Fixed::from_int(36)
                    }
                ) [
                    IrisLens::new(model.clone()),
                ]
                compose_notice (model, 200)
            }
            Column (
                width: Dimension::percent(100),
                height: 124,
                padding: Padding::all(14),
                row_gap: 12,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 14
            ) {
                compose_values (model, 20)
                compose_meter (model)
            }
            Column (
                width: Dimension::percent(100),
                height: 112,
                bg_color: PANEL,
                border_color: BORDER,
                border_width: 1,
                border_radius: 14
            ) {
                compose_histogram (model)
            }
            View (grow: 1.0)
            compose_capture (model, 68)
        }
    }
}

#[compose]
fn compose_landscape(model: model!(IrisState)) -> Entity {
    ui! {
        Column (
            id: "iris_landscape",
            width: Dimension::percent(100),
            height: Dimension::percent(100)
        ) {
            Row (
                width: Dimension::percent(100),
                height: 48,
                padding: Padding::all(12),
                align: AlignItems::Center,
                justify: JustifyContent::SpaceBetween
            ) {
                compose_brand (false)
                compose_status (false)
            }
            compose_divider ()
            Row (
                width: Dimension::percent(100),
                grow: 1.0,
                padding: Padding::all(14),
                column_gap: 14
            ) {
                View (
                    id: "iris_landscape_stage",
                    grow: 1.0,
                    height: Dimension::percent(100),
                    align: AlignItems::Center,
                    justify: JustifyContent::Center,
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14,
                    clip_children: true
                ) {
                    View (
                        id: "iris_lens_landscape",
                        width: @(id(iris_landscape_stage).width, id(iris_landscape_stage).height) {
                            lens_fit(iris_landscape_stage.width, iris_landscape_stage.height, 72)
                        },
                        height: @(id(iris_landscape_stage).width, id(iris_landscape_stage).height) {
                            lens_fit(iris_landscape_stage.width, iris_landscape_stage.height, 72)
                        }
                    ) [
                        IrisLens::new(model.clone()),
                    ]
                    compose_notice (model, 210)
                }
                Column (
                    width: @id(iris_landscape).width {
                        if iris_landscape.width < Fixed::from_int(1000) { 300 } else { 340 }
                    },
                    height: Dimension::percent(100),
                    bg_color: PANEL,
                    border_color: BORDER,
                    border_width: 1,
                    border_radius: 14,
                    clip_children: true
                ) {
                    compose_exposure (model)
                    compose_divider ()
                    compose_aperture (model)
                    compose_divider ()
                    compose_histogram (model)
                    // Short windows drop the focus section rather than squeeze it.
                    if ${ model.roomy() } {
                        compose_divider ()
                        compose_focus (model)
                    }
                    compose_divider ()
                    compose_capture (model, 60)
                }
            }
        }
    }
}

#[compose]
pub(super) fn build_widgets(model: model!(IrisState)) {
    //~focus-start
    ui! {
        View (
            id: "iris_shell",
            width: Dimension::percent(100),
            height: Dimension::percent(100),
            bg_color: BG,
            clip_children: true
        ) {
            match ${ model.layout() } {
                IrisLayout :: Micro => {
                    compose_micro (model)
                }
                IrisLayout :: Dial => {
                    compose_dial (model)
                }
                IrisLayout :: Portrait => {
                    compose_portrait (model)
                }
                IrisLayout :: Landscape => {
                    compose_landscape (model)
                }
            }
        } on SizeChanged { model.set_viewport(event.new.width, event.new.height); }
    };
    //~focus-end
}
