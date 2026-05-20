pub mod event;
pub mod recognizer;
pub mod system;

pub use event::GestureEvent;
pub use recognizer::GestureRecognizer;
pub use system::{GestureEvents, GestureSystem};

#[cfg(test)]
mod tests {
    use crate::ecs::Entity;
    use crate::event::input::InputEvent;
    use crate::types::Fixed;

    use super::recognizer::GestureState;
    use super::*;

    fn entity(id: u32) -> Entity {
        Entity { id, generation: 0 }
    }

    #[test]
    fn tap_on_quick_release() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(1);

        rec.update(
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            0,
            Some(target),
            &mut events,
        );
        assert!(events.buffer.is_empty());

        rec.update(
            &InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            100,
            None,
            &mut events,
        );
        assert_eq!(events.buffer.len(), 1);
        assert!(matches!(events.buffer[0], GestureEvent::Tap { .. }));
    }

    #[test]
    fn drag_after_threshold() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(2);

        rec.update(
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(10),
                y: Fixed::from_int(10),
            },
            0,
            Some(target),
            &mut events,
        );

        rec.update(
            &InputEvent::PointerMove {
                id: 0,
                x: Fixed::from_int(25),
                y: Fixed::from_int(10),
            },
            50,
            None,
            &mut events,
        );
        assert_eq!(events.buffer.len(), 1);
        assert!(matches!(events.buffer[0], GestureEvent::DragStart { .. }));

        events.clear();
        rec.update(
            &InputEvent::PointerMove {
                id: 0,
                x: Fixed::from_int(30),
                y: Fixed::from_int(10),
            },
            80,
            None,
            &mut events,
        );
        assert_eq!(events.buffer.len(), 1);
        assert!(matches!(events.buffer[0], GestureEvent::DragMove { .. }));

        events.clear();
        rec.update(
            &InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(30),
                y: Fixed::from_int(10),
            },
            100,
            None,
            &mut events,
        );
        assert_eq!(events.buffer.len(), 1);
        assert!(matches!(events.buffer[0], GestureEvent::DragEnd { .. }));
    }

    #[test]
    fn long_press_fires() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(3);

        rec.update(
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            1000,
            Some(target),
            &mut events,
        );

        rec.check_long_press(1400, &mut events);
        assert!(events.buffer.is_empty());

        rec.check_long_press(1501, &mut events);
        assert_eq!(events.buffer.len(), 1);
        assert!(matches!(events.buffer[0], GestureEvent::LongPress { .. }));
    }

    fn down(
        rec: &mut GestureRecognizer,
        id: u8,
        x: i32,
        y: i32,
        t: u32,
        target: Option<Entity>,
        ev: &mut GestureEvents,
    ) {
        rec.update(
            &InputEvent::PointerDown {
                id,
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
            },
            t,
            target,
            ev,
        );
    }

    fn motion(rec: &mut GestureRecognizer, id: u8, x: i32, y: i32, t: u32, ev: &mut GestureEvents) {
        rec.update(
            &InputEvent::PointerMove {
                id,
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
            },
            t,
            None,
            ev,
        );
    }

    fn up(rec: &mut GestureRecognizer, id: u8, x: i32, y: i32, t: u32, ev: &mut GestureEvents) {
        rec.update(
            &InputEvent::PointerUp {
                id,
                x: Fixed::from_int(x),
                y: Fixed::from_int(y),
            },
            t,
            None,
            ev,
        );
    }

    #[test]
    fn pinch_recognised_when_distance_grows() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(10);

        // f0 at (100, 100), f1 at (200, 100) → initial dist 100.
        down(&mut rec, 0, 100, 100, 0, Some(target), &mut events);
        down(&mut rec, 1, 200, 100, 5, None, &mut events);
        events.clear();

        // Pull f1 to (300, 100) → dist 200, scale 2.0, well over threshold.
        motion(&mut rec, 1, 300, 100, 20, &mut events);

        let pinch = events
            .buffer
            .iter()
            .find(|e| matches!(e, GestureEvent::Pinch { .. }));
        assert!(pinch.is_some(), "expected Pinch, got {:?}", events.buffer);
        if let Some(GestureEvent::Pinch {
            scale_delta,
            target: t,
            ..
        }) = pinch
        {
            // scale ≈ 2.0
            assert!(
                scale_delta.to_int() == 2 || scale_delta.to_int() == 1,
                "scale not ~2: {:?}",
                scale_delta
            );
            assert_eq!(*t, target);
        }
    }

    #[test]
    fn rotate_recognised_when_angle_changes() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(11);

        // Two fingers on horizontal axis, separated by 100 px.
        down(&mut rec, 0, 100, 100, 0, Some(target), &mut events);
        down(&mut rec, 1, 200, 100, 5, None, &mut events);
        events.clear();

        // Rotate the pair 90° — f1 moves to (150, 50), f0 to (150, 150).
        motion(&mut rec, 0, 150, 150, 20, &mut events);
        motion(&mut rec, 1, 150, 50, 25, &mut events);

        let rotate = events
            .buffer
            .iter()
            .find(|e| matches!(e, GestureEvent::Rotate { .. }));
        assert!(rotate.is_some(), "expected Rotate, got {:?}", events.buffer);
    }

    #[test]
    fn second_finger_during_drag_is_ignored() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(12);

        // Single-finger drag.
        down(&mut rec, 0, 0, 0, 0, Some(target), &mut events);
        motion(&mut rec, 0, 50, 0, 10, &mut events);
        assert!(matches!(
            events.buffer.last(),
            Some(GestureEvent::DragStart { .. })
        ));
        events.clear();

        // Second finger arrives mid-drag — must NOT promote to MultiPending.
        down(&mut rec, 1, 100, 0, 15, None, &mut events);
        motion(&mut rec, 0, 75, 0, 20, &mut events);

        // Drag continues on f0; no Pinch / Rotate emitted.
        assert!(
            events
                .buffer
                .iter()
                .any(|e| matches!(e, GestureEvent::DragMove { .. }))
        );
        assert!(
            !events
                .buffer
                .iter()
                .any(|e| matches!(e, GestureEvent::Pinch { .. }))
        );
        assert!(
            !events
                .buffer
                .iter()
                .any(|e| matches!(e, GestureEvent::Rotate { .. }))
        );
    }

    #[test]
    fn lift_one_of_two_fingers_ends_multi_touch() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(13);

        down(&mut rec, 0, 100, 100, 0, Some(target), &mut events);
        down(&mut rec, 1, 200, 100, 5, None, &mut events);
        motion(&mut rec, 1, 300, 100, 20, &mut events); // pinch active
        events.clear();

        // Lift f1 → multi-touch ends, recogniser back to Idle.
        up(&mut rec, 1, 300, 100, 30, &mut events);
        assert_eq!(rec.state, GestureState::Idle);

        // f0 still moving must NOT produce DragMove (recogniser fully reset).
        motion(&mut rec, 0, 200, 200, 50, &mut events);
        assert!(
            !events
                .buffer
                .iter()
                .any(|e| matches!(e, GestureEvent::DragMove { .. }))
        );
    }

    #[test]
    fn scroll_claimed_suppresses_gesture() {
        let mut rec = GestureRecognizer::new();
        let mut events = GestureEvents::new();
        let target = entity(4);

        rec.update(
            &InputEvent::PointerDown {
                id: 0,
                x: Fixed::from_int(10),
                y: Fixed::from_int(10),
            },
            0,
            Some(target),
            &mut events,
        );
        rec.scroll_claimed = true;

        rec.update(
            &InputEvent::PointerMove {
                id: 0,
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            50,
            None,
            &mut events,
        );
        assert!(events.buffer.is_empty());

        rec.update(
            &InputEvent::PointerUp {
                id: 0,
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            100,
            None,
            &mut events,
        );
        assert!(events.buffer.is_empty());
        assert_eq!(rec.state, GestureState::Idle);
    }
}
