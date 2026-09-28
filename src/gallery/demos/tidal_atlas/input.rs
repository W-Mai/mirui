use super::state::TideNodes;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Entity, Fixed, World};
use crate::ui::ComputedRect;

pub(super) fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    if local_x < Fixed::from_int(14)
        || local_x >= Fixed::from_int(224)
        || local_y < Fixed::from_int(60)
        || local_y >= Fixed::from_int(270)
    {
        return None;
    }
    let column = (local_x.to_int() - 14) / 35;
    let row = (local_y.to_int() - 60) / 35;
    if (local_x.to_int() - 14) % 35 >= 32 || (local_y.to_int() - 60) % 35 >= 32 {
        return None;
    }
    Some((row * 6 + column) as u8)
}

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(cell) = local_cell(world, entity, *x, *y) else {
        return false;
    };
    TideNodes::update(world, |model| model.select_cell(cell));
    true
}
