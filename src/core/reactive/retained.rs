use super::*;

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
    pub(crate) fn capture(world: &World, target: Entity) -> Option<Self> {
        let mut entity = target;
        for _ in 0..=world.live_entity_count() {
            if world.has::<RetainedEffects>(entity) {
                return Some(Self {
                    target,
                    issued: world.resource::<RetainedClock>().map_or(0, |clock| clock.0),
                });
            }
            entity = world.get::<crate::ui::Parent>(entity)?.0;
        }
        None
    }

    pub(crate) fn valid(self, world: &World) -> bool {
        world.is_alive(self.target) && enabled_at(world, self.target, self.issued)
    }
}

pub(crate) fn wake(world: &World, root: Entity) {
    with_reactive(|runtime| {
        for (&id, effect) in &runtime.effects {
            let effect = effect.borrow();
            if effect.owner_world == Some(world.id())
                && effect
                    .owner_entity
                    .is_some_and(|entity| crate::ui::branch::is_in_subtree(world, root, entity))
                && !runtime.dirty_effects.contains(&id)
            {
                runtime.dirty_effects.push_back(id);
            }
        }
    });
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
        entity = world.get::<crate::ui::Parent>(entity)?.0;
    }
    None
}

fn enabled_at(world: &World, mut entity: Entity, issued: u64) -> bool {
    for _ in 0..=world.live_entity_count() {
        if let Some(gate) = world.get::<RetainedEffects>(entity)
            && (!gate.active
                || gate.incarnation == u64::MAX
                || gate.incarnation > issued
                || !(gate.valid)(world, entity))
        {
            return false;
        }
        let Some(parent) = world.get::<crate::ui::Parent>(entity) else {
            return true;
        };
        entity = parent.0;
    }
    false
}

pub(super) fn depth(world: &World, mut entity: Entity) -> usize {
    for depth in 0..=world.live_entity_count() {
        let Some(parent) = world.get::<crate::ui::Parent>(entity) else {
            return depth;
        };
        entity = parent.0;
    }
    usize::MAX
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_lease_checks_every_retained_ancestor_without_allocating() {
        let mut world = World::new();
        let outer = world.spawn_empty();
        let inner = world.spawn_empty();
        let child = world.spawn_empty();
        let unrelated = world.spawn_empty();
        world.insert(inner, crate::ui::Parent(outer));
        world.insert(child, crate::ui::Parent(inner));
        for entity in [outer, inner, unrelated] {
            let gate = RetainedEffects::new(&mut world, |_, _| true);
            world.insert(entity, gate);
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let lease = InputLease::capture(&world, child).unwrap();
                retire(&mut world, unrelated);
                assert!(lease.valid(&world));
                retire(&mut world, outer);
                assert!(!lease.valid(&world));
                let lease = InputLease::capture(&world, child).unwrap();
                assert!(lease.valid(&world));
                retire(&mut world, inner);
                retire(&mut world, inner);
                assert!(!lease.valid(&world));
                assert!(InputLease::capture(&world, child).unwrap().valid(&world));
                world.get_mut::<RetainedEffects>(outer).unwrap().active = false;
                assert!(!enabled(&world, child));
            }),
            0
        );
    }
}
