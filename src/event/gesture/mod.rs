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
