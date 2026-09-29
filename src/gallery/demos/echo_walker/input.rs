use super::render::map_box;
use super::state::EchoSurface;
use crate::core::model::{Model, ModelHandle};
use crate::gallery::play::echo::{BOARD_HEIGHT, BOARD_WIDTH, Direction, EchoModel};
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

pub(super) fn local_cell(model: &EchoModel, rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int();
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int();
    let map = map_box(model);
    if local_x < map.x || local_y < map.y {
        return None;
    }
    let column = (local_x - map.x) / map.size;
    let row = (local_y - map.y) / map.size;
    if column < 0 || row < 0 || row >= BOARD_HEIGHT as i32 {
        return None;
    }
    let board_x = map.min as i32 + column;
    if board_x > map.max as i32 {
        return None;
    }
    Some((row as usize * BOARD_WIDTH + board_x as usize) as u8)
}

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let GestureEvent::Tap { x, y, .. } = ctx.event else {
        return false;
    };
    let Some(model) = ctx
        .component::<EchoSurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    let Some(rect) = ctx.component::<ComputedRect>(ctx.entity).map(|rect| rect.0) else {
        return false;
    };
    let Some((cell, position)) = ModelHandle::read(&model, |model| {
        local_cell(model, rect, *x, *y).map(|cell| (cell, model.position()))
    }) else {
        return false;
    };
    let dx = i16::from(cell % BOARD_WIDTH as u8) - i16::from(position % BOARD_WIDTH as u8);
    let dy = i16::from(cell / BOARD_WIDTH as u8) - i16::from(position / BOARD_WIDTH as u8);
    let direction = match (dx, dy) {
        (0, -1) => Some(Direction::Up),
        (1, 0) => Some(Direction::Right),
        (0, 1) => Some(Direction::Down),
        (-1, 0) => Some(Direction::Left),
        _ => None,
    };
    if let Some(direction) = direction {
        model.step(direction);
    }
    true
}

pub(super) fn handle_key(model: &<EchoModel as Model>::Handle, ch: char) -> bool {
    match ch {
        'w' | 'W' => model.step(Direction::Up),
        'a' | 'A' => model.step(Direction::Left),
        's' | 'S' => model.step(Direction::Down),
        'd' | 'D' => model.step(Direction::Right),
        ' ' => model.step(Direction::Wait),
        'r' | 'R' => model.rewind(),
        'u' | 'U' => model.undo(),
        _ => return false,
    };
    true
}

pub(super) struct EchoKeyboardPlugin {
    model: <EchoModel as Model>::Handle,
}

impl EchoKeyboardPlugin {
    pub(super) fn new(model: <EchoModel as Model>::Handle) -> Self {
        Self { model }
    }
}

impl<B, F> Plugin<B, F> for EchoKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        handle_key(&self.model, *ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_keeps_the_registered_model_instance() {
        let mut app = App::headless(1, 1);
        let model = app.add_model(EchoModel::default());
        let plugin = EchoKeyboardPlugin::new(model.clone());
        drop(model);

        assert!(handle_key(&plugin.model, ' '));
        ModelHandle::read(&plugin.model, |model| {
            assert_eq!(model.tick(), 1);
        });
        assert!(!handle_key(&plugin.model, 'x'));
    }
}
