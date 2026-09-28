use crate::prelude::*;
use crate::ui;

mirui_macros::animate!(AnimateX, |world, entity, value| {
    ui::set_position(world, entity, value, Fixed::from_int(70));
});

mirui_macros::animate!(AnimateColor, |world, entity, value| {
    let r = (value * Fixed::from_int(255)).to_int().clamp(0, 255) as u8;
    if let Some(style) = world.get_mut::<ui::Style>(entity) {
        style.set_bg_color(Color::rgb(r, 50, 255 - r));
    }
    world.invalidate(entity);
});
