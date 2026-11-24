use crate::prelude::*;
use crate::ui::widgets::{Button, ParagraphStyle, Text};

const STEP: StylePreset = style!(grow: 1.0, height: 44, border_radius: 22);

#[compose]
pub fn build_widgets(count: model!(Signal<i32>)) {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(20),
            row_gap: 14
        ) {
            Text (
                text: ${ format_args!("Saved  {}", count.get()) },
                capacity: "Saved  ".len() + 11,
                width: Dimension::percent(100),
                max_width: 260,
                height: 56,
                font_size: 26,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label()
            )
            Row (
                width: Dimension::percent(100),
                max_width: 188,
                height: 52,
                padding: Padding::all(4),
                column_gap: 4,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 26,
                font_size: 26
            ) {
                Button (
                    "-",
                    style: STEP,
                    normal_color: ColorToken::Outline,
                    pressed_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurface
                ) on Tap { count.update(|n| *n -= 1); }
                Button (
                    "+",
                    style: STEP,
                    normal_color: ColorToken::Primary,
                    pressed_color: ColorToken::Secondary,
                    text_color: ColorToken::OnPrimary
                ) on Tap { count.update(|n| *n += 1); }
            }
            Text (
                "Persists across reloads and restarts",
                width: Dimension::percent(100),
                max_width: 280,
                height: 34,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
        }
    };
    //~focus-end
}
