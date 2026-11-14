use super::render::Field;
use super::state::Orbit;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets(orbit: model!(Orbit)) {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(12),
            bg_color: ColorToken::Surface
        ) {
            Row (
                width: Dimension::percent(100),
                height: 34,
                padding: Padding {
                    left: Dimension::px(6),
                    right: Dimension::px(6),
                    ..Default::default()
                },
                align: AlignItems::Center
            ) {
                Text (
                    "Three-body field",
                    grow: 1.0,
                    font_size: 16,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "Fixed, live",
                    width: 96,
                    height: 24,
                    font_size: 12,
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurfaceVariant,
                    border_radius: 12,
                    paragraph: ParagraphStyle::label()
                )
            }
            View (grow: 1.0, width: Dimension::percent(100)) [
                Field { orbit: orbit.clone() },
            ]
        }
    };
    //~focus-end
}
