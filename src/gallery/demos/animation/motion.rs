use crate::prelude::*;

#[mirui_macros::animate(component = AnimateX)]
pub fn apply_x(value: motion!(), style: component!(mut Style)) {
    style.layout.left = Dimension::Px(value);
    style.layout.top = Dimension::Px(Fixed::from_int(70));
}

#[mirui_macros::animate(component = AnimateColor)]
pub fn apply_color(value: motion!(), style: component!(mut Style)) {
    let r = (value * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
    style.set_bg_color(Color::rgb(r, 50, 255 - r));
}
