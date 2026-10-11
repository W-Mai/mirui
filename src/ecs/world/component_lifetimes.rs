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
        self.remove(entity, kind);
        let Some(generation) = self.next.checked_add(1) else {
            crate::warn!("component attachment identity exhausted");
            return;
        };
        self.next = generation;
        self.instances.push(Instance {
            entity,
            kind,
            generation,
        });
        self.enqueue(entity);
    }

    pub(super) fn enqueue(&mut self, entity: Entity) {
        if !self.pending.contains(&entity) {
            self.pending.push_back(entity);
        }
    }

    pub(super) fn remove(&mut self, entity: Entity, kind: TypeId) {
        if let Some(index) = self
            .instances
            .iter()
            .position(|instance| instance.entity == entity && instance.kind == kind)
        {
            self.instances.swap_remove(index);
            self.enqueue(entity);
        }
    }

    pub(super) fn despawn(&mut self, entity: Entity) {
        self.instances.retain(|instance| instance.entity != entity);
        self.pending.retain(|pending| *pending != entity);
    }

    pub(super) fn generation(&self, entity: Entity, kind: TypeId) -> Option<u64> {
        self.instances
            .iter()
            .find(|instance| instance.entity == entity && instance.kind == kind)
            .map(|instance| instance.generation)
    }

    pub(super) fn next_pending(&mut self) -> Option<Entity> {
        self.pending.pop_front()
    }

    pub(super) fn pending_len(&self) -> usize {
        self.pending.len()
    }
}
