use super::render::ClipPath;
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
                "CLIP PATH",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "one vector boundary, four semantic layers",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            ClipPath (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
        }
    };
}
