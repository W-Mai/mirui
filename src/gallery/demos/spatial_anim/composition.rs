use super::motion::{AnimateTweenY, SpringBall};
use crate::anim::{BOUNCY, PlayMode, SMOOTH, Spring, Tween, ease};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            padding: Padding::all(10)
        ) {
            View (
                id: "spatial_animation_stage",
                width: Dimension::percent(100),
                max_width: 400,
                height: 280,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    id: "spatial_animation_header",
                    position: Position::Absolute,
                    left: 12,
                    top: 12,
                    width: Dimension::percent(92),
                    height: 24,
                    column_gap: 10
                ) {
                    Text (
                        "TWEEN",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Error,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "SPRING",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Success,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "ELASTIC",
                        grow: 1.0,
                        height: 24,
                        font_size: 10,
                        bg_color: ColorToken::Surface,
                        text_color: ColorToken::Primary,
                        border_radius: 12,
                        paragraph: ParagraphStyle::label()
                    )
                }
                walk [58, 170, 282] with x {
                    View (
                        position: Position::Absolute,
                        left: x,
                        top: 52,
                        width: 2,
                        height: 190,
                        bg_color: ColorToken::Outline,
                        border_radius: 1
                    )
                }
                View (
                    bg_color: ColorToken::Error,
                    position: Position::Absolute,
                    left: 48,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    AnimateTweenY(
                        Tween::new(
                                Fixed::from_int(48),
                                Fixed::from_int(220),
                                800,
                                ease::ease_in_out_cubic,
                                PlayMode::PingPong,
                            )
                            .into(),
                    ),
                ]
                View (
                    bg_color: ColorToken::Success,
                    position: Position::Absolute,
                    left: 160,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    SpringBall {
                        spring: Spring::preset(Fixed::from_int(48), Fixed::from_int(220), SMOOTH)
                            .repeat(),
                        x: Fixed::from_int(160),
                    },
                ]
                View (
                    bg_color: ColorToken::Primary,
                    position: Position::Absolute,
                    left: 272,
                    top: 48,
                    width: 22,
                    height: 22,
                    border_radius: 11
                ) [
                    SpringBall {
                        spring: Spring::preset(Fixed::from_int(48), Fixed::from_int(220), BOUNCY)
                            .repeat(),
                        x: Fixed::from_int(272),
                    },
                ]
            }
        }
    };
    //~focus-end
}
