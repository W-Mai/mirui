use super::view::Shapes;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    let now_ms = res!(optional MonoClock).with(|clock| clock.map_or(0, |clock| clock.now_ms()));

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
                max_height: 296
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
                        "Chronograph",
                        grow: 1.0,
                        font_size: 16,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Tap to change the dial",
                        width: 142,
                        height: 24,
                        font_size: 12,
                        bg_color: ColorToken::SurfaceVariant,
                        text_color: ColorToken::OnSurfaceVariant,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Shapes (
                    start_ms: now_ms,
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: Dimension::percent(100)
                ) on Tap(|event: event!(Tap), rows: query!(Shapes), commands: commands!()| {
                    let next = rows.get(event.target).copied().map(Shapes::next_dial);
                    if let Some(next) = next {
                        commands.insert(event.target, next);
                    }
                })
            }
        }
    };
    //~focus-end
}
