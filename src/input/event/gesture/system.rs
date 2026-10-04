use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::ecs::World;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GestureDrainError {
    Busy,
}

pub(crate) fn drain_gestures(
    world: &mut World,
    mut dispatch: impl FnMut(&mut World, &GestureEvent),
) -> Result<usize, GestureDrainError> {
    let Some(pending) = world
        .resource::<GestureSystem>()
        .map(|system| Rc::clone(&system.pending))
    else {
        return Ok(0);
    };
    let mut pending = pending
        .try_borrow_mut()
        .map_err(|_| GestureDrainError::Busy)?;
    let system = world
        .resource_mut::<GestureSystem>()
        .expect("gesture resource remains installed before dispatch");
    core::mem::swap(&mut *pending, &mut system.events.buffer);
    let count = pending.len();
    for event in pending.drain(..) {
        dispatch(world, &event);
    }
    Ok(count)
}

/// Combined resource holding both the recognizer state machine and the
/// output event buffers. Stored as a single World resource so one
/// `resource_mut` call suffices.
pub struct GestureSystem {
    pub recognizer: GestureRecognizer,
    pub events: GestureEvents,
    pending: Rc<RefCell<Vec<GestureEvent>>>,
}

impl Default for GestureSystem {
    fn default() -> Self {
        Self {
            recognizer: GestureRecognizer::default(),
            events: GestureEvents::new(),
            pending: Rc::new(RefCell::new(Vec::with_capacity(4))),
        }
    }
}

#[cfg(test)]
mod tests;
