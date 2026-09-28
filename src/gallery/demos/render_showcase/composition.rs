use super::render::RenderShowcase;
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
                "RENDER SHOWCASE",
                height: 28,
                font_size: 18,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Text (
                "gradient · clip · blur · fill-rule · stroke · transform",
                height: 20,
                font_size: 11,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            RenderShowcase (
                grow: 1.0,
                width: Dimension::percent(100),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            )
        }
    };
}
