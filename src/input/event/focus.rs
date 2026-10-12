use super::contract::{EntityNotification, EntitySource, Event, EventKey};
use super::listeners::InputBranch;
use crate::core::reactive::retained::InputLease;
use crate::ecs::Entity;
use crate::ecs::World;
use core::cell::Cell;

/// Keyboard focus entered or left this node. `related` is the other node in the transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusChanged {
    pub focused: bool,
    pub related: Option<Entity>,
}

impl Event for FocusChanged {
    type Route = EntityNotification;
    type Source = EntitySource;
    type Options = ();
}

/// Focus notifications available on component nodes.
#[allow(non_upper_case_globals)]
pub trait FocusSelectors: crate::ecs::Component {
    const FocusChanged: EventKey<FocusChanged> = FocusChanged::EVENT;
}
impl<W: crate::ecs::Component> FocusSelectors for W {}

#[derive(Clone, Copy)]
enum FocusIdentity {
    Entity(Entity),
    Retained(InputLease),
}

impl FocusIdentity {
    fn entity(self) -> Entity {
        match self {
            Self::Entity(entity) => entity,
            Self::Retained(lease) => lease.target(),
        }
    }

    fn live(self, world: &World) -> bool {
        match self {
            Self::Entity(entity) => world.is_alive(entity),
            Self::Retained(lease) => lease.valid(world),
        }
    }

    fn capture(world: &World, entity: Entity) -> Self {
        InputLease::capture(world, entity).map_or(Self::Entity(entity), Self::Retained)
    }
}

#[derive(Default)]
pub(super) struct FocusObserver {
    active: Cell<bool>,
    published: Cell<Option<FocusIdentity>>,
}

impl FocusObserver {
    pub(super) fn enable(&self, world: &World) {
        if !self.active.replace(true) {
            self.published.set(
                world
                    .resource::<FocusState>()
                    .and_then(|focus| focus.focused)
                    .map(|entity| FocusIdentity::capture(world, entity)),
            );
        }
    }

    pub(super) fn observe(&self, world: &mut World) {
        let old = self.published.get();
        let requested = world
            .resource::<FocusState>()
            .and_then(|focus| focus.focused);
        let current = requested.filter(|&entity| {
            old.is_none_or(|old| old.entity() != entity || old.live(world))
                && crate::core::reactive::retained::enabled(world, entity)
                && InputBranch::scan(world, entity).is_ok_and(|branch| branch.allowed())
        });
        if current != requested
            && let Some(focus) = world.resource_mut::<FocusState>()
        {
            focus.focused = current;
        }
        if !self.active.get() {
            return;
        }
        if old.map(FocusIdentity::entity) == current {
            return;
        }
        self.published
            .set(current.map(|entity| FocusIdentity::capture(world, entity)));
        if let Some(old) = old.filter(|old| old.live(world)) {
            Self::notify(
                world,
                old.entity(),
                FocusChanged {
                    focused: false,
                    related: current,
                },
            );
        }
        if let Some(entity) = current {
            Self::notify(
                world,
                entity,
                FocusChanged {
                    focused: true,
                    related: old.map(FocusIdentity::entity),
                },
            );
        }
    }

    fn notify(world: &mut World, entity: Entity, event: FocusChanged) {
        if let Err(error) = super::emission::submit_if_listened(world, entity, event) {
            crate::warn!(target: "mirui::event::focus", "focus notification skipped: {:?}", error);
        }
    }
}

/// Marker component: this entity can receive keyboard/char input.
pub struct Focusable;
impl crate::ecs::Component for Focusable {}

/// Controls whether pointer activation changes keyboard focus. The closest explicit policy on the hit node or an ancestor applies.
#[crate::component]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerFocus {
    /// Focus the closest focusable ancestor, or clear focus when none exists.
    #[default]
    Automatic,
    /// Keep the current keyboard focus while allowing pointer input normally.
    Preserve,
}

/// World resource tracking which entity currently has keyboard focus.
#[derive(Default)]
pub struct FocusState {
    pub focused: Option<Entity>,
}

#[cfg(test)]
mod tests;
