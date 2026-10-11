use super::geometry::board_geometry_for_level;
use super::state::PictureSurface;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, picture_level,
};
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::prelude::{Fixed, Rect, TextCommitted, event};

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
    input: event!(TextCommitted),
    model: model!(PictureModel),
    expedition: model!(ExpeditionUiModel),
) -> bool {
    if expedition.panel() != ExpeditionPanel::None || model.modal() != ExpeditionModal::None {
        return false;
    }
    match input.text() {
        "w" | "W" => model.move_cursor(Direction4::Up),
        "d" | "D" => model.move_cursor(Direction4::Right),
        "s" | "S" => model.move_cursor(Direction4::Down),
        "a" | "A" => model.move_cursor(Direction4::Left),
        " " => model.apply_cursor(None),
        "x" | "X" => model.apply_cursor(Some(PictureTool::Mark)),
        "z" | "Z" => model.undo(),
        "h" | "H" => model.reveal_hint(),
        _ => return false,
    };
    true
}
