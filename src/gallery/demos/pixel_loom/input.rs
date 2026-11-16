use super::render::{CELL, GRID_LEFT, GRID_TOP};
use super::state::PixelSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::pixel::{FRAME_COUNT, GRID_HEIGHT, GRID_WIDTH, PixelModel, PixelTool};
use crate::prelude::*;

pub(super) fn local_cell(rect: Rect, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int() - GRID_LEFT;
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int() - GRID_TOP;
    let width = CELL * i32::from(GRID_WIDTH);
    let height = CELL * i32::from(GRID_HEIGHT);
    if !(0..width).contains(&local_x) || !(0..height).contains(&local_y) {
        return None;
    }
    Some(((local_x / CELL) as u8, (local_y / CELL) as u8))
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

/// B brush, E eraser, M mirror, O onion skin, Z undo, Space play, 1 to 4 or
/// the left and right arrows pick a frame, Esc closes a dialog.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(PixelModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    let frame = model.frame();
    match input.physical {
        Some(PhysicalKey::KeyB | PhysicalKey::KeyP) => model.set_tool(PixelTool::Brush),
        Some(PhysicalKey::KeyE) => model.set_tool(PixelTool::Erase),
        Some(PhysicalKey::KeyM) => model.toggle_mirror(),
        Some(PhysicalKey::KeyO) => model.toggle_onion(),
        Some(PhysicalKey::KeyZ) => model.undo(),
        Some(PhysicalKey::Space) => model.toggle_playback(),
        Some(PhysicalKey::Digit1) => model.select_frame(0),
        Some(PhysicalKey::Digit2) => model.select_frame(1),
        Some(PhysicalKey::Digit3) => model.select_frame(2),
        Some(PhysicalKey::Digit4) => model.select_frame(3),
        Some(PhysicalKey::ArrowLeft) => model.select_frame((frame + FRAME_COUNT - 1) % FRAME_COUNT),
        Some(PhysicalKey::ArrowRight) => model.select_frame((frame + 1) % FRAME_COUNT),
        Some(PhysicalKey::Escape) => model.close_modal(),
        _ => return false,
    };
    true
}
