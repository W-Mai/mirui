use crate::prelude::*;
use crate::ui::widgets::{DropGlow, DropShadow, WidgetTransform};
use crate::ui::{ComputedRect, Parent};

pub struct ColorFlash {
    pub frame: u32,
}

#[system(order = ANIMATION)]
pub fn animate_color_flash(world: &mut World) {
    world.for_each_stable::<ColorFlash>(|world, e| {
        let frame = match world.get_mut::<ColorFlash>(e) {
            Some(c) => {
                c.frame = c.frame.wrapping_add(1);
                c.frame
            }
            None => return,
        };
        let color = match (frame / 60) % 3 {
            0 => Color::rgb(220, 60, 60),
            1 => Color::rgb(60, 200, 80),
            _ => Color::rgb(40, 140, 220),
        };
        if let Some(style) = world.get_mut::<Style>(e) {
            style.bg_color = Some(color.into());
        }
        world.invalidate(e);
    });
}

animate!(BlurPan, |world, entity, value| {
    let Some(parent) = world.get::<Parent>(entity).map(|parent| parent.0) else {
        return;
    };
    let Some(stage) = world.get::<ComputedRect>(parent).map(|rect| rect.0) else {
        return;
    };
    let Some(overlay) = world.get::<ComputedRect>(entity).map(|rect| rect.0) else {
        return;
    };
    let travel = (stage.w - overlay.w).max(Fixed::ZERO);
    if let Some(transform) = world.get_mut::<WidgetTransform>(entity) {
        transform.0.tx = travel * value;
    }
    world.invalidate_visual(entity);
});

animate!(ShadowOffset, |world, entity, value| {
    if let Some(sh) = world.get_mut::<DropShadow>(entity) {
        sh.offset.0 = value;
        sh.offset.1 = value;
    }
    world.invalidate(entity);
});

animate!(GlowPulse, |world, entity, value| {
    if let Some(gl) = world.get_mut::<DropGlow>(entity) {
        gl.blur_radius = value;
    }
    world.invalidate(entity);
});
