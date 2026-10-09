//! Node size notifications in logical layout coordinates.

use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::ecs::{Component, Entity, World};
use crate::types::Fixed;

use super::contract::{Event, EventKey, LayoutNotification, LayoutSource};
use super::listeners::{EventListeners, ListenerError, ListenerId};

/// Width and height before visual transforms and output scaling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutSize {
    pub width: Fixed,
    pub height: Fixed,
}

/// A node's first available size or a change from its last published size.
///
/// Delivery participates in layout settling. More than one distinct size can be published in a frame; this is not a final-layout notification. Position, scrolling, and visual transforms do not trigger it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeChanged {
    pub old: Option<LayoutSize>,
    pub new: LayoutSize,
}

impl Event for SizeChanged {
    type Route = LayoutNotification;
    type Source = LayoutSource;
    type Options = ();
}

/// Layout selectors available on widget and container nodes.
#[allow(non_upper_case_globals)]
pub trait LayoutSelectors: Component {
    const SizeChanged: EventKey<SizeChanged> = SizeChanged::EVENT;
}

impl<W: Component> LayoutSelectors for W {}

struct SizeObservation {
    listener: ListenerId,
    entity: Entity,
    published: Option<LayoutSize>,
    pending: Option<LayoutSize>,
}

#[derive(Default)]
pub(super) struct SizeObservers {
    entries: RefCell<Vec<SizeObservation>>,
    active: Cell<bool>,
}

impl SizeObservers {
    pub(super) fn reserve(
        &self,
        listeners: &EventListeners,
        world: &World,
    ) -> Result<(), ListenerError> {
        if self.active.get() {
            return Err(ListenerError::Busy);
        }
        let mut entries = self
            .entries
            .try_borrow_mut()
            .map_err(|_| ListenerError::Busy)?;
        entries.retain(|entry| world.is_alive(entry.entity) && listeners.contains(entry.listener));
        entries
            .try_reserve_exact(1)
            .map_err(|_| ListenerError::AllocationFailed)
    }

    pub(super) fn insert(&self, listener: ListenerId, entity: Entity) {
        self.entries.borrow_mut().push(SizeObservation {
            listener,
            entity,
            published: None,
            pending: None,
        });
    }

    pub(super) fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    pub(super) fn capture(
        &self,
        listeners: &EventListeners,
        world: &World,
        laid_out: &[Entity],
        invoke: bool,
    ) -> Result<(usize, bool), ListenerError> {
        if self.active.get() {
            return Err(ListenerError::Busy);
        }
        let mut entries = self
            .entries
            .try_borrow_mut()
            .map_err(|_| ListenerError::Busy)?;
        entries.retain(|entry| world.is_alive(entry.entity) && listeners.contains(entry.listener));
        let mut changed = false;
        for entry in &mut *entries {
            entry.pending = None;
            if !laid_out.contains(&entry.entity) {
                continue;
            }
            let Some(rect) = world.get::<crate::ui::ComputedRect>(entry.entity) else {
                continue;
            };
            let size = LayoutSize {
                width: rect.0.w,
                height: rect.0.h,
            };
            if entry.published != Some(size) {
                changed = true;
                if invoke {
                    entry.pending = Some(size);
                }
            }
        }
        self.active.set(true);
        Ok((entries.len(), changed))
    }

    pub(super) fn take(&self, index: usize) -> Option<(ListenerId, Entity, SizeChanged)> {
        let mut entries = self.entries.borrow_mut();
        let entry = &mut entries[index];
        let new = entry.pending.take()?;
        let old = entry.published.replace(new);
        Some((entry.listener, entry.entity, SizeChanged { old, new }))
    }

    pub(super) fn finish(&self) {
        for entry in &mut *self.entries.borrow_mut() {
            entry.pending = None;
        }
        self.active.set(false);
    }
}

#[cfg(test)]
mod tests;
