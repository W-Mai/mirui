use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

//~focus-start
ui!(compose Card {
    Column (
        direction: FlexDirection::Column,
        grow: 1.0,
        border_radius: 12,
        clip_children: true
    ) {
        View (
            bg_color: ColorToken::Primary,
            padding: Padding::all(8),
            height: 32
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
            bg_color: ColorToken::SurfaceVariant,
            padding: Padding::all(12),
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
            bg_color: ColorToken::Surface,
            padding: Padding::all(8),
            direction: FlexDirection::Row,
            height: 32
        ) {
            @@footer
        }
    }
});
//~focus-end
