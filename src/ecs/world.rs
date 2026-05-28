use alloc::boxed::Box;
use core::any::{Any, TypeId};
use hashbrown::HashMap;

use super::entity::{Entity, EntityAllocator};
use super::sparse_set::SparseSet;

trait ComponentStorage: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove_entity(&mut self, entity: Entity);
    fn contains_entity(&self, entity: Entity) -> bool;
    fn is_empty(&self) -> bool;
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
    fn contains_entity(&self, entity: Entity) -> bool {
        self.contains(entity)
    }
    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

pub struct World {
    allocator: EntityAllocator,
    storages: HashMap<TypeId, Box<dyn ComponentStorage>>,
    resources: HashMap<TypeId, Box<dyn Any>>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: HashMap::new(),
            resources: HashMap::new(),
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
        for storage in self.storages.values_mut() {
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

    pub fn has<T: 'static>(&self, entity: Entity) -> bool {
        self.storage::<T>().is_some_and(|s| s.contains(entity))
    }

    pub fn has_type(&self, entity: Entity, type_id: TypeId) -> bool {
        self.storages
            .get(&type_id)
            .is_some_and(|s| s.contains_entity(entity))
    }

    /// True iff *any* live entity owns a component of `type_id`.
    /// Pairs with [`Self::has_type`] for per-entity checks.
    pub fn has_any_by_id(&self, type_id: TypeId) -> bool {
        self.storages.get(&type_id).is_some_and(|s| !s.is_empty())
    }

    pub fn query<T: 'static>(&self) -> super::query::QueryBuilder<'_, T> {
        super::query::QueryBuilder::new(self)
    }

    pub(crate) fn storage<T: 'static>(&self) -> Option<&SparseSet<T>> {
        self.storages
            .get(&TypeId::of::<T>())
            .map(|s| s.as_any().downcast_ref::<SparseSet<T>>().unwrap())
    }

    fn storage_mut<T: 'static>(&mut self) -> &mut SparseSet<T> {
        self.storages
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Box::new(SparseSet::<T>::new()))
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            .unwrap()
    }

    pub fn insert_resource<T: 'static>(&mut self, value: T) {
        self.resources.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn resource<T: 'static>(&self) -> Option<&T> {
        self.resources
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }

    pub fn resource_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.resources
            .get_mut(&TypeId::of::<T>())
            .and_then(|v| v.downcast_mut::<T>())
    }
}
