use alloc::vec::Vec;

use super::event::GestureEvent;
use super::recognizer::GestureRecognizer;

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
