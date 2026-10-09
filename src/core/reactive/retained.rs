use super::*;

/// Internal gate for retained subtrees whose data binding can be inactive.
pub(crate) struct RetainedEffects {
    pub active: bool,
    pub valid: fn(&World, Entity) -> bool,
}
impl crate::ecs::Component for RetainedEffects {}

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

pub(crate) fn enabled(world: &World, mut entity: Entity) -> bool {
    for _ in 0..=world.live_entity_count() {
        if let Some(gate) = world.get::<RetainedEffects>(entity)
            && (!gate.active || !(gate.valid)(world, entity))
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
