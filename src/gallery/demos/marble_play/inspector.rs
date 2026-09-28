use super::style::{MUTED, TEXT};
use crate::gallery::play::marble::{MarbleModel, PAD_PITCHES};
use crate::input::event::scroll::TouchAction;
use crate::prelude::*;
use crate::types::Fixed64;
use crate::ui::widgets::{Button, ButtonSize, ParagraphStyle, Slider, Text, TextAlign};

fn pitch_label(pitch: u8) -> &'static str {
    const LABELS: [&str; 10] = ["C4", "D4", "E4", "G4", "A4", "C5", "D5", "E5", "G5", "A5"];
    PAD_PITCHES
        .iter()
        .position(|candidate| *candidate == pitch)
        .map(|index| LABELS[index])
        .unwrap_or("NOTE")
}

#[compose(bind(model))]
pub(super) fn marble_inspector(model: MarbleModel) -> Entity {
    ui! {
        View (
            id: "marble_inspector",
            visible: ${ model.inspector() },
            position: Position::Absolute,
            left: 0,
            top: 0,
            width: 480,
            height: 286,
            bg_color: Color::rgba(6, 12, 8, 150)
        ) [
            TouchAction::None,
        ] on Tap { }
        {
            Column (
                position: Position::Absolute,
                left: 213,
                top: 35,
                width: 253,
                height: 247,
                padding: Padding::all(10),
                row_gap: 4,
                bg_color: Color::rgb(34, 47, 37),
                border_color: Color::rgb(217, 248, 138),
                border_width: 1,
                border_radius: 9
            ) {
                Row (height: 22, align: AlignItems::Center) {
                    Text (
                        "PAD PROPERTIES",
                        grow: 1.0,
                        height: 20,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "X",
                        id: "marble_inspector_close",
                        size: ButtonSize::Compact,
                        width: 24,
                        height: 22,
                        font_size: 10,
                        normal_color: Color::rgb(45, 61, 48),
                        pressed_color: Color::rgb(217, 248, 138),
                        text_color: TEXT,
                        border_radius: 6
                    ) on Tap { model.set_inspector(false); }
                }
                Text (
                    "COLOR",
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Row (height: 20, column_gap: 8) {
                    Button (
                        "",
                        size: ButtonSize::Custom,
                        grow: 1.0,
                        height: 20,
                        normal_color: Color::rgb(180, 234, 189),
                        pressed_color: Color::rgb(180, 234, 189),
                        border_radius: 5
                    ) on Tap { model.set_color(0); }
                    Button (
                        "",
                        size: ButtonSize::Custom,
                        grow: 1.0,
                        height: 20,
                        normal_color: Color::rgb(198, 176, 239),
                        pressed_color: Color::rgb(198, 176, 239),
                        border_radius: 5
                    ) on Tap { model.set_color(1); }
                    Button (
                        "",
                        size: ButtonSize::Custom,
                        grow: 1.0,
                        height: 20,
                        normal_color: Color::rgb(238, 217, 132),
                        pressed_color: Color::rgb(238, 217, 132),
                        border_radius: 5
                    ) on Tap { model.set_color(2); }
                    Button (
                        "",
                        size: ButtonSize::Custom,
                        grow: 1.0,
                        height: 20,
                        normal_color: Color::rgb(238, 172, 139),
                        pressed_color: Color::rgb(238, 172, 139),
                        border_radius: 5
                    ) on Tap { model.set_color(3); }
                    Button (
                        "",
                        size: ButtonSize::Custom,
                        grow: 1.0,
                        height: 20,
                        normal_color: Color::rgb(160, 210, 232),
                        pressed_color: Color::rgb(160, 210, 232),
                        border_radius: 5
                    ) on Tap { model.set_color(4); }
                }
                Row (height: 24, align: AlignItems::Center, column_gap: 6) {
                    Text (
                        "PITCH",
                        width: 50,
                        height: 18,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        "-",
                        id: "marble_pitch_down",
                        size: ButtonSize::Compact,
                        width: 34,
                        height: 22,
                        font_size: 10,
                        normal_color: Color::rgb(45, 61, 48),
                        pressed_color: Color::rgb(217, 248, 138),
                        text_color: TEXT,
                        border_radius: 5
                    ) on Tap { model.adjust_pitch(-1); }
                    Text (
                        text: ${ pitch_label(model.selected_pitch()) },
                        text_capacity: 4,
                        id: "marble_pitch",
                        grow: 1.0,
                        height: 18,
                        font_size: 9,
                        text_color: TEXT,
                        paragraph: ParagraphStyle::label()
                    )
                    Button (
                        "+",
                        id: "marble_pitch_up",
                        size: ButtonSize::Compact,
                        width: 34,
                        height: 22,
                        font_size: 10,
                        normal_color: Color::rgb(45, 61, 48),
                        pressed_color: Color::rgb(217, 248, 138),
                        text_color: TEXT,
                        border_radius: 5
                    ) on Tap { model.adjust_pitch(1); }
                }
                Row (height: 24, align: AlignItems::Center, column_gap: 6) {
                    Text (
                        "TIMBRE",
                        width: 50,
                        height: 18,
                        font_size: 8,
                        text_color: MUTED,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Button (
                        text: ${ model.selected_timbre().label() },
                        text_capacity: 6,
                        id: "marble_timbre",
                        size: ButtonSize::Compact,
                        grow: 1.0,
                        height: 22,
                        font_size: 8,
                        normal_color: Color::rgb(45, 61, 48),
                        pressed_color: Color::rgb(198, 176, 239),
                        text_color: TEXT,
                        border_radius: 5
                    ) on Tap { model.cycle_timbre(); }
                }
                Text (
                    "BOUNCE",
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Slider (
                    id: "marble_bounce",
                    height: 20,
                    min: Fixed::from_ratio(7, 10),
                    max: Fixed::from_ratio(14, 10),
                    value: ${ model.selected_bounce().to_fixed() },
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(217, 248, 138),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { model.set_bounce(Fixed64::from_fixed(*new)); }
                Text (
                    "RADIUS",
                    height: 12,
                    font_size: 8,
                    text_color: MUTED,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Slider (
                    id: "marble_radius",
                    height: 20,
                    min: Fixed::from_int(13),
                    max: Fixed::from_int(23),
                    value: ${ model.selected_radius().to_fixed() },
                    track_color: Color::rgb(69, 87, 70),
                    fill_color: Color::rgb(217, 248, 138),
                    thumb_color: Color::rgb(225, 233, 214)
                ) on ValueChanged { model.set_radius(Fixed64::from_fixed(*new)); }
            }
        }
    }
}
