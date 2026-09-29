use super::geometry::board_geometry;
use super::state::PictureSurface;
use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::expeditions::{Direction4, ExpeditionPanel, ExpeditionUiModel};
use crate::gallery::play::picture::{PictureModel, PictureTool};
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::{App, Fixed, Rect, RendererFactory, Surface, World};
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

fn local_cell(model: &PictureModel, rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = (x - rect.x) * Fixed::from_int(480) / rect.w;
    let local_y = (y - rect.y) * Fixed::from_int(320) / rect.h;
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

fn event_cell(
    ctx: &HandlerCtx<'_, GestureEvent>,
    model: &<PictureModel as Model>::Handle,
    x: Fixed,
    y: Fixed,
) -> Option<u8> {
    let rect = ctx.component::<ComputedRect>(ctx.entity)?.0;
    ModelHandle::read(model, |model| local_cell(model, rect, x, y))
}

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let Some(model) = ctx
        .component::<PictureSurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    match ctx.event {
        GestureEvent::Tap { x, y, .. } => {
            let Some(cell) = event_cell(ctx, &model, *x, *y) else {
                return false;
            };
            model.apply_cell(cell);
        }
        GestureEvent::DragStart { x, y, .. } => {
            let Some(cell) = event_cell(ctx, &model, *x, *y) else {
                return false;
            };
            model.begin_stroke(cell);
        }
        GestureEvent::DragMove { x, y, .. } => {
            let Some(cell) = event_cell(ctx, &model, *x, *y) else {
                return true;
            };
            model.continue_stroke(cell);
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
        if self.expedition.panel() != ExpeditionPanel::None {
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
