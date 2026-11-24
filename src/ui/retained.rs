use crate::core::reactive::{BindingRole, schedule::wake_entity_effects};
use crate::ecs::{Entity, World};

use super::Parent;

/// Internal gate for retained subtrees whose data binding can be inactive.
pub(crate) struct RetainedEffects {
    pub active: bool,
    pub valid: fn(&World, Entity) -> bool,
    incarnation: u64,
}
impl crate::ecs::Component for RetainedEffects {}

#[derive(Default)]
struct RetainedClock(u64);

fn advance(world: &mut World) -> u64 {
    if world.resource::<RetainedClock>().is_none() {
        world.insert_resource(RetainedClock::default());
    }
    let clock = world.resource_mut::<RetainedClock>().unwrap();
    clock.0 = clock.0.saturating_add(1);
    clock.0
}

impl RetainedEffects {
    pub(crate) fn new(world: &mut World, valid: fn(&World, Entity) -> bool) -> Self {
        Self {
            active: true,
            valid,
            incarnation: advance(world),
        }
    }
}

pub(crate) fn retire(world: &mut World, root: Entity) {
    if world.has::<RetainedEffects>(root) {
        let incarnation = advance(world);
        world.get_mut::<RetainedEffects>(root).unwrap().incarnation = incarnation;
    }
}

#[derive(Clone, Copy)]
pub(crate) struct InputLease {
    target: Entity,
    issued: u64,
}

impl InputLease {
    pub(crate) fn target(self) -> Entity {
        self.target
    }

    pub(crate) fn capture(world: &World, target: Entity) -> Option<Self> {
        let mut entity = target;
        for _ in 0..=world.live_entity_count() {
            if world.has::<RetainedEffects>(entity) {
                return Some(Self {
                    target,
                    issued: world.resource::<RetainedClock>().map_or(0, |clock| clock.0),
                });
            }
            entity = world.get::<Parent>(entity)?.0;
        }
        None
    }

    pub(crate) fn valid(self, world: &World) -> bool {
        world.is_alive(self.target) && enabled_at(world, self.target, self.issued)
    }
}

pub(crate) fn wake(world: &World, root: Entity) {
    wake_matching(world, root, |_| true);
}

pub(crate) fn wake_display(world: &World, root: Entity) {
    wake_matching(world, root, |role| role == BindingRole::Display);
}

fn wake_matching(world: &World, root: Entity, accepts: impl Fn(BindingRole) -> bool) {
    let contains = super::branch::subtree_membership(world, root);
    wake_entity_effects(world, contains, accepts);
}

pub(super) fn display_enabled(world: &World, mut entity: Entity) -> bool {
    let branches = world.storage::<super::branch::CachedBranchVisibility>();
    let parents = world.storage::<Parent>();
    for _ in 0..=world.live_entity_count() {
        if branches
            .and_then(|branches| branches.get(entity))
            .is_some_and(|branch| !branch.selected)
        {
            return false;
        }
        let Some(parent) = parents.and_then(|parents| parents.get(entity)) else {
            return true;
        };
        entity = parent.0;
    }
    false
}

pub(crate) fn enabled(world: &World, entity: Entity) -> bool {
    enabled_at(world, entity, u64::MAX)
}

pub(crate) fn invalid_root(world: &World, mut entity: Entity) -> Option<Entity> {
    for _ in 0..=world.live_entity_count() {
        if let Some(gate) = world.get::<RetainedEffects>(entity)
            && (!gate.active || gate.incarnation == u64::MAX || !(gate.valid)(world, entity))
        {
            return Some(entity);
        }
        entity = world.get::<Parent>(entity)?.0;
    }
    None
}

fn enabled_at(world: &World, mut entity: Entity, issued: u64) -> bool {
    let gates = world.storage::<RetainedEffects>();
    let parents = world.storage::<Parent>();
    for _ in 0..=world.live_entity_count() {
        if let Some(gate) = gates.and_then(|gates| gates.get(entity))
            && (!gate.active
                || gate.incarnation == u64::MAX
                || gate.incarnation > issued
                || !(gate.valid)(world, entity))
        {
            return false;
        }
        let Some(parent) = parents.and_then(|parents| parents.get(entity)) else {
            return true;
        };
        entity = parent.0;
    }
    false
}

pub(super) fn ancestry_depth(world: &World) -> impl Fn(Entity) -> usize + '_ {
    let parents = world.storage::<Parent>();
    let limit = world.live_entity_count();
    move |mut entity| {
        for depth in 0..=limit {
            let Some(parent) = parents.and_then(|parents| parents.get(entity)) else {
                return depth;
            };
            entity = parent.0;
        }
        usize::MAX
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ui/retained/tests.rs"]
mod tests;
