use super::state::Page;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{ParagraphStyle, Text, TextAlign, TransformOrigin, WidgetTransform3D};

#[compose]
pub fn build_widgets() {
    //~focus-start
    ui! {
        Column (
            id: "book_flip_shell",
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12,
            bg_color: ColorToken::Surface
        ) {
            Text (
                "Projective book",
                width: Dimension::percent(100),
                max_width: 440,
                height: 28,
                font_size: 20,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Row (
                id: "book_flip_spread",
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                Column (
                    id: "book_flip_left_page",
                    width: Dimension::percent(34),
                    min_width: 112,
                    max_width: 180,
                    height: Dimension::percent(78),
                    min_height: 160,
                    max_height: 240,
                    padding: Padding::all(16),
                    row_gap: 10,
                    bg_color: ColorToken::SurfaceVariant,
                    border_radius: 14
                ) {
                    Text (
                        "mirui",
                        height: 34,
                        font_size: 22,
                        text_color: ColorToken::OnSurface,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "retained geometry\nfixed-point motion",
                        grow: 1.0,
                        font_size: 12,
                        text_color: ColorToken::OnSurfaceVariant,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "01",
                        height: 24,
                        font_size: 12,
                        text_color: ColorToken::Secondary,
                        paragraph: ParagraphStyle::label()
                    )
                }
                Column (
                    id: "book_flip_right_page",
                    width: Dimension::percent(34),
                    min_width: 112,
                    max_width: 180,
                    height: Dimension::percent(78),
                    min_height: 160,
                    max_height: 240,
                    padding: Padding::all(16),
                    row_gap: 10,
                    bg_color: ColorToken::Primary,
                    border_radius: 14
                ) [
                    TransformOrigin {
                        x: Fixed::ZERO,
                        y: Fixed::ONE / 2,
                    },
                    Page {
                        angle_deg: super::PROJECTIVE_SPIN_PHASE,
                        speed_deg_per_second: Fixed::from_int(30),
                    },
                    WidgetTransform3D(Transform3D::IDENTITY),
                ] {
                    Text (
                        "Live page",
                        height: 34,
                        font_size: 18,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "one node\none transform",
                        grow: 1.0,
                        font_size: 12,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label()
                    )
                    Text (
                        "02",
                        height: 24,
                        font_size: 12,
                        text_color: ColorToken::OnPrimary,
                        paragraph: ParagraphStyle::label()
                    )
                }
            }
        }
    };
    //~focus-end
}
