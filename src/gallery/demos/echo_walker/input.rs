use super::render::{MapBox, map_box_from_span};
use super::state::EchoSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::echo::{BOARD_WIDTH, Direction, EchoModal, EchoModel};
use crate::prelude::*;

/// The logical 480 × 320 point under a surface position.
fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<(i32, i32)> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some((
        ((x - rect.x) * Fixed::from_int(480) / rect.w).to_int(),
        ((y - rect.y) * Fixed::from_int(320) / rect.h).to_int(),
    ))
}

pub(super) fn local_cell(map: MapBox, rect: Rect, x: Fixed, y: Fixed) -> Option<u8> {
    let (local_x, local_y) = local_point(rect, x, y)?;
    if local_x < map.x || local_y < map.y {
        return None;
    }
    let column = map.min_x + ((local_x - map.x) / map.size) as usize;
    let row = map.min_y + ((local_y - map.y) / map.size) as usize;
    map.contains(column, row)
        .then_some((row * BOARD_WIDTH + column) as u8)
}

pub(super) fn surface_tap(surface: &EchoSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let map = map_box_from_span(surface.model.room_span());
    let Some(cell) = local_cell(map, rect, x, y) else {
        return false;
    };
    surface.model.tap_cell(cell);
    true
}

pub(super) fn surface_drag_move(surface: &EchoSurface, rect: Rect, dx: Fixed, dy: Fixed) -> bool {
    if rect.w.is_zero() || rect.h.is_zero() {
        return false;
    }
    surface.model.swipe(
        (dx * Fixed::from_int(480) / rect.w).to_int(),
        (dy * Fixed::from_int(320) / rect.h).to_int(),
    );
    true
}

/// Moves the board while it animates. An idle board is only read, so it
/// neither repaints nor asks for a save.
#[mirui_macros::system(order = Animation)]
pub(super) fn echo_tick_system(model: model!(EchoModel), delta: res!(DeltaTimeMs)) {
    if model.animating() {
        model.advance_ms(delta.0);
    }
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
        Some(PhysicalKey::KeyW | PhysicalKey::ArrowUp) => model.step(Direction::Up),
        Some(PhysicalKey::KeyA | PhysicalKey::ArrowLeft) => model.step(Direction::Left),
        Some(PhysicalKey::KeyS | PhysicalKey::ArrowDown) => model.step(Direction::Down),
        Some(PhysicalKey::KeyD | PhysicalKey::ArrowRight) => model.step(Direction::Right),
        Some(PhysicalKey::Space | PhysicalKey::Period) => model.step(Direction::Wait),
        Some(PhysicalKey::KeyE) => model.rewind(),
        Some(PhysicalKey::KeyZ | PhysicalKey::KeyU) => model.undo(),
        Some(PhysicalKey::KeyR) => model.restart(),
        Some(PhysicalKey::Enter) => model.continue_archive(),
        Some(PhysicalKey::Escape) => model.set_modal(EchoModal::None),
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
