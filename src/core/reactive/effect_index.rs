use alloc::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::ecs::Entity;
use crate::ecs::world::WorldId;

use super::EffectId;
use super::identity::SlotId;

type EntityKey = (u64, u32, u32, EffectId);

#[derive(Default)]
struct WorldEffects {
    ids: VecDeque<EffectId>,
    releasing: bool,
}

#[derive(Debug)]
pub(super) enum IndexError {
    CapacityOverflow,
    Allocation,
}

/// Ordered identities share the registry's destruction order without borrowing callbacks.
pub(super) struct EffectIndex {
    worlds: BTreeMap<u64, WorldEffects>,
    entities: BTreeSet<EntityKey>,
}

fn entity_key(world: WorldId, entity: Entity, id: EffectId) -> EntityKey {
    (world.diagnostic_id(), entity.id, entity.generation, id)
}

impl EffectIndex {
    pub const fn new() -> Self {
        Self {
            worlds: BTreeMap::new(),
            entities: BTreeSet::new(),
        }
    }

    pub fn prepare_world(unbound: usize) -> Result<VecDeque<EffectId>, IndexError> {
        let mut ids = VecDeque::new();
        ids.try_reserve(unbound)
            .map_err(|_| IndexError::Allocation)?;
        Ok(ids)
    }

    pub fn register_world(&mut self, world: WorldId, ids: VecDeque<EffectId>) {
        if ids.capacity() == 0 {
            return;
        }
        assert!(
            !self.worlds.contains_key(&world.diagnostic_id()),
            "Effect owner already indexed"
        );
        self.worlds.insert(
            world.diagnostic_id(),
            WorldEffects {
                ids,
                releasing: false,
            },
        );
    }

    pub fn reserve_unbound(
        &mut self,
        worlds: impl Iterator<Item = WorldId>,
        unbound: usize,
    ) -> Result<(), IndexError> {
        for world in worlds {
            let ids = &mut self.worlds.entry(world.diagnostic_id()).or_default().ids;
            ids.len()
                .checked_add(unbound)
                .ok_or(IndexError::CapacityOverflow)?;
            ids.try_reserve(unbound)
                .map_err(|_| IndexError::Allocation)?;
        }
        Ok(())
    }

    pub fn prepare_owned(&mut self, world: WorldId, unbound: usize) -> Result<(), IndexError> {
        let ids = &mut self.worlds.entry(world.diagnostic_id()).or_default().ids;
        let additional = unbound.checked_add(1).ok_or(IndexError::CapacityOverflow)?;
        ids.len()
            .checked_add(additional)
            .ok_or(IndexError::CapacityOverflow)?;
        ids.try_reserve(additional)
            .map_err(|_| IndexError::Allocation)?;
        Ok(())
    }

    pub fn bind_world(&mut self, world: WorldId, id: EffectId) {
        let ids = &mut self
            .worlds
            .get_mut(&world.diagnostic_id())
            .expect("Effect owner is registered")
            .ids;
        let at = ids.binary_search(&id).expect_err("Effect already indexed");
        assert!(
            ids.len() < ids.capacity(),
            "Effect index requires structural reservation"
        );
        ids.insert(at, id);
    }

    pub fn insert_owned(&mut self, world: WorldId, entity: Option<Entity>, id: EffectId) {
        self.bind_world(world, id);
        if let Some(entity) = entity
            && !self.worlds[&world.diagnostic_id()].releasing
        {
            assert!(
                self.entities.insert(entity_key(world, entity, id)),
                "Effect entity already indexed"
            );
        }
    }

    pub fn first_world(&self, world: WorldId) -> Option<EffectId> {
        self.worlds
            .get(&world.diagnostic_id())?
            .ids
            .front()
            .copied()
    }

    pub fn first_entity(
        &self,
        world: WorldId,
        entity: Entity,
        mut matches: impl FnMut(EffectId) -> bool,
    ) -> Option<EffectId> {
        if let Some(owner) = self.worlds.get(&world.diagnostic_id())
            && owner.releasing
        {
            return owner.ids.iter().copied().find(|id| matches(*id));
        }
        let first = entity_key(
            world,
            entity,
            EffectId(SlotId {
                slot: 0,
                generation: 0,
            }),
        );
        let last = entity_key(
            world,
            entity,
            EffectId(SlotId {
                slot: u32::MAX,
                generation: u32::MAX,
            }),
        );
        self.entities.range(first..=last).next().map(|key| key.3)
    }

    pub fn remove(&mut self, world: WorldId, entity: Option<Entity>, id: EffectId) -> bool {
        let Some(position) = self
            .worlds
            .get(&world.diagnostic_id())
            .and_then(|owner| owner.ids.binary_search(&id).ok())
        else {
            return false;
        };
        let entity_key = entity.map(|entity| entity_key(world, entity, id));
        let releasing = self.worlds[&world.diagnostic_id()].releasing;
        if !releasing && entity_key.is_some_and(|key| !self.entities.contains(&key)) {
            return false;
        }
        self.worlds
            .get_mut(&world.diagnostic_id())
            .expect("Effect owner validated")
            .ids
            .remove(position);
        if let Some(key) = entity_key
            && !releasing
        {
            self.entities.remove(&key);
            if self.entities.is_empty() {
                self.entities = BTreeSet::new();
            }
        }
        true
    }

    pub fn begin_release(&mut self, world: WorldId) {
        let Some(owner) = self.worlds.get_mut(&world.diagnostic_id()) else {
            return;
        };
        owner.releasing = true;
        let world = world.diagnostic_id();
        if self.entities.first().is_some_and(|key| key.0 == world)
            && self.entities.last().is_some_and(|key| key.0 == world)
        {
            self.entities = BTreeSet::new();
        } else {
            self.entities.retain(|key| key.0 != world);
        }
    }

    pub fn remove_world(&mut self, world: WorldId) {
        if let Some(owner) = self.worlds.remove(&world.diagnostic_id()) {
            assert!(owner.ids.is_empty(), "Effect owner still has live members");
        }
        debug_assert!(
            !self
                .entities
                .iter()
                .any(|key| key.0 == world.diagnostic_id())
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reactive::identity::SlotId;
    use crate::ecs::World;

    fn id(slot: u32, generation: u32) -> EffectId {
        EffectId(SlotId { slot, generation })
    }

    #[test]
    fn entity_and_world_members_keep_full_identity_and_registry_order() {
        let mut index = EffectIndex::new();
        let a = World::new();
        let b = World::new();
        let entity = Entity {
            id: 1,
            generation: 2,
        };
        let replacement = Entity {
            generation: 3,
            ..entity
        };
        for world in [a.id(), b.id()] {
            index.register_world(world, EffectIndex::prepare_world(0).unwrap());
        }
        for (world, entity, effect) in [
            (a.id(), entity, id(9, 0)),
            (a.id(), replacement, id(2, 1)),
            (b.id(), entity, id(0, 1)),
            (a.id(), entity, id(1, 4)),
        ] {
            index.prepare_owned(world, 0).unwrap();
            index.insert_owned(world, Some(entity), effect);
        }
        assert_eq!(index.first_world(a.id()), Some(id(1, 4)));
        assert_eq!(
            index.first_entity(a.id(), entity, |_| false),
            Some(id(1, 4))
        );
        assert!(!index.remove(a.id(), Some(entity), id(1, 3)));
        assert!(!index.remove(a.id(), Some(replacement), id(1, 4)));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert!(index.remove(a.id(), Some(entity), id(1, 4)));
                assert_eq!(
                    index.first_entity(a.id(), entity, |_| false),
                    Some(id(9, 0))
                );
                assert_eq!(index.first_world(a.id()), Some(id(2, 1)));
                assert!(index.remove(a.id(), Some(entity), id(9, 0)));
            }),
            0
        );
        assert_eq!(index.first_entity(a.id(), entity, |_| false), None);
        assert_eq!(
            index.first_entity(a.id(), replacement, |_| false),
            Some(id(2, 1))
        );
        assert_eq!(
            index.first_entity(b.id(), entity, |_| false),
            Some(id(0, 1))
        );
        assert_eq!(index.entities.len(), 2);
    }

    #[test]
    fn all_worlds_retain_unbound_headroom_after_owned_spawns() {
        let mut index = EffectIndex::new();
        let a = World::new();
        let b = World::new();
        for world in [a.id(), b.id()] {
            index.register_world(world, EffectIndex::prepare_world(2).unwrap());
        }
        for slot in 0..8 {
            index.prepare_owned(a.id(), 2).unwrap();
            index.insert_owned(a.id(), None, id(slot, 0));
        }
        index
            .reserve_unbound([a.id(), b.id()].into_iter(), 3)
            .unwrap();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for slot in 8..11 {
                    index.bind_world(a.id(), id(slot, 0));
                }
                for slot in 11..14 {
                    index.bind_world(b.id(), id(slot, 0));
                }
            }),
            0
        );
        for slot in 8..11 {
            assert!(index.remove(a.id(), None, id(slot, 0)));
        }
        for slot in 11..14 {
            assert!(index.remove(b.id(), None, id(slot, 0)));
        }
        index.remove_world(b.id());
        assert!(!index.worlds.contains_key(&b.id().diagnostic_id()));
    }

    #[test]
    fn static_worlds_allocate_index_storage_only_when_effects_are_admitted() {
        let mut index = EffectIndex::new();
        let world = World::new();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                index.register_world(world.id(), EffectIndex::prepare_world(0).unwrap());
            }),
            0
        );
        assert!(index.worlds.is_empty());
        index
            .reserve_unbound(core::iter::once(world.id()), 1)
            .unwrap();
        assert_eq!(
            crate::test_support::tracked_allocations(|| index.bind_world(world.id(), id(0, 1))),
            0
        );
        assert!(index.remove(world.id(), None, id(0, 1)));
        index.remove_world(world.id());
        assert!(index.worlds.is_empty());
    }
}
