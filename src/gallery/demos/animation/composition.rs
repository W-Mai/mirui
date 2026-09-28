use super::motion::{AnimateColor, AnimateX};
use crate::anim::{PlayMode, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(12)
        ) {
            View (
                id: "animation_stage",
                width: Dimension::percent(100),
                max_width: 320,
                height: 144,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    id: "animation_header",
                    height: 52,
                    align: AlignItems::Center,
                    padding: Padding {
                        top: Dimension::px(12),
                        right: Dimension::px(16),
                        bottom: Dimension::px(12),
                        left: Dimension::px(16),
                    }
                ) {
                    Text (
                        "TWEEN MOTION",
                        grow: 1.0,
                        font_size: 16,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "PING · PONG",
                        width: 100,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Primary,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                }
                View (
                    position: Position::Absolute,
                    left: 18,
                    top: 85,
                    width: 264,
                    height: 4,
                    bg_color: ColorToken::Outline,
                    border_radius: 2
                )
                View (
                    bg_color: Color::rgb(255, 86, 139),
                    border_color: Color::rgb(255, 174, 206),
                    border_width: 2,
                    border_radius: 18,
                    position: Position::Absolute,
                    left: 18,
                    top: 70,
                    width: 36,
                    height: 36
                ) [
                    AnimateX(
                        Tween::new(
                                Fixed::from_int(18),
                                Fixed::from_int(246),
                                1200,
                                ease::ease_in_out_cubic,
                                PlayMode::PingPong,
                            )
                            .into(),
                    ),
                    AnimateColor(
                        Tween::new(Fixed::ZERO, Fixed::ONE, 2400, ease::ease_in_out_quad, PlayMode::Loop)
                            .into(),
                    ),
                ]
            }
        }
    };
    //~focus-end
}
