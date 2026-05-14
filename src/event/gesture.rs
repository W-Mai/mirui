use alloc::vec::Vec;

use crate::ecs::Entity;
use crate::surface::InputEvent;
use crate::types::Fixed;

#[derive(Clone, Debug)]
pub enum GestureEvent {
    Tap {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    LongPress {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    DragStart {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
    DragMove {
        x: Fixed,
        y: Fixed,
        dx: Fixed,
        dy: Fixed,
        target: Entity,
    },
    DragEnd {
        x: Fixed,
        y: Fixed,
        target: Entity,
    },
}

impl GestureEvent {
    pub fn target(&self) -> Entity {
        match self {
            Self::Tap { target, .. }
            | Self::LongPress { target, .. }
            | Self::DragStart { target, .. }
            | Self::DragMove { target, .. }
            | Self::DragEnd { target, .. } => *target,
        }
    }
}

#[derive(Default)]
pub struct GestureEvents {
    pub buffer: Vec<GestureEvent>,
}

impl GestureEvents {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4),
        }
    }

    pub fn push(&mut self, event: GestureEvent) {
        self.buffer.push(event);
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    pub fn drain(&mut self) -> alloc::vec::Drain<'_, GestureEvent> {
        self.buffer.drain(..)
    }
}

/// Combined resource holding both the recognizer state machine and the
/// output event buffer. Stored as a single World resource so one
/// `resource_mut` call suffices.
#[derive(Default)]
pub struct GestureSystem {
    pub recognizer: GestureRecognizer,
    pub events: GestureEvents,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GestureState {
    #[default]
    Idle,
    Pending,
    Dragging,
    LongPressed,
}

const DRAG_THRESHOLD: i32 = 10;
const LONG_PRESS_MS: u16 = 500;

#[derive(Default)]
pub struct GestureRecognizer {
    state: GestureState,
    pointer_id: u8,
    start_x: Fixed,
    start_y: Fixed,
    current_x: Fixed,
    current_y: Fixed,
    down_elapsed_ms: u32,
    target: Option<Entity>,
    pub scroll_claimed: bool,
}

impl GestureRecognizer {
    pub fn new() -> Self {
        Self {
            state: GestureState::Idle,
            pointer_id: 0,
            start_x: Fixed::ZERO,
            start_y: Fixed::ZERO,
            current_x: Fixed::ZERO,
            current_y: Fixed::ZERO,
            down_elapsed_ms: 0,
            target: None,
            scroll_claimed: false,
        }
    }

    pub fn update(
        &mut self,
        event: &InputEvent,
        elapsed_ms: u32,
        hit_target: Option<Entity>,
        events_out: &mut GestureEvents,
    ) {
        if self.scroll_claimed {
            if matches!(event, InputEvent::PointerUp { .. }) {
                self.reset();
            }
            return;
        }

        match event {
            InputEvent::PointerDown { id, x, y } => {
                if self.state != GestureState::Idle {
                    return;
                }
                self.state = GestureState::Pending;
                self.pointer_id = *id;
                self.start_x = *x;
                self.start_y = *y;
                self.current_x = *x;
                self.current_y = *y;
                self.down_elapsed_ms = elapsed_ms;
                self.target = hit_target;
            }
            InputEvent::PointerMove { id, x, y } => {
                if *id != self.pointer_id {
                    return;
                }
                self.current_x = *x;
                self.current_y = *y;

                match self.state {
                    GestureState::Pending => {
                        let dx = (*x - self.start_x).to_int().abs();
                        let dy = (*y - self.start_y).to_int().abs();
                        if dx + dy > DRAG_THRESHOLD {
                            self.state = GestureState::Dragging;
                            if let Some(target) = self.target {
                                events_out.push(GestureEvent::DragStart {
                                    x: self.start_x,
                                    y: self.start_y,
                                    target,
                                });
                            }
                        }
                    }
                    GestureState::Dragging => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::DragMove {
                                x: *x,
                                y: *y,
                                dx: *x - self.start_x,
                                dy: *y - self.start_y,
                                target,
                            });
                        }
                    }
                    _ => {}
                }
            }
            InputEvent::PointerUp { id, x, y } => {
                if *id != self.pointer_id {
                    return;
                }
                match self.state {
                    GestureState::Pending => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::Tap {
                                x: *x,
                                y: *y,
                                target,
                            });
                        }
                    }
                    GestureState::Dragging => {
                        if let Some(target) = self.target {
                            events_out.push(GestureEvent::DragEnd {
                                x: *x,
                                y: *y,
                                target,
                            });
                        }
                    }
                    GestureState::LongPressed => {}
                    GestureState::Idle => {}
                }
                self.reset();
            }
            _ => {}
        }
    }

    pub fn check_long_press(&mut self, elapsed_ms: u32, events_out: &mut GestureEvents) {
        if self.state == GestureState::Pending && !self.scroll_claimed {
            let held = elapsed_ms.wrapping_sub(self.down_elapsed_ms);
            if held >= LONG_PRESS_MS as u32 {
                self.state = GestureState::LongPressed;
                if let Some(target) = self.target {
                    events_out.push(GestureEvent::LongPress {
                        x: self.current_x,
                        y: self.current_y,
                        target,
                    });
                }
            }
        }
    }

    fn reset(&mut self) {
        self.state = GestureState::Idle;
        self.target = None;
        self.scroll_claimed = false;
    }
}

#[cfg(test)]
mod tests {
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
