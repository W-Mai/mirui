use super::model::CounterModel;
use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

#[compose(bind(counter))]
pub fn build_widgets(counter: CounterModel) {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 14
        ) {
            Text (
                text: ${ alloc::format!("COUNT  {}", counter.count()) },
                width: Dimension::percent(100),
                max_width: 260,
                height: 56,
                font_size: 26,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 180,
                height: 44,
                column_gap: 12
            ) {
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
                    normal_color: ColorToken::Error,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnPrimary
                ) [
                    Text::label("−"),
                ] on Tap { counter.decrement(); }
                Button (
                    grow: 1.0,
                    height: 44,
                    border_radius: 12,
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
