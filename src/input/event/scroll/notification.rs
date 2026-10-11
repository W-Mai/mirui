//! Notifications of committed scroll displacement and motion completion.

use alloc::vec::Vec;
use core::cell::RefCell;

use crate::core::reactive::retained::{self, InputLease};
use crate::ecs::{Entity, World};

use super::{ScrollMotion, ScrollOffset};
use crate::input::event::contract::{EntityNotification, Event, EventKey, ScrollSource};
use crate::input::event::listeners::{EventListeners, InputBranch, ListenerError};

/// Content started moving from this offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollStarted {
    pub offset: ScrollOffset,
}

/// Actual content displacement, including fractional and elastic positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollChanged {
    pub old: ScrollOffset,
    pub new: ScrollOffset,
}

/// Why a scroll motion ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollEndReason {
    Settled,
    Cancelled,
}

/// Content stopped moving, after any inertia or rebound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollEnded {
    pub offset: ScrollOffset,
    pub reason: ScrollEndReason,
}

macro_rules! scroll_events {
    ($($event:ident),+ $(,)?) => {
        $(impl Event for $event {
            type Route = EntityNotification;
            type Source = ScrollSource;
            type Options = ();
        })+

        /// Scroll notifications on nodes with a [`ScrollOffset`].
        #[allow(non_upper_case_globals)]
        pub trait ScrollSelectors: crate::ecs::Component {
            $(const $event: EventKey<$event> = $event::EVENT;)+
        }
        impl<T: crate::ecs::Component> ScrollSelectors for T {}
    };
}
scroll_events!(ScrollStarted, ScrollChanged, ScrollEnded);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Joining,
    Moving,
}

struct Baseline {
    lease: Option<InputLease>,
    offset: ScrollOffset,
    phase: Phase,
    cancelled: bool,
}

struct Observation {
    entity: Entity,
    baseline: Option<Baseline>,
}

fn active(world: &World, entity: Entity) -> bool {
    world
        .resource::<ScrollMotion>()
        .is_some_and(|motion| motion.target_entity == Some(entity))
}

fn listened(
    world: &World,
    listeners: &EventListeners,
    entity: Entity,
) -> Result<bool, ListenerError> {
    Ok(
        listeners.has_entity_listener::<ScrollStarted>(world, entity)?
            || listeners.has_entity_listener::<ScrollChanged>(world, entity)?
            || listeners.has_entity_listener::<ScrollEnded>(world, entity)?,
    )
}

fn baseline(world: &World, entity: Entity) -> Option<Baseline> {
    let offset = *world.get::<ScrollOffset>(entity)?;
    if !retained::enabled(world, entity)
        || !InputBranch::scan(world, entity).is_ok_and(|branch| branch.allowed())
        || world
            .resource::<crate::ui::WidgetRoot>()
            .is_some_and(|root| !crate::ui::branch::is_in_subtree(world, root.0, entity))
    {
        return None;
    }
    Some(Baseline {
        lease: InputLease::capture(world, entity),
        offset,
        phase: if active(world, entity) {
            Phase::Joining
        } else {
            Phase::Idle
        },
        cancelled: false,
    })
}

#[derive(Default)]
pub(in crate::input::event) struct ScrollObservers(RefCell<Vec<Observation>>);

impl ScrollObservers {
    pub(in crate::input::event) fn reserve(
        &self,
        world: &World,
        listeners: &EventListeners,
        entity: Entity,
    ) -> Result<(), ListenerError> {
        let mut entries = self.0.try_borrow_mut().map_err(|_| ListenerError::Busy)?;
        let mut index = 0;
        while index < entries.len() {
            if listened(world, listeners, entries[index].entity)? {
                index += 1;
            } else {
                entries.remove(index);
            }
        }
        if !entries.iter().any(|entry| entry.entity == entity) {
            entries
                .try_reserve_exact(1)
                .map_err(|_| ListenerError::AllocationFailed)?;
        }
        Ok(())
    }

    pub(in crate::input::event) fn insert(&self, world: &World, entity: Entity) {
        let mut entries = self.0.borrow_mut();
        if !entries.iter().any(|entry| entry.entity == entity) {
            entries.push(Observation {
                entity,
                baseline: baseline(world, entity),
            });
        }
    }

    pub(in crate::input::event) fn cancel(&self, entity: Entity) {
        if let Some(baseline) = self
            .0
            .borrow_mut()
            .iter_mut()
            .find(|entry| entry.entity == entity)
            .and_then(|entry| entry.baseline.as_mut())
        {
            baseline.cancelled = true;
        }
    }

    pub(in crate::input::event) fn observe(
        &self,
        world: &mut World,
        listeners: &EventListeners,
    ) -> Result<(), ListenerError> {
        let mut entries = self.0.try_borrow_mut().map_err(|_| ListenerError::Busy)?;
        let mut index = 0;
        while index < entries.len() {
            let entity = entries[index].entity;
            if !listened(world, listeners, entity)? {
                entries.remove(index);
                continue;
            }
            let entry = &mut entries[index];
            index += 1;
            let Some(next) = baseline(world, entity) else {
                entry.baseline = None;
                continue;
            };
            let Some(old) = entry.baseline.as_mut().filter(|old| {
                old.lease.is_some() == next.lease.is_some()
                    && old.lease.is_none_or(|lease| lease.valid(world))
            }) else {
                entry.baseline = Some(next);
                continue;
            };
            let changed = (old.offset != next.offset).then_some(ScrollChanged {
                old: old.offset,
                new: next.offset,
            });
            let started = changed
                .filter(|_| old.phase == Phase::Idle)
                .map(|change| ScrollStarted { offset: change.old });
            if changed.is_some() {
                old.phase = Phase::Moving;
            }
            let ended = (old.phase == Phase::Moving && (old.cancelled || !active(world, entity)))
                .then_some(ScrollEnded {
                    offset: next.offset,
                    reason: if old.cancelled {
                        ScrollEndReason::Cancelled
                    } else {
                        ScrollEndReason::Settled
                    },
                });
            if old.cancelled || !active(world, entity) {
                old.phase = Phase::Idle;
            }
            old.offset = next.offset;
            old.cancelled = false;
            if let Some(event) = started {
                notify(world, entity, event);
            }
            if let Some(event) = changed {
                notify(world, entity, event);
            }
            if let Some(event) = ended {
                notify(world, entity, event);
            }
        }
        Ok(())
    }
}

fn notify<E: Event<Route = EntityNotification, Source = ScrollSource>>(
    world: &mut World,
    entity: Entity,
    event: E,
) {
    if let Err(error) = crate::input::event::emission::submit_if_listened(world, entity, event) {
        crate::warn!(target: "mirui::event::scroll", "scroll notification skipped: {:?}", error);
    }
}

#[cfg(test)]
mod tests;
