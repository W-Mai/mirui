use crate::prelude::*;

#[mirui_macros::animate(component = AnimateX)]
pub fn apply_x(value: motion!(), style: component!(mut Style)) {
    style.layout.left = Dimension::Px(value);
    style.layout.top = Dimension::Px(Fixed::from_int(70));
}
