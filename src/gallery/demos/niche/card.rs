use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

//~focus-start
ui!(compose Card {
    Column (
        direction: FlexDirection::Column,
        grow: 1.0,
        border_radius: 18,
        bg_color: ColorToken::SurfaceVariant,
        padding: Padding::all(8),
        row_gap: 6
    ) {
        View (
            bg_color: ColorToken::Primary,
            padding: Padding::all(8),
            height: 32,
            border_radius: 12
        ) {
            @@header {
                Text (
                    "(untitled)",
                    grow: 1.0,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
            }
        }
        View (
            padding: Padding::all(8),
            direction: FlexDirection::Column,
            grow: 1.0
        ) {
            @@body {
                Text (
                    "no body yet",
                    text_color: ColorToken::OnSurfaceVariant
                )
            }
        }
        View (
            padding: Padding::all(8),
            direction: FlexDirection::Row,
            height: 28
        ) {
            @@footer
        }
    }
});
//~focus-end
