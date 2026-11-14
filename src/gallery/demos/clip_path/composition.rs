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
                "Clip path",
                height: 28,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "One boundary, four layers",
                width: Dimension::percent(100),
                height: 18,
                font_size: 12,
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
