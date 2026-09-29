use super::gesture::pinch_target;
use super::state::PinchModel;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose(bind(model))]
pub(super) fn build_widgets(model: PinchModel) {
    ui! {
        Column (
            grow: 1.0,
            padding: Padding::all(16),
            row_gap: 12,
            bg_color: ColorToken::Surface
        ) {
            View (
                width: Dimension::percent(100),
                height: 32,
                bg_color: ColorToken::SurfaceVariant,
                border_radius: 16
            ) {
                Text (
                    text: ${ format_args!("{}", model.status()) },
                    text_capacity: 64,
                    grow: 1.0,
                    width: Dimension::percent(100),
                    height: 32,
                    font_size: 10,
                    text_color: ColorToken::Secondary,
                    paragraph: ParagraphStyle::label(),
                    id: "pinch_status"
                )
            }
            View (
                grow: 1.0,
                width: Dimension::percent(100),
                align: AlignItems::Center,
                justify: JustifyContent::Center
            ) {
                pinch_target (model.clone())
            }
            Text (
                "TWO-POINTER GESTURE · LIVE TRANSFORM",
                width: Dimension::percent(100),
                height: 20,
                font_size: 9,
                text_color: ColorToken::OnSurfaceVariant,
                paragraph: ParagraphStyle::label()
            )
        }
    };
}
