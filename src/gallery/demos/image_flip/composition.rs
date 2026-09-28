use super::motion::Spinner;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{Image, ParagraphStyle, Text, TextAlign, WidgetTransform3D};

#[compose]
pub fn build_widgets() {
    ui! {
        Column (
            grow: 1.0,
            align: AlignItems::Center,
            padding: Padding::all(12),
            row_gap: 8
        ) {
            Text (
                "PROJECTIVE IMAGE",
                width: Dimension::percent(100),
                max_width: 440,
                height: 28,
                font_size: 17,
                text_color: ColorToken::OnSurface,
                paragraph: ParagraphStyle::label().with_align(TextAlign::Start)
            )
            Column (
                width: Dimension::percent(100),
                max_width: 440,
                grow: 1.0,
                align: AlignItems::Center,
                justify: JustifyContent::FlexEnd,
                padding: Padding::all(24),
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 18,
                clip_children: true
            ) {
                Image (
                    width: 120,
                    height: 120,
                    src: "thumbs_up"
                ) [
                    Spinner {
                        angle: super::PROJECTIVE_SPIN_PHASE,
                        speed_deg_per_second: Fixed::from_int(180),
                        bounce_phase: Fixed::ZERO,
                    },
                    WidgetTransform3D(Transform3D::IDENTITY),
                ]
            }
        }
    };
}
