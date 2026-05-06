use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::{Any, TypeId};

use super::entity::{Entity, EntityAllocator};
use super::sparse_set::SparseSet;

trait ComponentStorage: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove_entity(&mut self, entity: Entity);
}

impl<T: 'static> ComponentStorage for SparseSet<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn remove_entity(&mut self, entity: Entity) {
        self.remove(entity);
    }
}

pub struct World {
    allocator: EntityAllocator,
    storages: Vec<(TypeId, Box<dyn ComponentStorage>)>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: Vec::new(),
        }
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn(&mut self) -> Entity {
        self.allocator.allocate()
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.allocator.deallocate(entity) {
            return false;
        }
        for (_, storage) in &mut self.storages {
            storage.remove_entity(entity);
        }
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.allocator.is_alive(entity)
    }

    pub fn insert<T: 'static>(&mut self, entity: Entity, component: T) {
        if !self.is_alive(entity) {
            return;
        }
        let storage = self.storage_mut::<T>();
        storage.insert(entity, component);
    }

    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Option<T> {
        let storage = self.storage_mut::<T>();
        storage.remove(entity)
    }

    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.storage::<T>()?.get(entity)
    }

    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        self.storage_mut::<T>().get_mut(entity)
    }

    fn storage<T: 'static>(&self) -> Option<&SparseSet<T>> {
        let type_id = TypeId::of::<T>();
        self.storages
            .iter()
            .find(|(id, _)| *id == type_id)
            .map(|(_, s)| s.as_any().downcast_ref::<SparseSet<T>>().unwrap())
    }

    fn storage_mut<T: 'static>(&mut self) -> &mut SparseSet<T> {
        let type_id = TypeId::of::<T>();
        let idx = self.storages.iter().position(|(id, _)| *id == type_id);
        let idx = match idx {
            Some(i) => i,
            None => {
                self.storages
                    .push((type_id, Box::new(SparseSet::<T>::new())));
                self.storages.len() - 1
            }
        };
        self.storages[idx]
            .1
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            .unwrap()
    }
}
