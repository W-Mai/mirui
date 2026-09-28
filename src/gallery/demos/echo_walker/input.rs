use super::render::map_box;
use super::state::EchoNodes;
use crate::gallery::play::echo::{BOARD_HEIGHT, BOARD_WIDTH, Direction, EchoCommand, EchoModel};
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

pub(super) fn local_cell(world: &World, entity: Entity, x: Fixed, y: Fixed) -> Option<u8> {
    let rect = world.get::<ComputedRect>(entity)?.0;
    let model = world.resource::<EchoModel>()?;
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

pub(super) fn surface_gesture(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let GestureEvent::Tap { x, y, .. } = event else {
        return false;
    };
    let Some(cell) = local_cell(world, entity, *x, *y) else {
        return false;
    };
    let Some(position) = world.resource::<EchoModel>().map(EchoModel::position) else {
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
        EchoNodes::dispatch(world, EchoCommand::Step(direction));
    }
    true
}
pub(super) struct EchoKeyboardPlugin;

pub(super) fn handle_key(world: &mut World, ch: char) -> bool {
    match ch {
        'w' | 'W' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Up)),
        'a' | 'A' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Left)),
        's' | 'S' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Down)),
        'd' | 'D' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Right)),
        ' ' => EchoNodes::dispatch(world, EchoCommand::Step(Direction::Wait)),
        'r' | 'R' => EchoNodes::dispatch(world, EchoCommand::Rewind),
        'u' | 'U' => EchoNodes::dispatch(world, EchoCommand::Undo),
        _ => return false,
    }
    true
}

impl<B, F> Plugin<B, F> for EchoKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        handle_key(world, *ch)
    }
}
