//! Hover notifications from pointer membership in input paths.

use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::RefCell;

use crate::core::reactive::retained::InputLease;
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::ui::{IgnoreHitTest, Parent};

use super::PointerCursor;
use super::contract::{EntityNotification, EntitySource, Event, EventKey};
use super::hit_test;
use super::input::{InputEvent, PointerKind};
use super::listeners::{EventListeners, InputBranch, ListenerError};

/// A hover-capable pointer entered or left this node's input path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoverChanged {
    pub hovered: bool,
    pub x: Fixed,
    pub y: Fixed,
}

impl Event for HoverChanged {
    type Route = EntityNotification;
    type Source = EntitySource;
    type Options = ();
}

/// Hover selectors available on component nodes.
#[allow(non_upper_case_globals)]
pub trait HoverSelectors: crate::ecs::Component {
    const HoverChanged: EventKey<HoverChanged> = HoverChanged::EVENT;
}
impl<T: crate::ecs::Component> HoverSelectors for T {}

const POINTER_CAPACITY: usize = 18;

#[derive(Clone, Copy)]
struct Membership {
    lease: Option<InputLease>,
    depth: usize,
    pointer: (PointerKind, u8),
    pointers: u32,
    x: Fixed,
    y: Fixed,
}

struct Observation {
    entity: Entity,
    published: Option<Membership>,
    pending: Option<Membership>,
}

#[derive(Default)]
struct Pointer {
    cursor: PointerCursor,
    target: Option<(Entity, Option<InputLease>)>,
    occupied: bool,
}

impl Pointer {
    fn target(&self, world: &World) -> Option<Entity> {
        if !self.cursor.present || !self.cursor.hover_capable {
            return None;
        }
        self.target
            .filter(|(target, lease)| {
                lease.is_none_or(|lease| lease.valid(world))
                    && crate::core::reactive::retained::enabled(world, *target)
                    && !world.has::<IgnoreHitTest>(*target)
                    && InputBranch::scan(world, *target).is_ok_and(|branch| branch.allowed())
            })
            .map(|(target, _)| target)
    }
}

#[derive(Default)]
struct Observations {
    entries: Vec<Observation>,
    pointers: Vec<Pointer>,
}

impl Observations {
    fn prune(&mut self, world: &World, listeners: &EventListeners) -> Result<(), ListenerError> {
        let mut index = 0;
        while index < self.entries.len() {
            if !listeners.has_entity_listener::<HoverChanged>(world, self.entries[index].entity)? {
                self.entries.remove(index);
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn membership(&self, world: &World, entity: Entity) -> Option<Membership> {
        if world.has::<IgnoreHitTest>(entity)
            || !crate::core::reactive::retained::enabled(world, entity)
            || !InputBranch::scan(world, entity).is_ok_and(|branch| branch.allowed())
        {
            return None;
        }
        let mut membership: Option<Membership> = None;
        for (index, tracked) in self.pointers.iter().enumerate() {
            let Some(target) = tracked.target(world) else {
                continue;
            };
            let pointer = &tracked.cursor;
            if !crate::ui::branch::is_in_subtree(world, entity, target) {
                continue;
            }
            let mut depth = 0;
            let mut ancestor = entity;
            while let Some(parent) = world.get::<Parent>(ancestor) {
                depth += 1;
                if depth > world.live_entity_count() {
                    return None;
                }
                ancestor = parent.0;
            }
            let next = membership.get_or_insert(Membership {
                lease: InputLease::capture(world, entity),
                depth,
                pointer: (pointer.kind, pointer.id),
                pointers: 0,
                x: pointer.x,
                y: pointer.y,
            });
            next.pointers |= 1 << index;
        }
        membership
    }
}

#[derive(Default)]
pub(super) struct HoverObservers(RefCell<Observations>);

impl HoverObservers {
    pub(super) fn reserve(
        &self,
        world: &World,
        listeners: &EventListeners,
        entity: Entity,
    ) -> Result<(), ListenerError> {
        let mut state = self.0.try_borrow_mut().map_err(|_| ListenerError::Busy)?;
        state.prune(world, listeners)?;
        if !state.entries.iter().any(|entry| entry.entity == entity) {
            state
                .entries
                .try_reserve_exact(1)
                .map_err(|_| ListenerError::AllocationFailed)?;
        }
        let additional = POINTER_CAPACITY.saturating_sub(state.pointers.len());
        state
            .pointers
            .try_reserve_exact(additional)
            .map_err(|_| ListenerError::AllocationFailed)
    }

    pub(super) fn insert(&self, world: &World, entity: Entity) {
        let mut state = self.0.borrow_mut();
        if state.entries.iter().any(|entry| entry.entity == entity) {
            return;
        }
        let published = state.membership(world, entity);
        state.entries.push(Observation {
            entity,
            published,
            pending: None,
        });
    }

    pub(super) fn input(&self, input: &InputEvent, width: u16, height: u16) {
        let mut state = self.0.borrow_mut();
        if state.entries.is_empty() {
            return;
        }
        let Some((kind, id)) = input.pointer_identity() else {
            return;
        };
        if !matches!(kind, PointerKind::Mouse | PointerKind::Pen) {
            return;
        }
        let index = match state
            .pointers
            .iter()
            .position(|pointer| (pointer.cursor.kind, pointer.cursor.id) == (kind, id))
        {
            Some(index) => index,
            None if matches!(
                input,
                InputEvent::PointerDown { .. }
                    | InputEvent::PointerMove { .. }
                    | InputEvent::PointerEnter { .. }
            ) =>
            {
                if let Some(index) = state.pointers.iter().position(|pointer| !pointer.occupied) {
                    for entry in &mut state.entries {
                        if let Some(old) = &mut entry.published {
                            old.pointers &= !(1 << index);
                        }
                    }
                    state.pointers[index] = Pointer::default();
                    index
                } else {
                    if state.pointers.len() == POINTER_CAPACITY {
                        crate::warn!(target: "mirui::event::hover", "hover pointer capacity exhausted");
                        return;
                    }
                    state.pointers.push(Pointer::default());
                    state.pointers.len() - 1
                }
            }
            None => return,
        };
        let pointer = &mut state.pointers[index];
        pointer.cursor.update(input, width, height);
        pointer.occupied = match input {
            InputEvent::PointerLeave { .. } | InputEvent::PointerCancel { .. } => {
                pointer.cursor.down
            }
            InputEvent::PointerUp { .. } => pointer.cursor.present || pointer.cursor.down,
            _ => true,
        };
    }

    pub(super) fn cancel(&self) {
        for pointer in &mut self.0.borrow_mut().pointers {
            pointer.cursor.present = false;
            pointer.cursor.down = false;
            pointer.occupied = false;
        }
    }

    pub(super) fn observe(
        &self,
        world: &mut World,
        listeners: &EventListeners,
    ) -> Result<(), ListenerError> {
        let mut state = self.0.try_borrow_mut().map_err(|_| ListenerError::Busy)?;
        state.prune(world, listeners)?;
        if state.entries.is_empty() {
            state.pointers.clear();
            return Ok(());
        }
        let (layout, visual) = hit_test::geometry_changes(world);
        let pending_geometry = (layout
            && world.has_any_by_id(TypeId::of::<crate::ui::dirty::Dirty>()))
            || (visual && world.has_any_by_id(TypeId::of::<crate::ui::dirty::VisualDirty>()));
        if !pending_geometry {
            let viewport = hit_test::geometry_viewport(world);
            for pointer in &mut state.pointers {
                let cursor = &pointer.cursor;
                pointer.target = viewport
                    .filter(|_| cursor.present && cursor.hover_capable)
                    .and_then(|(root, width, height)| {
                        hit_test::hit_test(world, root, cursor.x, cursor.y, width, height)
                    })
                    .map(|target| (target, InputLease::capture(world, target)));
            }
        }
        let active = state
            .pointers
            .iter()
            .enumerate()
            .fold(0_u32, |mask, (index, pointer)| {
                mask | if pointer.target(world).is_some() {
                    1 << index
                } else {
                    0
                }
            });
        for index in 0..state.entries.len() {
            let entry = &state.entries[index];
            let old = entry
                .published
                .filter(|old| old.lease.is_none_or(|lease| lease.valid(world)));
            let pending = if pending_geometry {
                old.filter(|old| {
                    old.pointers & active != 0
                        && InputBranch::scan(world, entry.entity)
                            .is_ok_and(|branch| branch.allowed())
                        && crate::core::reactive::retained::enabled(world, entry.entity)
                        && !world.has::<IgnoreHitTest>(entry.entity)
                })
                .map(|mut old| {
                    old.pointers &= active;
                    let pointer = &state.pointers[old.pointers.trailing_zeros() as usize].cursor;
                    old.pointer = (pointer.kind, pointer.id);
                    old.x = pointer.x;
                    old.y = pointer.y;
                    old
                })
            } else {
                state.membership(world, entry.entity)
            };
            state.entries[index].published = old;
            state.entries[index].pending = pending;
        }
        state.entries.sort_unstable_by_key(|entry| {
            core::cmp::Reverse(entry.published.map_or(0, |old| old.depth))
        });
        let count = state.entries.len();
        drop(state);
        for index in 0..count {
            let mut state = self.0.borrow_mut();
            if state.entries[index].pending.is_some() {
                continue;
            }
            let Some(old) = state.entries[index].published.take() else {
                continue;
            };
            let (x, y) = state
                .pointers
                .iter()
                .map(|pointer| &pointer.cursor)
                .find(|pointer| (pointer.kind, pointer.id) == old.pointer)
                .map_or((old.x, old.y), |pointer| (pointer.x, pointer.y));
            let entity = state.entries[index].entity;
            drop(state);
            notify(
                world,
                entity,
                HoverChanged {
                    hovered: false,
                    x,
                    y,
                },
            );
        }
        self.0
            .borrow_mut()
            .entries
            .sort_unstable_by_key(|entry| entry.pending.map_or(usize::MAX, |next| next.depth));
        for index in 0..count {
            let mut state = self.0.borrow_mut();
            let entry = &mut state.entries[index];
            let Some(next) = entry.pending.take() else {
                continue;
            };
            let entered = entry.published.is_none();
            entry.published = Some(next);
            let entity = entry.entity;
            drop(state);
            if entered {
                notify(
                    world,
                    entity,
                    HoverChanged {
                        hovered: true,
                        x: next.x,
                        y: next.y,
                    },
                );
            }
        }
        Ok(())
    }
}

fn notify(world: &mut World, entity: Entity, event: HoverChanged) {
    if let Err(error) = super::emission::submit_if_listened(world, entity, event) {
        crate::warn!(target: "mirui::event::hover", "hover notification skipped: {:?}", error);
    }
}

#[cfg(test)]
mod tests;
