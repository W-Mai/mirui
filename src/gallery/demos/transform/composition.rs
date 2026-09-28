use super::motion::Spinner;
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign, WidgetTransform};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(12),
            row_gap: 10
        ) {
            Text (
                "AFFINE TRANSFORMS",
                width: Dimension::percent(100),
                max_width: 440,
                height: 28,
                font_size: 17,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                width: Dimension::percent(100),
                max_width: 440,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::SpaceAround,
                padding: Padding::all(18),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18
            ) {
                Column (width: 120, align: AlignItems::Center, row_gap: 14) {
                    View (
                        width: 76,
                        height: 76,
                        bg_color: Color::rgb(248, 81, 73),
                        border_radius: 14
                    ) [
                        Spinner {
                            angle: Fixed::ZERO,
                            speed: Fixed::from_int(2),
                        },
                        WidgetTransform(Transform::IDENTITY),
                    ]
                    Text (
                        "VIEW",
                        width: 76,
                        height: 22,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Column (width: 120, align: AlignItems::Center, row_gap: 14) {
                    Image (
                        width: 70,
                        height: 70,
                        src: "thumbs_up"
                    ) [
                        Spinner {
                            angle: Fixed::ZERO,
                            speed: Fixed::from_int(3),
                        },
                        WidgetTransform(Transform::IDENTITY),
                    ]
                    Text (
                        "IMAGE",
                        width: 76,
                        height: 22,
                        font_size: 10,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
        }
    };
}
