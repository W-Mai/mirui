use alloc::boxed::Box;
use alloc::rc::{Rc, Weak};
use core::any::{Any, TypeId};
use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

use super::entity::{Entity, EntityAllocator};
use super::sparse_set::SparseSet;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct WorldId(u64);

impl WorldId {
    fn next() -> Self {
        static mut NEXT: u64 = 1;
        critical_section::with(|_| {
            // SAFETY: the counter is only accessed while holding the critical section.
            let value = unsafe { NEXT };
            let next = value.checked_add(1).expect("world identity exhausted");
            // SAFETY: the same critical section protects the write.
            unsafe { NEXT = next };
            Self(value)
        })
    }
}

trait ComponentStorage: Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn remove_entity(&mut self, entity: Entity);
    fn contains_entity(&self, entity: Entity) -> bool;
    fn is_empty(&self) -> bool;
    fn entities(&self) -> &[Entity];
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

    fn entities(&self) -> &[Entity] {
        self.entities()
    }
}

/// A component type on one entity changed since the last drain.
/// Repeated replacements are coalesced; consumers read the final state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ComponentChange {
    pub entity: Entity,
    pub type_id: TypeId,
}

pub struct World {
    id: WorldId,
    allocator: EntityAllocator,
    storages: HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    resources: HashMap<TypeId, Box<dyn Any>, FxBuildHasher>,
    watched_component_types: alloc::vec::Vec<TypeId>,
    component_changes: alloc::vec::Vec<ComponentChange>,
    drop_hooks: alloc::vec::Vec<(TypeId, fn(WorldId))>,
    lifetime: Option<Rc<()>>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            id: WorldId::next(),
            allocator: EntityAllocator::new(),
            storages: HashMap::default(),
            resources: HashMap::default(),
            watched_component_types: alloc::vec::Vec::new(),
            component_changes: alloc::vec::Vec::new(),
            drop_hooks: alloc::vec::Vec::new(),
            lifetime: Some(Rc::new(())),
        }
    }
}

impl World {
    pub(crate) fn id(&self) -> WorldId {
        self.id
    }

    pub(crate) fn lifetime(&self) -> Weak<()> {
        Rc::downgrade(self.lifetime.as_ref().expect("World has ended"))
    }

    pub(crate) fn register_drop_hook<T: 'static>(&mut self, hook: fn(WorldId)) {
        let kind = TypeId::of::<T>();
        if !self
            .drop_hooks
            .iter()
            .any(|(registered, _)| *registered == kind)
        {
            self.drop_hooks.push((kind, hook));
        }
    }

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
        for index in 0..self.watched_component_types.len() {
            let type_id = self.watched_component_types[index];
            if self.has_type(entity, type_id) {
                self.record_component_change(entity, type_id);
            }
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
        self.record_component_change(entity, TypeId::of::<T>());
    }

    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Option<T> {
        let removed = self.storage_mut_if_exists::<T>()?.remove(entity);
        if removed.is_some() {
            self.record_component_change(entity, TypeId::of::<T>());
        }
        removed
    }

    /// Begin recording structural changes for a component type. Existing
    /// components are queued so a newly registered View can attach them.
    pub fn watch_component_type(&mut self, type_id: TypeId) {
        if !self.watched_component_types.contains(&type_id) {
            self.watched_component_types.push(type_id);
        }
        let count = self
            .storages
            .get(&type_id)
            .map_or(0, |storage| storage.entities().len());
        for index in 0..count {
            let entity = self
                .storages
                .get(&type_id)
                .expect("observed storage")
                .entities()[index];
            self.record_component_change(entity, type_id);
        }
    }

    /// Reserve the change queue before a batch of component replacements.
    pub fn reserve_component_changes(&mut self, additional: usize) {
        self.component_changes.reserve(additional);
    }

    fn record_component_change(&mut self, entity: Entity, type_id: TypeId) {
        if !self.watched_component_types.contains(&type_id) {
            return;
        }
        let change = ComponentChange { entity, type_id };
        if !self.component_changes.contains(&change) {
            if self.component_changes.len() == self.component_changes.capacity() {
                self.component_changes
                    .try_reserve(1)
                    .expect("component change queue capacity exhausted");
            }
            self.component_changes.push(change);
        }
    }

    /// Visit queued changes outside component storage borrows. New structural
    /// changes made by the callback are visited in this same pass.
    pub fn drain_component_changes(&mut self, mut visit: impl FnMut(&mut Self, ComponentChange)) {
        let mut index = 0;
        while let Some(change) = self.component_changes.get(index).copied() {
            index += 1;
            visit(self, change);
        }
        self.component_changes.clear();
    }

    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.storage::<T>()?.get(entity)
    }

    /// In-place writes do not produce structural component-change records.
    /// Replace the component with `insert` when its bindings change.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        self.storage_mut_if_exists::<T>()?.get_mut(entity)
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

    fn storage_mut_if_exists<T: 'static>(&mut self) -> Option<&mut SparseSet<T>> {
        self.storages
            .get_mut(&TypeId::of::<T>())
            .map(|storage| storage.as_any_mut().downcast_mut::<SparseSet<T>>().unwrap())
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

    /// Temporarily separate a resource from the World while its callback also
    /// needs World access. The original allocation is restored on unwind.
    pub(crate) fn with_resource_box<T: 'static, R>(
        &mut self,
        run: impl FnOnce(&mut T, &mut Self) -> R,
    ) -> Option<R> {
        struct Restore<'a, T: 'static> {
            world: &'a mut World,
            value: Option<Box<T>>,
        }
        impl<T: 'static> Drop for Restore<'_, T> {
            fn drop(&mut self) {
                if let Some(value) = self.value.take() {
                    self.world.put_resource_box(value);
                }
            }
        }
        let value = self.take_resource_box::<T>()?;
        let mut restore = Restore {
            world: self,
            value: Some(value),
        };
        Some(run(
            restore.value.as_deref_mut().expect("resource"),
            restore.world,
        ))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        self.lifetime.take();
        for (_, hook) in &self.drop_hooks {
            hook(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_expires_before_resource_destructors() {
        struct CheckLifetime {
            lifetime: Weak<()>,
            observed: Rc<core::cell::Cell<bool>>,
        }

        impl Drop for CheckLifetime {
            fn drop(&mut self) {
                self.observed.set(self.lifetime.upgrade().is_none());
            }
        }

        let mut world = World::new();
        let observed = Rc::new(core::cell::Cell::new(false));
        world.insert_resource(CheckLifetime {
            lifetime: world.lifetime(),
            observed: Rc::clone(&observed),
        });

        drop(world);
        assert!(observed.get());
    }

    #[test]
    fn absent_component_mutation_does_not_create_storage() {
        let mut world = World::new();
        let entity = world.spawn_empty();

        assert!(world.get_mut::<u16>(entity).is_none());
        assert!(world.remove::<u16>(entity).is_none());
        assert!(world.storages.is_empty());

        world.insert(entity, 7u16);
        assert_eq!(world.get_mut::<u16>(entity), Some(&mut 7));
        assert_eq!(world.remove::<u16>(entity), Some(7));
        assert!(world.get_mut::<u16>(entity).is_none());
        assert!(world.remove::<u16>(entity).is_none());
        assert_eq!(world.storages.len(), 1);
    }

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

    #[test]
    fn watched_component_changes_dedupe_replacements_and_removal() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 1u16);
        world.watch_component_type(TypeId::of::<u16>());
        world.reserve_component_changes(2);
        let capacity = world.component_changes.capacity();

        world.insert(entity, 2u16);
        world.insert(entity, 3u16);
        world.insert(entity, true);
        assert_eq!(world.remove::<u16>(entity), Some(3));
        let mut changes = alloc::vec::Vec::new();
        world.drain_component_changes(|world, change| {
            assert!(world.get::<u16>(change.entity).is_none());
            changes.push(change);
        });
        assert_eq!(
            changes,
            [ComponentChange {
                entity,
                type_id: TypeId::of::<u16>()
            }]
        );
        assert_eq!(world.component_changes.capacity(), capacity);
        assert!(world.component_changes.is_empty());
    }

    #[test]
    fn despawn_records_only_watched_component_types() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 1u16);
        world.insert(entity, 2u32);
        world.watch_component_type(TypeId::of::<u16>());
        world.reserve_component_changes(1);
        assert!(world.despawn(entity));
        assert!(!world.despawn(entity));
        let mut count = 0;
        world.drain_component_changes(|world, change| {
            assert_eq!(change.entity, entity);
            assert_eq!(change.type_id, TypeId::of::<u16>());
            assert!(!world.is_alive(entity));
            count += 1;
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn separated_resource_is_restored_after_callback_unwinds() {
        let mut world = World::new();
        world.insert_resource(7u16);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.with_resource_box::<u16, _>(|value, world| {
                assert!(world.resource::<u16>().is_none());
                *value += 1;
                panic!("callback failed");
            });
        }));
        assert!(failure.is_err());
        assert_eq!(world.resource::<u16>(), Some(&8));
    }
}
