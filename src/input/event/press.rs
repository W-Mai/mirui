//! Notifications for captured single-pointer presses.

use alloc::vec::Vec;
use core::cell::RefCell;

use crate::core::reactive::retained::{self, InputLease};
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::ui::{IgnoreHitTest, Parent};

use super::contract::{EntityNotification, EntitySource, Event, EventKey};
use super::gesture::recognizer::SinglePress;
use super::input::InputEvent;
use super::listeners::{EventListeners, InputBranch, ListenerError};

/// A single-pointer press started on this node's captured path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pressed {
    pub pointer_id: u8,
    pub x: Fixed,
    pub y: Fixed,
}

/// A captured press ended normally. This does not imply activation or a tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Released {
    pub pointer_id: u8,
    pub x: Fixed,
    pub y: Fixed,
}

/// Why a single-pointer press stopped without normal release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PressCancelReason {
    Scroll,
    Gesture,
    Platform,
    Suspended,
    Unavailable,
}

/// A captured press was cancelled instead of released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PressCancelled {
    pub pointer_id: u8,
    pub x: Fixed,
    pub y: Fixed,
    pub reason: PressCancelReason,
}

macro_rules! press_events {
    ($($event:ident),+ $(,)?) => {
        $(impl Event for $event {
            type Route = EntityNotification;
            type Source = EntitySource;
            type Options = ();
        })+

        /// Press selectors available on component nodes.
        #[allow(non_upper_case_globals)]
        pub trait PressSelectors: crate::ecs::Component {
            $(const $event: EventKey<$event> = $event::EVENT;)+
        }
        impl<T: crate::ecs::Component> PressSelectors for T {}
    };
}
press_events!(Pressed, Released, PressCancelled);

#[derive(Clone, Copy)]
struct Receiver {
    entity: Entity,
    lease: Option<InputLease>,
    depth: usize,
}

#[derive(Clone, Copy)]
struct ActivePress {
    press: SinglePress,
    lease: Option<InputLease>,
}

#[derive(Default)]
struct Observation {
    registered: Vec<Entity>,
    receivers: Vec<Receiver>,
    active: Option<ActivePress>,
}

#[derive(Default)]
pub(super) struct PressObserver(RefCell<Observation>);

fn eligible(world: &World, entity: Entity) -> bool {
    !world.has::<IgnoreHitTest>(entity)
        && retained::enabled(world, entity)
        && InputBranch::scan(world, entity).is_ok_and(|branch| branch.allowed())
}

fn listened(
    world: &World,
    listeners: &EventListeners,
    node: Entity,
) -> Result<bool, ListenerError> {
    Ok(listeners.has_entity_listener::<Pressed>(world, node)?
        || listeners.has_entity_listener::<Released>(world, node)?
        || listeners.has_entity_listener::<PressCancelled>(world, node)?)
}

impl PressObserver {
    pub(super) fn reserve(
        &self,
        world: &World,
        listeners: &EventListeners,
        entity: Entity,
    ) -> Result<(), ListenerError> {
        let mut state = self.0.try_borrow_mut().map_err(|_| ListenerError::Busy)?;
        let mut index = 0;
        while index < state.registered.len() {
            let node = state.registered[index];
            if listened(world, listeners, node)? {
                index += 1;
            } else {
                state.registered.remove(index);
            }
        }
        if !state.registered.contains(&entity) {
            state
                .registered
                .try_reserve_exact(1)
                .map_err(|_| ListenerError::AllocationFailed)?;
        }
        let additional = (state.registered.len() + 1).saturating_sub(state.receivers.len());
        state
            .receivers
            .try_reserve_exact(additional)
            .map_err(|_| ListenerError::AllocationFailed)
    }

    pub(super) fn insert(&self, entity: Entity) {
        let mut state = self.0.borrow_mut();
        if !state.registered.contains(&entity) {
            state.registered.push(entity);
        }
    }

    fn begin(
        &self,
        world: &mut World,
        listeners: &EventListeners,
        press: SinglePress,
    ) -> Result<(), ListenerError> {
        if !eligible(world, press.target) {
            return Ok(());
        }
        let mut state = self.0.borrow_mut();
        state.receivers.clear();
        if state.registered.is_empty() {
            return Ok(());
        }
        for index in 0..state.registered.len() {
            let entity = state.registered[index];
            if !listened(world, listeners, entity)?
                || !eligible(world, entity)
                || !crate::ui::branch::is_in_subtree(world, entity, press.target)
            {
                continue;
            }
            let mut ancestor = entity;
            let mut depth = 0;
            while let Some(parent) = world.get::<Parent>(ancestor) {
                depth += 1;
                ancestor = parent.0;
            }
            state.receivers.push(Receiver {
                entity,
                lease: InputLease::capture(world, entity),
                depth,
            });
        }
        state
            .receivers
            .sort_unstable_by_key(|receiver| core::cmp::Reverse(receiver.depth));
        state.active = Some(ActivePress {
            press,
            lease: InputLease::capture(world, press.target),
        });
        drop(state);
        self.publish(
            world,
            Pressed {
                pointer_id: press.pointer.1,
                x: press.x,
                y: press.y,
            },
        );
        Ok(())
    }

    fn publish<E: Event<Route = EntityNotification, Source = EntitySource> + Copy>(
        &self,
        world: &mut World,
        event: E,
    ) {
        let count = self.0.borrow().receivers.len();
        for index in 0..count {
            let receiver = self.0.borrow().receivers[index];
            if receiver.lease.is_some_and(|lease| !lease.valid(world))
                || !world.is_alive(receiver.entity)
            {
                continue;
            }
            if let Err(error) = super::emission::submit_if_listened(world, receiver.entity, event) {
                crate::warn!(target: "mirui::event::press", "press notification skipped: {:?}", error);
            }
        }
    }

    pub(super) fn cancel(&self, world: &mut World, reason: PressCancelReason) {
        let active = self.0.borrow_mut().active.take();
        if let Some(active) = active {
            self.publish(
                world,
                PressCancelled {
                    pointer_id: active.press.pointer.1,
                    x: active.press.x,
                    y: active.press.y,
                    reason,
                },
            );
        }
    }

    pub(super) fn observe(&self, world: &mut World) {
        let active = self.0.borrow().active;
        if active.is_some_and(|active| {
            active.lease.is_some_and(|lease| !lease.valid(world))
                || !eligible(world, active.press.target)
        }) {
            self.cancel(world, PressCancelReason::Unavailable);
        }
    }

    pub(super) fn input(
        &self,
        world: &mut World,
        listeners: &EventListeners,
        input: &InputEvent,
        can_start: bool,
        after: Option<SinglePress>,
        scroll_claimed: bool,
    ) -> Result<(), ListenerError> {
        {
            let mut state = self.0.borrow_mut();
            if let Some(active) = &mut state.active
                && input.pointer_identity() == Some(active.press.pointer)
                && let InputEvent::PointerMove { x, y, .. }
                | InputEvent::PointerUp { x, y, .. }
                | InputEvent::PointerCancel { x, y, .. } = input
            {
                active.press.x = *x;
                active.press.y = *y;
            }
        }
        self.observe(world);
        if can_start
            && matches!(input, InputEvent::PointerDown { .. })
            && let Some(press) = after
        {
            return self.begin(world, listeners, press);
        }
        let active = self.0.borrow().active;
        let Some(active) = active else { return Ok(()) };
        if after.is_some() {
            return Ok(());
        }
        if scroll_claimed {
            self.cancel(world, PressCancelReason::Scroll);
        } else if matches!(input, InputEvent::PointerUp { .. })
            && input.pointer_identity() == Some(active.press.pointer)
        {
            self.0.borrow_mut().active = None;
            self.publish(
                world,
                Released {
                    pointer_id: active.press.pointer.1,
                    x: active.press.x,
                    y: active.press.y,
                },
            );
        } else {
            let reason = if matches!(input, InputEvent::PointerCancel { .. }) {
                PressCancelReason::Platform
            } else {
                PressCancelReason::Gesture
            };
            self.cancel(world, reason);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
