use super::card::Card;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(12),
            direction: FlexDirection::Column,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            row_gap: 8
        ) {
            Text (
                "NAMED SLOTS",
                width: Dimension::percent(100),
                max_width: 420,
                height: 24,
                font_size: 18,
                text_color: ColorToken::OnSurface
            )
            Card (width: Dimension::percent(100), max_width: 420, height: 140) {
                @header {
                    Text (
                        "ALERT",
                        grow: 1.0,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                }
                @body {
                    Column (direction: FlexDirection::Column, grow: 1.0) {
                        Text (
                            "@header @body @footer",
                            text_color: ColorToken::OnSurface
                        )
                        Text (
                            "each pick their own slot.",
                            text_color: ColorToken::OnSurface
                        )
                    }
                }
                @footer {
                    Text (
                        "FOOTER SLOT",
                        font_size: 11,
                        text_color: ColorToken::OnSurfaceVariant
                    )
                }
            }
            Card (width: Dimension::percent(100), max_width: 420, height: 92) {
                @body {
                    Column (direction: FlexDirection::Column, grow: 1.0) {
                        Text (
                            "body only. header uses",
                            text_color: ColorToken::OnSurface
                        )
                        Text (
                            "its @@ fallback content.",
                            text_color: ColorToken::OnSurface
                        )
                    }
                }
            }
        }
    };
}
