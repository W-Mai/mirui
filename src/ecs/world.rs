use alloc::boxed::Box;
use core::any::{Any, TypeId};
use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

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
    storages: HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    resources: HashMap<TypeId, Box<dyn Any>, FxBuildHasher>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            allocator: EntityAllocator::new(),
            storages: HashMap::default(),
            resources: HashMap::default(),
        }
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spawn_empty(&mut self) -> Entity {
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

    /// Visits every entity that currently owns `T` without allocating.
    ///
    /// The callback may mutate component values and other component types, but
    /// it must not insert or remove `T`. Debug builds verify that invariant.
    pub fn for_each_stable<T: 'static>(&mut self, mut visit: impl FnMut(&mut Self, Entity)) {
        let count = self.storage::<T>().map_or(0, SparseSet::len);
        for index in 0..count {
            let entity = self
                .storage::<T>()
                .and_then(|storage| storage.entities().get(index))
                .copied()
                .expect("stable component storage changed during iteration");
            visit(self, entity);
            debug_assert_eq!(
                self.storage::<T>().map_or(0, SparseSet::len),
                count,
                "for_each_stable callback inserted or removed the iterated component"
            );
            debug_assert_eq!(
                self.storage::<T>()
                    .and_then(|storage| storage.entities().get(index))
                    .copied(),
                Some(entity),
                "for_each_stable callback reordered the iterated component storage"
            );
        }
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

    pub fn remove_resource<T: 'static>(&mut self) -> Option<T> {
        self.take_resource_box::<T>().map(|value| *value)
    }

    pub(crate) fn take_resource_box<T: 'static>(&mut self) -> Option<Box<T>> {
        self.resources
            .remove(&TypeId::of::<T>())
            .and_then(|v| v.downcast::<T>().ok())
    }

    pub(crate) fn put_resource_box<T: 'static>(&mut self, value: Box<T>) {
        self.resources.insert(TypeId::of::<T>(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_iteration_mutates_values_and_other_component_types() {
        let mut world = World::new();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(first, 1u16);
        world.insert(second, 2u16);

        world.for_each_stable::<u16>(|world, entity| {
            *world.get_mut::<u16>(entity).unwrap() += 10;
            world.insert(entity, true);
        });

        assert_eq!(world.get::<u16>(first), Some(&11));
        assert_eq!(world.get::<u16>(second), Some(&12));
        assert_eq!(world.get::<bool>(first), Some(&true));
        assert_eq!(world.get::<bool>(second), Some(&true));
    }

    #[test]
    #[should_panic(expected = "inserted or removed the iterated component")]
    fn stable_iteration_rejects_structural_changes_to_the_iterated_type() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 1u16);

        world.for_each_stable::<u16>(|world, entity| {
            world.remove::<u16>(entity);
        });
    }
}
