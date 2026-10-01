use crate::prelude::*;
use crate::ui::widgets::BackgroundBlur;

//~focus-start
#[mirui_macros::animate(component = GlassX)]
pub fn apply_glass_x(value: motion!(), style: component!(mut Style)) {
    style.layout.left = Dimension::Px(value);
    style.layout.top = Dimension::Px(Fixed::from_int(50));
}

#[mirui_macros::animate(component = GaussRadius)]
pub fn apply_gauss_radius(value: motion!(), blur: component!(mut BackgroundBlur)) {
    blur.radius = value;
}
//~focus-end
