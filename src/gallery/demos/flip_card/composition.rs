use super::state::FlipCard;
use crate::prelude::*;
use crate::types::Transform3D;
use crate::ui::widgets::{ParagraphStyle, Text, WidgetTransform3D};

#[compose]
pub fn build_widgets() {
    let root = cx.parent();
    ui! {
        View (grow: 1.0, bg_color: ColorToken::Surface) {
            Text (
                "PROJECTIVE FLIP · FRONT / BACK",
                position: Position::Absolute,
                left: 16,
                top: 12,
                width: Dimension::percent(100),
                max_width: 360,
                height: 26,
                font_size: 14,
                text_color: ColorToken::OnSurface
            )
            Column (
                position: Position::Absolute,
                bg_color: ColorToken::Primary,
                border_radius: 18,
                align: AlignItems::Center,
                justify: JustifyContent::Center,
                row_gap: 8,
                clip_children: true
            ) [
                FlipCard {
                    angle_deg: super::PROJECTIVE_SPIN_PHASE,
                    speed_deg_per_second: Fixed::from_int(60),
                    front_color: ColorToken::Primary,
                    back_color: ColorToken::Error,
                    root,
                },
                WidgetTransform3D(Transform3D::IDENTITY),
            ] {
                Text (
                    "MIRUI",
                    width: Dimension::percent(100),
                    height: 34,
                    font_size: 22,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
                Text (
                    "2.5D CARD",
                    width: Dimension::percent(100),
                    height: 22,
                    font_size: 10,
                    text_color: ColorToken::OnPrimary,
                    paragraph: ParagraphStyle::label()
                )
            }
        }
    };
}
