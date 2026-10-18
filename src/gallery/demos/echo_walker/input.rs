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
pub(super) fn keyboard(input: event!(Key), model: model!(EchoModel)) -> bool {
    if !input.pressed
        || input
            .modifiers
            .is_some_and(|m| m.control || m.meta || m.alt)
    {
        return false;
    }
    match input.physical {
        Some(PhysicalKey::KeyW) => model.step(Direction::Up),
        Some(PhysicalKey::KeyA) => model.step(Direction::Left),
        Some(PhysicalKey::KeyS) => model.step(Direction::Down),
        Some(PhysicalKey::KeyD) => model.step(Direction::Right),
        Some(PhysicalKey::Space) => model.step(Direction::Wait),
        Some(PhysicalKey::KeyR) => model.rewind(),
        Some(PhysicalKey::KeyU) => model.undo(),
        _ => return false,
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelHandle;
    use crate::input::event::{dispatch_input, input::PhysicalKey};

    #[derive(Default)]
    struct Unhandled(Option<PhysicalKey>);

    #[event]
    fn unhandled(input: event!(Key), seen: res!(mut Unhandled)) -> bool {
        seen.0 = input.physical;
        true
    }

    #[test]
    fn committed_words_do_not_trigger_individual_game_shortcuts() {
        let mut app = crate::app::App::headless(32, 32);
        let model = app.add_model(EchoModel::default()).finish();
        app.on_event(Key::EVENT, keyboard::callback(model.clone()));
        for text in ["wasd", " ", "w"] {
            dispatch_input(
                &mut app.world,
                None,
                &TextCommitted::new(text).into(),
                0,
                32,
                32,
            );
            assert_eq!(model.tick(), 0);
        }
    }

    #[test]
    fn game_positions_ignore_releases_and_shortcuts_but_preserve_shift_and_repeat() {
        use crate::input::event::input::KeyModifiers;
        let mut app = App::headless(32, 32);
        let model = app.add_model(EchoModel::default()).finish();
        app.on_event(Key::EVENT, keyboard::callback(model.clone()));
        let base = crate::test_support::physical_key(PhysicalKey::Space);
        for event in [
            Key {
                pressed: false,
                ..base
            },
            Key {
                modifiers: Some(KeyModifiers {
                    control: true,
                    ..Default::default()
                }),
                ..base
            },
            Key {
                modifiers: Some(KeyModifiers {
                    meta: true,
                    ..Default::default()
                }),
                ..base
            },
            Key {
                modifiers: Some(KeyModifiers {
                    alt: true,
                    ..Default::default()
                }),
                ..base
            },
            Key {
                physical: None,
                ..base
            },
        ] {
            dispatch_input(&mut app.world, None, &event.into(), 0, 32, 32);
            assert_eq!(model.tick(), 0);
        }
        for event in [
            Key {
                modifiers: Some(KeyModifiers {
                    shift: true,
                    ..Default::default()
                }),
                ..base
            },
            Key {
                repeat: Some(true),
                ..base
            },
        ] {
            model.rewind();
            dispatch_input(&mut app.world, None, &event.into(), 0, 32, 32);
            assert_eq!(model.tick(), 1);
        }
    }

    #[test]
    fn keyboard_keeps_the_registered_model_instance() {
        let mut app = App::headless(1, 1);
        let model = app.add_model(EchoModel::default()).finish();
        let observer = model.clone();
        app.world.insert_resource(Unhandled::default());
        app.on_event(Key::EVENT, keyboard::callback(model.clone()))
            .on_event(Key::EVENT, unhandled::callback());
        drop(model);

        dispatch_input(
            &mut app.world,
            None,
            &crate::test_support::physical_key(PhysicalKey::Space).into(),
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
            &crate::test_support::physical_key(PhysicalKey::KeyX).into(),
            1,
            1,
            1,
        );
        observer.read(|model| assert_eq!(model.tick(), 1));
        assert_eq!(
            app.world.resource::<Unhandled>().unwrap().0,
            Some(PhysicalKey::KeyX)
        );
    }
}
