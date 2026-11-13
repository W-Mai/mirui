use super::model::CounterModel;
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose]
pub fn build_widgets(counter: model!(CounterModel)) {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 6
        ) {
            Text (
                text: "Count",
                width: Dimension::percent(100),
                height: 16,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
            Text (
                text: ${ alloc::format!("{}", counter.count()) },
                width: Dimension::percent(100),
                height: 76,
                font_size: 48,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 208,
                height: 52,
                padding: Padding::all(4),
                column_gap: 4,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 26
            ) {
                Button (
                    font_size: 26,
                    grow: 1.0,
                    height: 44,
                    border_radius: 22,
                    normal_color: ColorToken::Outline,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface
                ) [
                    Text::label("-"),
                ] on Tap { counter.decrement(); }
                Button (
                    font_size: 26,
                    grow: 1.0,
                    height: 44,
                    border_radius: 22,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("+"),
                ] on Tap { counter.increment(); }
            }
        }
    };
    //~focus-end
}
