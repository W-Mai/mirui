use super::geometry::board_geometry_for_level;
use super::state::PictureSurface;
use crate::core::model::Model;
use crate::gallery::play::expeditions::{
    Direction4, ExpeditionModal, ExpeditionPanel, ExpeditionUiModel, picture_level,
};
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::prelude::plugin::Plugin;
use crate::prelude::{App, Fixed, Rect, RendererFactory, Surface, World};
use crate::surface::InputEvent;

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

pub(super) struct PictureKeyboardPlugin {
    model: <PictureModel as Model>::Handle,
    expedition: <ExpeditionUiModel as Model>::Handle,
}

impl PictureKeyboardPlugin {
    pub(super) fn new(
        model: <PictureModel as Model>::Handle,
        expedition: <ExpeditionUiModel as Model>::Handle,
    ) -> Self {
        Self { model, expedition }
    }

    pub(super) fn on_char(&self, ch: char) -> bool {
        if self.expedition.panel() != ExpeditionPanel::None
            || self.model.modal() != ExpeditionModal::None
        {
            return false;
        }
        match ch {
            'w' | 'W' => self.model.move_cursor(Direction4::Up),
            'd' | 'D' => self.model.move_cursor(Direction4::Right),
            's' | 'S' => self.model.move_cursor(Direction4::Down),
            'a' | 'A' => self.model.move_cursor(Direction4::Left),
            ' ' => self.model.apply_cursor(None),
            'x' | 'X' => self.model.apply_cursor(Some(PictureTool::Mark)),
            'z' | 'Z' => self.model.undo(),
            'h' | 'H' => self.model.reveal_hint(),
            _ => return false,
        };
        true
    }
}

impl<B, F> Plugin<B, F> for PictureKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        self.on_char(*ch)
    }
}
