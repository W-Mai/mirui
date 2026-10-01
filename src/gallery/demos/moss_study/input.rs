use super::state::MossSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::moss::MossModel;
use crate::prelude::{Fixed, Rect};

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

pub(super) fn surface_tap(surface: &MossSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some((cell_x, cell_y)) = local_cell(rect, x, y) else {
        return false;
    };
    surface.model.paint_cell(cell_x, cell_y);
    true
}

pub(super) fn surface_drag_start(surface: &MossSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some((cell_x, cell_y)) = local_cell(rect, x, y) else {
        return false;
    };
    surface.model.begin_stroke(cell_x, cell_y);
    true
}

pub(super) fn surface_drag_move(surface: &MossSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    if let Some((cell_x, cell_y)) = local_cell(rect, x, y) {
        surface.model.continue_stroke(cell_x, cell_y);
    }
    true
}

pub(super) fn surface_drag_end(surface: &MossSurface, cancelled: bool) -> bool {
    surface.model.end_stroke(cancelled);
    true
}

#[mirui_macros::system(order = Animation)]
pub(super) fn moss_tick_system(model: model!(MossModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
