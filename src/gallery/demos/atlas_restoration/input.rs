use super::geometry::board_geometry;
use super::state::PictureNodes;
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiState};
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::{App, Entity, Fixed, RendererFactory, Surface, World};
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
    let model = world.resource::<PictureModel>()?;
    let size = model.level().size();
    let geometry = board_geometry(model);
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

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::Tap { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PictureNodes::update(world, |model| {
                model.begin_stroke(cell) | model.end_stroke(false)
            });
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return false;
            };
            PictureNodes::update(world, |model| model.begin_stroke(cell));
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some(cell) = local_cell(world, entity, *x, *y) else {
                return true;
            };
            PictureNodes::update(world, |model| model.continue_stroke(cell));
        }
        GestureEvent::DragEnd { .. } => {
            PictureNodes::update(world, |model| model.end_stroke(false));
        }
        GestureEvent::DragCancel { .. } => {
            PictureNodes::update(world, |model| model.end_stroke(true));
        }
        _ => return false,
    }
    true
}

pub(super) struct PictureKeyboardPlugin;

impl<B, F> Plugin<B, F> for PictureKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        if world
            .resource::<ExpeditionUiState>()
            .is_some_and(|state| state.panel() != ExpeditionPanel::None)
        {
            return false;
        }
        match ch {
            'w' | 'W' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Up)),
            'd' | 'D' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Right)),
            's' | 'S' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Down)),
            'a' | 'A' => PictureNodes::update(world, |model| model.move_cursor(Direction4::Left)),
            ' ' => PictureNodes::update(world, |model| model.apply_cursor(None)),
            'x' | 'X' => {
                PictureNodes::update(world, |model| model.apply_cursor(Some(PictureTool::Mark)))
            }
            'z' | 'Z' => PictureNodes::update(world, PictureModel::undo),
            'h' | 'H' => PictureNodes::update(world, PictureModel::reveal_hint),
            _ => return false,
        }
        true
    }
}
