use super::model::CounterModel;
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

const STEP: StylePreset = style!(grow: 1.0, height: 44, border_radius: 22);

fn label() -> StylePreset<Text> {
    style!(width: Dimension::percent(100), paragraph: ParagraphStyle::label())
}

#[compose]
pub fn build_widgets(counter: model!(CounterModel)) {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 6,
            font_size: 12,
            text_color: ColorToken::OnSurface
        ) {
            Text ("Count", style: label(), height: 16, text_color: ColorToken::OnSurfaceVariant)
            Text (
                style: label(),
                text: ${ format_args!("{}", counter.count()) },
                capacity: 11,
                height: 76,
                font_size: 48
            )
            Row (
                width: Dimension::percent(100),
                max_width: 208,
                height: 52,
                padding: Padding::all(4),
                column_gap: 4,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 26,
                font_size: 26
            ) {
                Button (
                    "-",
                    style: STEP,
                    normal_color: ColorToken::Outline,
                    pressed_color: ColorToken::SurfaceVariant
                ) on Tap { counter.decrement(); }
                Button (
                    "+",
                    style: STEP,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { counter.increment(); }
            }
        }
    };
    //~focus-end
}
