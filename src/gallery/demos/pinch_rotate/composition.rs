use super::gesture::pinch_target;
use super::state::PinchStatus;
use crate::prelude::*;
use crate::ui::widgets::{ParagraphStyle, Text};

#[compose]
pub fn build_widgets() {
    let status = Signal::new(PinchStatus::IDLE);
    let status_text = status.clone();
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
                    text: ${ status_text.get().label() },
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
                pinch_target (status)
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
