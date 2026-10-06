use super::state::PostSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::post::PostModel;
use crate::prelude::*;

fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Point> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

pub(super) fn surface_tap(surface: &PostSurface, rect: Rect, x: Fixed, y: Fixed) -> bool {
    let Some(point) = local_point(rect, x, y) else {
        return false;
    };
    for (index, center_x) in [151, 265].into_iter().enumerate() {
        let dx = point.x - Fixed::from_int(center_x);
        let dy = point.y - Fixed::from_int(157);
        if dx * dx + dy * dy <= Fixed::from_int(23) * Fixed::from_int(23) {
            surface.model.toggle_switch(index);
            return true;
        }
    }
    false
}

#[mirui_macros::system(order = Animation)]
pub(super) fn post_tick_system(model: model!(PostModel), delta: res!(DeltaTimeMs)) {
    model.advance_ms(delta.0);
}

#[event]
pub(super) fn keyboard(input: event!(CharInput), model: model!(PostModel)) -> bool {
    match input.ch {
        'a' | 'A' => model.toggle_switch(0),
        's' | 'S' => model.toggle_switch(1),
        ' ' => model.toggle_running(),
        _ => return false,
    };
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelHandle;
    use crate::gallery::play::post::PostModal;
    use crate::input::event::{dispatch_input, input::InputEvent};

    #[derive(Default)]
    struct Unhandled(Option<char>);

    #[event]
    fn unhandled(input: event!(CharInput), seen: res!(mut Unhandled)) -> bool {
        seen.0 = Some(input.ch);
        true
    }

    #[test]
    fn keyboard_commands_keep_using_the_registered_model_after_the_original_handle_is_dropped() {
        let mut app = App::headless(1, 1);
        let model = app.add_model(PostModel::default());
        let observer = model.clone();
        app.world.insert_resource(Unhandled::default());
        app.on_event(CharInput::EVENT, keyboard::callback(model.clone()))
            .on_event(CharInput::EVENT, unhandled::callback());
        drop(model);

        for ch in ['a', 'S', ' '] {
            dispatch_input(&mut app.world, None, &InputEvent::CharInput { ch }, 0, 1, 1);
        }
        observer.read(|model| {
            assert_eq!(model.switch(0), 1);
            assert_eq!(model.switch(1), 1);
            assert!(model.running());
        });

        observer.open_manifests();
        for ch in ['A', 's', ' '] {
            dispatch_input(&mut app.world, None, &InputEvent::CharInput { ch }, 1, 1, 1);
        }
        observer.read(|model| {
            assert_eq!(model.modal(), PostModal::Manifests);
            assert_eq!(model.switch(0), 1);
            assert_eq!(model.switch(1), 1);
            assert!(!model.running());
        });
        assert_eq!(app.world.resource::<Unhandled>().unwrap().0, None);
        dispatch_input(
            &mut app.world,
            None,
            &InputEvent::CharInput { ch: 'x' },
            2,
            1,
            1,
        );
        assert_eq!(app.world.resource::<Unhandled>().unwrap().0, Some('x'));
        observer.read(|model| {
            assert_eq!(model.switch(0), 1);
            assert_eq!(model.switch(1), 1);
            assert!(!model.running());
        });
    }
}
