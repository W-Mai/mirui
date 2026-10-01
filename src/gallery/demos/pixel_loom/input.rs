use super::state::PixelSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::pixel::PixelModel;
use crate::prelude::{Fixed, Rect};

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

pub(super) fn surface_tap(surface: &PixelSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some((cell_x, cell_y)) = local_cell(rect, x, y) else {
        return false;
    };
    surface.model.paint_cell(cell_x, cell_y);
    true
}

pub(super) fn surface_drag_start(surface: &PixelSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some((cell_x, cell_y)) = local_cell(rect, x, y) else {
        return false;
    };
    surface.model.begin_stroke(cell_x, cell_y);
    true
}

pub(super) fn surface_drag_move(surface: &PixelSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    if let Some((cell_x, cell_y)) = local_cell(rect, x, y) {
        surface.model.continue_stroke(cell_x, cell_y);
    }
    true
}

pub(super) fn surface_drag_end(surface: &PixelSurface, cancelled: bool) -> bool {
    surface.model.end_stroke(cancelled);
    true
}

#[mirui_macros::system(order = Animation)]
pub(super) fn pixel_tick_system(model: model!(PixelModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}
