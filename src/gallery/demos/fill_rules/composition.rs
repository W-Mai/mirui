use super::render::FillRules;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 6,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "Fill rules",
                height: 28,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "The same star, filled two ways",
                height: 18,
                font_size: 12,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            FillRules (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
            Row (width: Dimension::percent(100), height: 20) {
                Text (
                    "Even-odd",
                    grow: 1.0,
                    height: 20,
                    font_size: 13,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "Non-zero",
                    grow: 1.0,
                    height: 20,
                    font_size: 13,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    };
}
