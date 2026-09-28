use super::view::Shapes;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let now_ms = cx
        .world_mut()
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);

    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(12),
            bg_color: ColorToken::Surface
        ) {
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                max_width: 440,
                max_height: 296,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: Dimension::percent(100),
                    height: 38,
                    padding: Padding {
                        left: Dimension::px(14),
                        right: Dimension::px(14),
                        ..Default::default()
                    },
                    align: AlignItems::Center
                ) {
                    Text (
                        "VECTOR CLOCK",
                        grow: 1.0,
                        font_size: 14,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "ARC · LINE",
                        width: 88,
                        height: 22,
                        font_size: 9,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Secondary,
                        border_color: ColorToken::Secondary,
                        border_width: 1,
                        border_radius: 11,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Shapes (
                    start_ms: now_ms,
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100)
                )
            }
        }
    };
    //~focus-end
}
