use super::state::PixelSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::pixel::PixelModel;
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::{Fixed, Rect};
use crate::ui::ComputedRect;

pub(super) fn local_cell(rect: Rect, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
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

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let Some(model) = ctx
        .component::<PixelSurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    let Some(rect) = ctx.component::<ComputedRect>(ctx.entity).map(|rect| rect.0) else {
        return false;
    };
    match ctx.event {
        GestureEvent::Tap { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(rect, *x, *y) else {
                return false;
            };
            model.paint_cell(cell_x, cell_y);
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(rect, *x, *y) else {
                return false;
            };
            model.begin_stroke(cell_x, cell_y);
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some((cell_x, cell_y)) = local_cell(rect, *x, *y) else {
                return true;
            };
            model.continue_stroke(cell_x, cell_y);
        }
        GestureEvent::DragEnd { .. } => {
            model.end_stroke(false);
        }
        GestureEvent::DragCancel { .. } => {
            model.end_stroke(true);
        }
        _ => return false,
    }
    true
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn pixel_tick_system(model: &PixelModel, delta: Option<DeltaTimeMs>) {
    model.advance_ms(delta.map_or(16, |delta| delta.0));
}
