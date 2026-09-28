use super::state::PixelNodes;
use crate::ecs::DeltaTimeMs;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Entity, Fixed, World};
use crate::ui::ComputedRect;

pub(super) fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    if local_x < Fixed::from_int(17)
        || local_x >= Fixed::from_int(209)
        || local_y < Fixed::from_int(59)
        || local_y >= Fixed::from_int(251)
    {
        return None;
    }
    let column = (local_x.to_int() - 17) / 16;
    let row = (local_y.to_int() - 59) / 16;
    if (0..12).contains(&column) && (0..12).contains(&row) {
        Some((column as u8, row as u8))
    } else {
        None
    }
}

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PixelNodes::update(world, |model| {
                model.begin_stroke(cell_x, cell_y) | model.end_stroke(false)
            });
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PixelNodes::update(world, |model| model.begin_stroke(cell_x, cell_y));
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(world, entity, *x, *y) else {
                return true;
            };
            PixelNodes::update(world, |model| model.continue_stroke(cell_x, cell_y));
        }
        GestureEvent::DragEnd { .. } => {
            PixelNodes::update(world, |model| model.end_stroke(false));
        }
        GestureEvent::DragCancel { .. } => {
            PixelNodes::update(world, |model| model.end_stroke(true));
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = ANIMATION)]
pub(super) fn pixel_tick_system(world: &mut World) {
    let elapsed = world.resource::<DeltaTimeMs>().map_or(16, |delta| delta.0);
    PixelNodes::update(world, |model| model.advance_ms(elapsed));
}
