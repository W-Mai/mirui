use super::motion::{DriftX, DriftY, Flap};
use super::state::Butterfly;
use crate::anim::{Motion, PlayMode, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

/// A full turn, from 0 to 360 degrees, every `period_ms`, forever.
fn turn_every(period_ms: u16) -> Motion {
    Tween::new(
        Fixed::ZERO,
        Fixed::from_int(360),
        period_ms,
        &ease::LINEAR,
        PlayMode::Loop,
    )
    .into()
}

#[compose]
pub fn build_widgets() {
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
                    "Flight study",
                    grow: 1.0,
                    font_size: 16,
                    text_color: ColorToken::OnSurface,
                    paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                )
                Text (
                    "Vector, live",
                    width: 96,
                    height: 24,
                    font_size: 12,
                    bg_color: ColorToken::SurfaceVariant,
                    text_color: ColorToken::OnSurfaceVariant,
                    border_radius: 12,
                    paragraph: ParagraphStyle::label()
                )
            }
            Butterfly (grow: 1.0, width: Dimension::percent(100)) [
                Flap(turn_every(760)),
                DriftX(turn_every(6200)),
                DriftY(turn_every(3800)),
            ]
        }
    };
    //~focus-end
}
