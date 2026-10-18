use super::geometry::board_geometry_for_level;
use super::state::PictureSurface;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, picture_level,
};
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::prelude::{Fixed, Key, PhysicalKey, Rect, event};

fn local_cell(level_index: u8, rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    let size = picture_level(level_index)?.size();
    let geometry = board_geometry_for_level(level_index);
    if local_x < Fixed::from_int(geometry.x)
        || local_y < Fixed::from_int(geometry.y)
        || local_x >= Fixed::from_int(geometry.x + geometry.size)
        || local_y >= Fixed::from_int(geometry.y + geometry.size)
    {
        return None;
    }
    let column = (local_x.to_int() - geometry.x) / geometry.cell;
    let row = (local_y.to_int() - geometry.y) / geometry.cell;
    if column >= 0 && row >= 0 && column < i32::from(size) && row < i32::from(size) {
        Some((row as u8) * size + column as u8)
    } else {
        None
    }
}

fn surface_blocked(surface: &PictureSurface) -> bool {
    surface.expedition.panel() != ExpeditionPanel::None
        || surface.model.modal() != ExpeditionModal::None
}

pub(super) fn surface_tap(
    surface: &PictureSurface,
    rect: Option<Rect>,
    x: Fixed,
    y: Fixed,
) -> bool {
    if surface_blocked(surface) {
        return true;
    }
    let Some(rect) = rect else {
        return false;
    };
    let Some(cell) = local_cell(surface.model.level_index(), rect, x, y) else {
        return false;
    };
    surface.model.apply_cell(cell);
    true
}

pub(super) fn surface_drag_start(
    surface: &PictureSurface,
    rect: Option<Rect>,
    x: Fixed,
    y: Fixed,
) -> bool {
    if surface_blocked(surface) {
        return true;
    }
    let Some(rect) = rect else {
        return false;
    };
    let Some(cell) = local_cell(surface.model.level_index(), rect, x, y) else {
        return false;
    };
    surface.model.begin_stroke(cell);
    true
}

pub(super) fn surface_drag_move(
    surface: &PictureSurface,
    rect: Option<Rect>,
    x: Fixed,
    y: Fixed,
) -> bool {
    if surface_blocked(surface) {
        return true;
    }
    let Some(rect) = rect else {
        return true;
    };
    if let Some(cell) = local_cell(surface.model.level_index(), rect, x, y) {
        surface.model.continue_stroke(cell);
    }
    true
}

pub(super) fn surface_drag_end(surface: &PictureSurface, cancelled: bool) -> bool {
    surface.model.end_stroke(cancelled);
    true
}

#[event]
pub(super) fn keyboard(
    input: event!(Key),
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> bool {
    if expedition.panel() != ExpeditionPanel::None || model.modal() != ExpeditionModal::None {
        return false;
    }
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    match input.physical {
        Some(PhysicalKey::KeyW) => model.move_cursor(Direction4::Up),
        Some(PhysicalKey::KeyD) => model.move_cursor(Direction4::Right),
        Some(PhysicalKey::KeyS) => model.move_cursor(Direction4::Down),
        Some(PhysicalKey::KeyA) => model.move_cursor(Direction4::Left),
        Some(PhysicalKey::Space) => model.apply_cursor(None),
        Some(PhysicalKey::KeyX) => model.apply_cursor(Some(PictureTool::Mark)),
        Some(PhysicalKey::KeyZ) => model.undo(),
        Some(PhysicalKey::KeyH) => model.reveal_hint(),
        _ => return false,
    };
    true
}
