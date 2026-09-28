use super::state::MossSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::moss::MossModel;
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
        || local_x >= Fixed::from_int(357)
        || local_y < Fixed::from_int(67)
        || local_y >= Fixed::from_int(271)
    {
        return None;
    }
    Some((
        ((local_x.to_int() - 17) / 17) as u8,
        ((local_y.to_int() - 67) / 17) as u8,
    ))
}

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let Some(model) = ctx
        .component::<MossSurface>(ctx.entity)
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
pub(super) fn moss_tick_system(model: &MossModel, delta: Option<DeltaTimeMs>) {
    model.advance_ms(delta.map_or(16, |delta| delta.0));
}
