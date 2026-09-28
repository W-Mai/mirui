use super::state::{BAR_H, BAR_MARGIN, BAR_W, BarState, START_Y};
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(12),
            align: AlignItems::Center,
            justify: JustifyContent::Center,
            bg_color: ColorToken::Surface
        ) {
            View (
                id: "subpixel_stage",
                width: Dimension::percent(100),
                max_width: 480,
                min_height: 140,
                grow: 1.0,
                max_height: 360,
                padding: Padding::all(12),
                direction: FlexDirection::Column,
                bg_color: ColorToken::SurfaceVariant,
                border_color: ColorToken::Outline,
                border_width: 1,
                border_radius: 18,
                clip_children: true
            ) {
                Row (
                    width: Dimension::percent(100),
                    height: 32,
                    align: AlignItems::Center,
                    padding: Padding {
                        top: Dimension::px(0),
                        right: Dimension::px(4),
                        bottom: Dimension::px(0),
                        left: Dimension::px(4),
                    }
                ) {
                    Text (
                        "PIXEL-SNAPPED",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::Error,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
                    )
                    Text (
                        "Q24.8 SUBPIXEL",
                        grow: 1.0,
                        font_size: 11,
                        text_color: ColorToken::Secondary,
                        paragraph: ParagraphStyle::label().with_align(TextAlign::End)
                    )
                }
                View (
                    id: "subpixel_arena",
                    width: Dimension::percent(100),
                    grow: 1.0,
                    bg_color: ColorToken::Surface,
                    border_radius: 12,
                    clip_children: true
                ) {
                    View (
                        id: "subpixel_snapped_bar",
                        bg_color: ColorToken::Error,
                        position: Position::Absolute,
                        left: BAR_MARGIN,
                        top: START_Y,
                        width: BAR_W,
                        height: BAR_H,
                        border_radius: 4
                    ) [
                        BarState {
                            y: Fixed::from_int(START_Y),
                            speed_per_second: Fixed::from_ratio(135, 64),
                            snap: true,
                            x: Fixed::from_int(BAR_MARGIN),
                            right_anchored: false,
                        },
                    ]
                    View (
                        id: "subpixel_smooth_bar",
                        bg_color: ColorToken::Secondary,
                        position: Position::Absolute,
                        left: 0,
                        top: START_Y,
                        width: BAR_W,
                        height: BAR_H,
                        border_radius: 4
                    ) [
                        BarState {
                            y: Fixed::from_int(START_Y),
                            speed_per_second: Fixed::from_ratio(135, 64),
                            snap: false,
                            x: Fixed::ZERO,
                            right_anchored: true,
                        },
                    ]
                }
            }
        }
    };
}
