use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::any::TypeId;

use crate::ecs::Entity;

#[derive(Clone, Copy)]
struct Instance {
    entity: Entity,
    kind: TypeId,
    generation: u64,
}

impl Instance {
    fn key(&self) -> (u32, u32, TypeId) {
        (self.entity.id, self.entity.generation, self.kind)
    }
}

#[derive(Default)]
pub(super) struct ComponentLifetimes {
    kinds: Vec<TypeId>,
    instances: Vec<Instance>,
    pending: VecDeque<Entity>,
    next: u64,
}

impl ComponentLifetimes {
    pub(super) fn watch(&mut self, kind: TypeId) -> bool {
        if self.kinds.contains(&kind) {
            return false;
        }
        self.kinds.push(kind);
        true
    }

    pub(super) fn replace(&mut self, entity: Entity, kind: TypeId) {
        if !self.kinds.contains(&kind) {
            return;
        }
        let Some(generation) = self.next.checked_add(1) else {
            self.remove(entity, kind);
            crate::warn!("component attachment identity exhausted");
            return;
        };
        self.next = generation;
        let instance = Instance {
            entity,
            kind,
            generation,
        };
        match self.locate(entity, kind) {
            Ok(index) => self.instances[index] = instance,
            Err(index) => self.instances.insert(index, instance),
        }
        self.enqueue(entity);
    }

    pub(super) fn enqueue(&mut self, entity: Entity) {
        if !self.pending.contains(&entity) {
            self.pending.push_back(entity);
        }
    }

    pub(super) fn remove(&mut self, entity: Entity, kind: TypeId) {
        if let Ok(index) = self.locate(entity, kind) {
            self.instances.remove(index);
            self.enqueue(entity);
        }
    }

    pub(super) fn despawn(&mut self, entity: Entity) {
        self.instances.retain(|instance| instance.entity != entity);
        self.enqueue(entity);
    }

    pub(super) fn generation(&self, entity: Entity, kind: TypeId) -> Option<u64> {
        self.locate(entity, kind)
            .ok()
            .map(|index| self.instances[index].generation)
    }

    fn locate(&self, entity: Entity, kind: TypeId) -> Result<usize, usize> {
        self.instances
            .binary_search_by_key(&(entity.id, entity.generation, kind), Instance::key)
    }

    pub(super) fn next_pending(&mut self) -> Option<Entity> {
        self.pending.pop_front()
    }

    pub(super) fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_lifetimes_preserve_identity_through_replacement_and_retirement() {
        let mut lifetimes = ComponentLifetimes::default();
        let kinds = [TypeId::of::<u32>(), TypeId::of::<u64>()];
        for kind in kinds {
            assert!(lifetimes.watch(kind));
        }
        for id in (0..128).rev() {
            for kind in kinds {
                lifetimes.replace(Entity { id, generation: 0 }, kind);
            }
        }
        let original = lifetimes.instances.clone();
        while lifetimes.next_pending().is_some() {}
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for instance in &original {
                    assert_eq!(
                        lifetimes.generation(instance.entity, instance.kind),
                        Some(instance.generation)
                    );
                }
                let old = Entity {
                    id: 63,
                    generation: 0,
                };
                let generation = lifetimes.generation(old, kinds[0]).unwrap();
                lifetimes.replace(old, kinds[0]);
                assert_ne!(lifetimes.generation(old, kinds[0]), Some(generation));
                lifetimes.remove(old, kinds[0]);
                assert_eq!(lifetimes.generation(old, kinds[0]), None);
                assert!(lifetimes.generation(old, kinds[1]).is_some());
                lifetimes.despawn(old);
                let new = Entity {
                    generation: 1,
                    ..old
                };
                lifetimes.replace(new, kinds[0]);
                assert!(lifetimes.generation(new, kinds[0]).is_some());
                assert_eq!(lifetimes.generation(old, kinds[0]), None);
                assert_eq!(lifetimes.generation(old, kinds[1]), None);
            }),
            0
        );
        assert!(
            lifetimes
                .instances
                .windows(2)
                .all(|pair| pair[0].key() < pair[1].key())
        );
        assert_eq!(lifetimes.instances.len(), 255);
    }
}
