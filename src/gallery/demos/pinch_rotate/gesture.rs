use super::state::{PinchModel, PinchTarget};
use crate::prelude::*;
use crate::ui::icons::ICON_PLUS;
use crate::ui::theme::ThemedColor;
use crate::ui::widgets::icon::Icon;

const BASE_W: i32 = 160;
const BASE_H: i32 = 120;

#[compose(bind(model))]
pub(super) fn pinch_target(model: PinchModel) -> Entity {
    //~focus-start
    ui! {
        View (
            id: "pinch_target",
            width: BASE_W,
            height: BASE_H,
            bg_color: ColorToken::Primary,
            border_color: ColorToken::OnPrimary,
            border_width: 2,
            border_radius: 28,
            transform: ${ model.transform() }
        ) [
            PinchTarget {
                model: model.clone(),
            },
        ] on Pinch { model.apply_pinch(*scale_delta); } on Rotate { model.apply_rotation(*angle); }
        {
            Icon (
                path: ICON_PLUS,
                color: ThemedColor::Token(ColorToken::OnPrimary),
                size: Dimension::Px(Fixed::from_int(46)),
                grow: 1.0,
                width: Dimension::percent(100)
            )
        }
    }
    //~focus-end
}
