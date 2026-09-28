use super::render::FillRules;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::FlexEnd,
            padding: Padding::all(10)
        ) {
            Row (height: 24) {
                Text (
                    "EvenOdd",
                    width: 92,
                    height: 22,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "NonZero",
                    width: 92,
                    height: 22,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label()
                )
            }
            FillRules (
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: Dimension::percent(100),
                height: Dimension::percent(100)
            )
        }
    };
}
