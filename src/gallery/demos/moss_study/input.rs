use super::render::{CELL_ORIGIN, CELL_PITCH};
use super::state::MossSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::moss::{GRID_HEIGHT, GRID_WIDTH, MossModal, MossModel, MossTool};
use crate::prelude::*;

pub(super) fn local_cell(rect: Rect, x: Fixed, y: Fixed) -> Option<(u8, u8)> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int() - CELL_ORIGIN.0;
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int() - CELL_ORIGIN.1;
    let width = CELL_PITCH * i32::from(GRID_WIDTH);
    let height = CELL_PITCH * i32::from(GRID_HEIGHT);
    if !(0..width).contains(&local_x) || !(0..height).contains(&local_y) {
        return None;
    }
    Some(((local_x / CELL_PITCH) as u8, (local_y / CELL_PITCH) as u8))
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

/// Space runs and pauses, N or → steps once, Z undoes, 1 2 3 pick plant,
/// erase and the glider, R turns the glider, and Esc opens or closes the
/// seeds.
#[event]
pub(super) fn keyboard(input: event!(Key), model: model!(MossModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    if model.modal() != MossModal::None {
        if !matches!(input.physical, Some(PhysicalKey::Escape)) {
            return false;
        }
        model.close_modal();
        return true;
    }
    match input.physical {
        Some(PhysicalKey::Space) => model.toggle_running(),
        Some(PhysicalKey::KeyN | PhysicalKey::ArrowRight) => model.step(),
        Some(PhysicalKey::KeyZ) => model.undo(),
        Some(PhysicalKey::Digit1) => model.set_tool(MossTool::Plant),
        Some(PhysicalKey::Digit2) => model.set_tool(MossTool::Erase),
        Some(PhysicalKey::Digit3) => model.set_tool(MossTool::Glider),
        Some(PhysicalKey::KeyR) => model.pick_glider(),
        Some(PhysicalKey::Escape) => model.open_seeds(),
        _ => return false,
    };
    true
}
