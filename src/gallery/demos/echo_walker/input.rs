use super::render::{MapBox, map_box_from_columns};
use super::state::EchoSurface;
use crate::gallery::play::echo::{BOARD_HEIGHT, BOARD_WIDTH, Direction, EchoModel};
use crate::prelude::*;

pub(super) fn local_cell(map: MapBox, rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    let local_x = ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int();
    let local_y = ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int();
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

pub(super) fn surface_tap(surface: &EchoSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let (min, max) = surface.model.room_column_span();
    let Some(cell) = local_cell(map_box_from_columns(min, max), rect, x, y) else {
        return false;
    };
    let position = surface.model.position();
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
        surface.model.step(direction);
    }
    true
}

#[event]
pub(super) fn keyboard(input: event!(TextCommitted), model: model!(EchoModel)) -> bool {
    match input.text() {
        "w" | "W" => model.step(Direction::Up),
        "a" | "A" => model.step(Direction::Left),
        "s" | "S" => model.step(Direction::Down),
        "d" | "D" => model.step(Direction::Right),
        " " => model.step(Direction::Wait),
        "r" | "R" => model.rewind(),
        "u" | "U" => model.undo(),
        _ => return false,
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelHandle;
    use crate::input::event::{dispatch_input, input::InputEvent};

    #[derive(Default)]
    struct Unhandled(Option<char>);

    #[event]
    fn unhandled(input: event!(TextCommitted), seen: res!(mut Unhandled)) -> bool {
        seen.0 = input.text().chars().next();
        true
    }

    #[test]
    fn committed_words_do_not_trigger_individual_game_shortcuts() {
        let mut app = crate::app::App::headless(32, 32);
        app.add_resource(Unhandled::default());
        let model = app.add_model(EchoModel::default());
        app.on_event(TextCommitted::EVENT, keyboard::callback(model))
            .on_event(TextCommitted::EVENT, unhandled::callback());
        crate::input::event::dispatch_input(
            &mut app.world,
            None,
            &TextCommitted::new("wasd").into(),
            0,
            32,
            32,
        );
        assert_eq!(app.world.resource::<Unhandled>().unwrap().0, Some('w'));
    }

    #[test]
    fn keyboard_keeps_the_registered_model_instance() {
        let mut app = App::headless(1, 1);
        let model = app.add_model(EchoModel::default());
        let observer = model.clone();
        app.world.insert_resource(Unhandled::default());
        app.on_event(TextCommitted::EVENT, keyboard::callback(model.clone()))
            .on_event(TextCommitted::EVENT, unhandled::callback());
        drop(model);

        dispatch_input(
            &mut app.world,
            None,
            &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new(" ")),
            0,
            1,
            1,
        );
        observer.read(|model| {
            assert_eq!(model.tick(), 1);
        });
        assert_eq!(app.world.resource::<Unhandled>().unwrap().0, None);
        dispatch_input(
            &mut app.world,
            None,
            &InputEvent::TextCommitted(crate::input::event::input::TextCommitted::new("x")),
            1,
            1,
            1,
        );
        observer.read(|model| assert_eq!(model.tick(), 1));
        assert_eq!(app.world.resource::<Unhandled>().unwrap().0, Some('x'));
    }
}
