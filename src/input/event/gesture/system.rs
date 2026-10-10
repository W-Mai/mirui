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
pub(crate) enum GestureDrainError<E> {
    Busy,
    Dispatch(E),
}

struct PendingGestures {
    buffer: Vec<GestureEvent>,
    next: Option<usize>,
}

struct ActiveBatch<'a>(&'a RefCell<PendingGestures>);

impl Drop for ActiveBatch<'_> {
    fn drop(&mut self) {
        let mut pending = self.0.borrow_mut();
        pending.buffer.clear();
        pending.next = None;
    }
}

pub(crate) fn discard_pending_in_subtree(world: &World, root: crate::ecs::Entity) {
    let Some(system) = world.resource::<GestureSystem>() else {
        return;
    };
    let mut pending = system.pending.borrow_mut();
    let next = pending.next.unwrap_or(0);
    let mut index = 0;
    pending.buffer.retain(|event| {
        let keep = index < next || !crate::ui::branch::is_in_subtree(world, root, event.target());
        index += 1;
        keep
    });
}

pub(crate) fn drain_gestures<E>(
    world: &mut World,
    mut dispatch: impl FnMut(&mut World, &GestureEvent) -> Result<(), E>,
) -> Result<usize, GestureDrainError<E>> {
    let Some(pending) = world
        .resource::<GestureSystem>()
        .map(|system| Rc::clone(&system.pending))
    else {
        return Ok(0);
    };
    {
        let mut batch = pending
            .try_borrow_mut()
            .map_err(|_| GestureDrainError::Busy)?;
        if batch.next.is_some() {
            return Err(GestureDrainError::Busy);
        }
        let system = world
            .resource_mut::<GestureSystem>()
            .expect("gesture resource remains installed before dispatch");
        core::mem::swap(&mut batch.buffer, &mut system.events.buffer);
        batch.next = Some(0);
    }
    let _active = ActiveBatch(&pending);
    let mut count = 0;
    loop {
        let event = {
            let mut batch = pending.borrow_mut();
            let next = batch.next.expect("active gesture batch");
            let Some(event) = batch.buffer.get(next).cloned() else {
                break;
            };
            batch.next = Some(next + 1);
            event
        };
        dispatch(world, &event).map_err(GestureDrainError::Dispatch)?;
        count += 1;
    }
    Ok(count)
}

/// Combined resource holding both the recognizer state machine and the
/// output event buffers. Stored as a single World resource so one
/// `resource_mut` call suffices.
pub struct GestureSystem {
    pub recognizer: GestureRecognizer,
    pub events: GestureEvents,
    pending: Rc<RefCell<PendingGestures>>,
}

impl Default for GestureSystem {
    fn default() -> Self {
        Self {
            recognizer: GestureRecognizer::default(),
            events: GestureEvents::new(),
            pending: Rc::new(RefCell::new(PendingGestures {
                buffer: Vec::with_capacity(4),
                next: None,
            })),
        }
    }
}

#[cfg(test)]
mod tests;
