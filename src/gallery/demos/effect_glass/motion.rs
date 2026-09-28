use crate::prelude::*;
use crate::ui::widgets::BackgroundBlur;

//~focus-start
mirui_macros::animate!(GlassX, |world, entity, value| {
    mirui::ui::set_position(world, entity, value, Fixed::from_int(50));
});

mirui_macros::animate!(GaussRadius, |world, entity, value| {
    if let Some(blur) = world.get_mut::<BackgroundBlur>(entity) {
        blur.radius = value;
        world.invalidate_visual(entity);
    }
});
//~focus-end
