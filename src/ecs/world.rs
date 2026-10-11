use alloc::boxed::Box;
use alloc::rc::{Rc, Weak};
use core::any::{Any, TypeId};
use core::cell::RefCell;
use core::marker::PhantomData;
use core::ptr::NonNull;
use hashbrown::HashMap;
use rustc_hash::FxBuildHasher;

use super::bundle::Component;
use super::entity::{Entity, EntityAllocator};
use super::sparse_set::{SparseSet, SparseSetParts};

/// Exclusive World capability used while building typed system parameters.
///
/// The type is an opaque, doc-hidden macro support seam. Construction and raw
/// component access stay crate-private. Snapshots obtained from this cell are
/// only sound when the complete parameter set has already been checked for
/// conflicting access and cannot structurally mutate the World.
#[cfg_attr(not(test), allow(dead_code))]
#[doc(hidden)]
pub struct SystemWorldCell<'world> {
    storages: NonNull<HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>>,
    resources: NonNull<HashMap<TypeId, Box<dyn Any>, FxBuildHasher>>,
    watched_component_types: NonNull<alloc::vec::Vec<WatchedComponentType>>,
    component_changes: NonNull<alloc::vec::Vec<ComponentChange>>,
    _marker: PhantomData<&'world mut World>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'world> SystemWorldCell<'world> {
    pub(crate) fn new(world: &'world mut World) -> Self {
        Self {
            storages: NonNull::from(&mut world.storages),
            resources: NonNull::from(&mut world.resources),
            watched_component_types: NonNull::from(&mut world.watched_component_types),
            component_changes: NonNull::from(&mut world.component_changes),
            _marker: PhantomData,
        }
    }

    /// # Safety
    ///
    /// The caller must reject incompatible accesses to `T` before calling,
    /// and no component storage may change structure for `'world`.
    pub(crate) unsafe fn component_parts<T: Component>(&self) -> Option<SparseSetParts<'world, T>> {
        // SAFETY: SystemWorldCell originates from a unique World borrow. The
        // storage map is disjoint from resource and change-queue fields, and
        // caller prevents structural storage changes. Shared lookup is
        // deliberate: one query may snapshot the same SparseSet as its driver,
        // filter, and row data without creating overlapping `&mut SparseSet`
        // references. SparseSet's value cells provide the controlled interior
        // mutability needed by validated Write<T> elements.
        let storages = unsafe { &*self.storages.as_ptr() };
        storages.get(&TypeId::of::<T>()).map(|storage| {
            let storage = storage
                .as_any()
                .downcast_ref::<SparseSet<T>>()
                .expect("component storage type");
            // SAFETY: delegated from SystemWorldCell::component_parts.
            unsafe { SparseSet::parts(storage) }
        })
    }

    /// # Safety
    ///
    /// The complete system parameter set must not contain a mutable access to
    /// `T`. Resource structure must remain unchanged for `'world`.
    pub(crate) unsafe fn resource<T: 'static>(&self) -> Option<&'world T> {
        // SAFETY: the caller validated the complete resource access plan. The
        // pointer is branded by the unique World borrow held by this cell.
        let resources = unsafe { &*self.resources.as_ptr() };
        let value = resources.get(&TypeId::of::<T>())?.downcast_ref::<T>()? as *const T;
        Some(unsafe { &*value })
    }

    /// # Safety
    ///
    /// The complete system parameter set must make this the only access to
    /// `T`. Resource structure must remain unchanged for `'world`.
    pub(crate) unsafe fn resource_mut<T: 'static>(&self) -> Option<&'world mut T> {
        // SAFETY: the caller validated exclusive access to T. The pointer is
        // branded by the unique World borrow held by this cell.
        let resources = unsafe { &mut *self.resources.as_ptr() };
        let value = resources.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()? as *mut T;
        Some(unsafe { &mut *value })
    }

    /// # Safety
    ///
    /// The returned sink may only be used by query states built from this
    /// cell. No other path may mutate the change queue during that window.
    pub(crate) unsafe fn component_change_sink(&self) -> ComponentChangeSink<'world> {
        ComponentChangeSink {
            watched_component_types: self.watched_component_types,
            component_changes: self.component_changes,
            _marker: PhantomData,
        }
    }

    /// Whether mutable access to `T` needs an entity-level change record.
    ///
    /// # Safety
    ///
    /// The cell must own the active system World window. No other path may
    /// mutate the watched-type registry during this call.
    pub(crate) unsafe fn watches_component<T: Component>(&self) -> bool {
        // SAFETY: this pointer targets a field disjoint from component storage.
        unsafe { self.watched_component_types.as_ref() }
            .iter()
            .any(|watched| watched.type_id == TypeId::of::<T>() && watched.entity_tracking)
    }

    /// Reserve the complete system's change-record budget before any mutable
    /// row is yielded.
    ///
    /// # Safety
    ///
    /// The cell must own the active system World window. No other path may
    /// mutate the change queue during this call.
    pub(crate) unsafe fn reserve_component_change_records(&self, additional: usize) {
        // SAFETY: this pointer targets a field disjoint from component storage.
        let changes = unsafe { &mut *self.component_changes.as_ptr() };
        changes
            .try_reserve(additional)
            .expect("component change queue capacity exhausted");
    }
}

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
pub(crate) struct ComponentChange {
    pub entity: Entity,
    pub type_id: TypeId,
}

#[derive(Clone, Copy)]
struct WatchedComponentType {
    type_id: TypeId,
    revision: u64,
    entity_tracking: bool,
    query_tracking: bool,
    query_pending: Option<QueryDependencyCause>,
    query_snapshot: Option<QueryDependencyCause>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryDependencyCause {
    /// Component values or membership changed on one or more entities.
    Component,
    /// Referenced storage changed even if component identities stayed equal.
    Source,
}

impl QueryDependencyCause {
    fn merge(self, pending: Option<Self>) -> Self {
        match (self, pending) {
            (Self::Source, _) | (_, Some(Self::Source)) => Self::Source,
            _ => Self::Component,
        }
    }
}

impl WatchedComponentType {
    fn record_query_change(&mut self, cause: QueryDependencyCause) {
        if self.query_tracking {
            self.query_pending = Some(cause.merge(self.query_pending));
        }
    }
}

/// Disjoint access to component-change records and dependency flags while generated queries hold references into component storage.
pub(crate) struct ComponentChangeSink<'world> {
    watched_component_types: NonNull<alloc::vec::Vec<WatchedComponentType>>,
    component_changes: NonNull<alloc::vec::Vec<ComponentChange>>,
    _marker: PhantomData<&'world mut World>,
}

impl ComponentChangeSink<'_> {
    /// Record the conservative write represented by one yielded `mut T` row.
    ///
    /// # Safety
    ///
    /// The sink must originate from the same live `SystemWorldCell` as the yielded component reference, and callers must not access the registry or change queue through another path while this method runs.
    pub(crate) unsafe fn record<T: Component>(&self, entity: Entity) {
        // SAFETY: the pointers target distinct World fields and the caller
        // holds the system's exclusive World window.
        let watched = unsafe { &mut *self.watched_component_types.as_ptr() };
        let changes = unsafe { &mut *self.component_changes.as_ptr() };
        record_component_change(watched, changes, entity, TypeId::of::<T>());
    }
}

fn record_component_change(
    watched_component_types: &mut [WatchedComponentType],
    component_changes: &mut alloc::vec::Vec<ComponentChange>,
    entity: Entity,
    type_id: TypeId,
) {
    let Some(watched) = watched_component_types
        .iter_mut()
        .find(|watched| watched.type_id == type_id)
    else {
        return;
    };
    watched.record_query_change(QueryDependencyCause::Component);
    watched.revision = watched.revision.wrapping_add(1);
    if !watched.entity_tracking {
        return;
    }
    let change = ComponentChange { entity, type_id };
    if !component_changes.contains(&change) {
        if component_changes.len() == component_changes.capacity() {
            component_changes
                .try_reserve(1)
                .expect("component change queue capacity exhausted");
        }
        component_changes.push(change);
    }
}

fn record_query_dependency_change(
    watched_component_types: &mut [WatchedComponentType],
    type_id: TypeId,
) {
    if let Some(watched) = watched_component_types
        .iter_mut()
        .find(|watched| watched.type_id == type_id)
    {
        watched.record_query_change(QueryDependencyCause::Source);
    }
}

struct WorldDropHook {
    kind: TypeId,
    callback: fn(WorldId),
}

pub struct World {
    id: WorldId,
    allocator: EntityAllocator,
    storages: HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    resources: HashMap<TypeId, Box<dyn Any>, FxBuildHasher>,
    watched_component_types: alloc::vec::Vec<WatchedComponentType>,
    component_changes: alloc::vec::Vec<ComponentChange>,
    drop_hooks: RefCell<alloc::vec::Vec<WorldDropHook>>,
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
            drop_hooks: RefCell::new(alloc::vec::Vec::new()),
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

    pub(crate) fn register_drop_hook<T: 'static>(&self, hook: fn(WorldId)) {
        let kind = TypeId::of::<T>();
        let mut drop_hooks = self.drop_hooks.borrow_mut();
        if !drop_hooks.iter().any(|registered| registered.kind == kind) {
            drop_hooks.push(WorldDropHook {
                kind,
                callback: hook,
            });
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
            let type_id = self.watched_component_types[index].type_id;
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

    pub(crate) fn allocated_entity_slots(&self) -> u32 {
        self.allocator.allocated_slots()
    }

    pub(crate) fn live_entity_count(&self) -> usize {
        self.allocator.live_count()
    }

    pub(crate) fn component_count(&self, kind: TypeId) -> usize {
        self.storages
            .get(&kind)
            .map_or(0, |storage| storage.entities().len())
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
    pub(crate) fn watch_component_type(&mut self, type_id: TypeId) {
        self.watch_component_type_entry(type_id).entity_tracking = true;
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

    /// Track type-level writes without recording entities or creating storage.
    pub(crate) fn watch_query_dependency_type(&mut self, type_id: TypeId) {
        self.watch_component_type_entry(type_id).query_tracking = true;
    }

    pub(crate) fn watch_component_revision<T: 'static>(&mut self) {
        self.watch_component_type_entry(TypeId::of::<T>());
    }

    pub(crate) fn component_revision<T: 'static>(&self) -> u64 {
        self.watched_component_types
            .iter()
            .find(|watched| watched.type_id == TypeId::of::<T>())
            .map_or(0, |watched| watched.revision)
    }

    fn watch_component_type_entry(&mut self, type_id: TypeId) -> &mut WatchedComponentType {
        let index = if let Some(index) = self
            .watched_component_types
            .iter()
            .position(|watched| watched.type_id == type_id)
        {
            index
        } else {
            let index = self.watched_component_types.len();
            self.watched_component_types.push(WatchedComponentType {
                type_id,
                revision: 0,
                entity_tracking: false,
                query_tracking: false,
                query_pending: None,
                query_snapshot: None,
            });
            index
        };
        &mut self.watched_component_types[index]
    }

    pub(crate) fn reserve_component_storage<T: 'static>(
        &mut self,
        max_entity_id: u32,
        additional: usize,
    ) {
        self.storage_mut::<T>()
            .reserve_entities(max_entity_id, additional);
    }

    fn record_component_change(&mut self, entity: Entity, type_id: TypeId) {
        record_component_change(
            &mut self.watched_component_types,
            &mut self.component_changes,
            entity,
            type_id,
        );
    }

    /// Visit queued changes outside component storage borrows. New structural
    /// changes made by the callback are visited in this same pass.
    pub(crate) fn drain_component_changes(
        &mut self,
        mut visit: impl FnMut(&mut Self, ComponentChange),
    ) {
        let mut index = 0;
        while let Some(change) = self.component_changes.get(index).copied() {
            index += 1;
            visit(self, change);
        }
        self.component_changes.clear();
    }

    /// Freeze all pending type changes before visiting this batch once. Writes made by callbacks remain pending for the next drain. An interrupted snapshot is retained until its callbacks complete successfully.
    pub(crate) fn drain_query_dependency_changes(
        &mut self,
        mut visit: impl FnMut(&mut Self, TypeId, QueryDependencyCause),
    ) {
        let count = self.watched_component_types.len();
        for watched in &mut self.watched_component_types {
            if let Some(pending) = watched.query_pending.take() {
                watched.query_snapshot = Some(pending.merge(watched.query_snapshot));
            }
        }
        for index in 0..count {
            let watched = self.watched_component_types[index];
            if let Some(cause) = watched.query_snapshot {
                visit(self, watched.type_id, cause);
                self.watched_component_types[index].query_snapshot = None;
            }
        }
    }

    /// Invalidates references whose backing data changed without replacing their component values.
    pub(crate) fn invalidate_query_dependency<T: 'static>(&mut self) {
        record_query_dependency_change(&mut self.watched_component_types, TypeId::of::<T>());
    }

    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.storage::<T>()?.get(entity)
    }

    /// Successful mutable access records a possible component change and marks query dependencies. Registered bindings reconcile the component's final value before layout and painting.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        let type_id = TypeId::of::<T>();
        let component = self
            .storages
            .get_mut(&type_id)?
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            .expect("component storage type")
            .get_mut(entity)?;
        record_component_change(
            &mut self.watched_component_types,
            &mut self.component_changes,
            entity,
            type_id,
        );
        Some(component)
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
    pub fn for_each_stable<T: 'static>(&mut self, visit: impl FnMut(&mut Self, Entity)) {
        self.for_each_stable_type(TypeId::of::<T>(), visit);
    }

    /// Visit a prepared component type without holding its storage borrow across the callback. The callback must preserve that type's structure.
    pub(crate) fn for_each_stable_type(
        &mut self,
        type_id: TypeId,
        mut visit: impl FnMut(&mut Self, Entity),
    ) {
        let count = self.component_count(type_id);
        for index in 0..count {
            let entity = self
                .storages
                .get(&type_id)
                .and_then(|storage| storage.entities().get(index))
                .copied()
                .expect("stable component storage changed during iteration");
            visit(self, entity);
            debug_assert_eq!(
                self.component_count(type_id),
                count,
                "for_each_stable callback inserted or removed the iterated component"
            );
            debug_assert_eq!(
                self.storages
                    .get(&type_id)
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

    /// Register a resource, reusing its storage when the type is already present.
    pub fn insert_resource<T: 'static>(&mut self, value: T) {
        if let Some(existing) = self.resources.get_mut(&TypeId::of::<T>()) {
            let slot = existing
                .downcast_mut::<T>()
                .expect("resource type does not match its TypeId");
            drop(core::mem::replace(slot, value));
        } else {
            self.resources.insert(TypeId::of::<T>(), Box::new(value));
        }
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
        for hook in self.drop_hooks.get_mut() {
            (hook.callback)(self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::tracked_allocations;

    #[test]
    fn component_revisions_preserve_repeated_writes_without_entity_tracking() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.watch_component_revision::<u32>();
        world.insert(entity, 1u32);
        let revision = world.component_revision::<u32>();
        world.insert(entity, 1u32);
        assert_ne!(world.component_revision::<u32>(), revision);
        assert!(world.component_changes.is_empty());
        let revision = world.component_revision::<u32>();
        world.remove::<u32>(entity);
        assert_ne!(world.component_revision::<u32>(), revision);
        let revision = world.component_revision::<u32>();
        world.remove::<u32>(entity);
        assert_eq!(world.component_revision::<u32>(), revision);
    }
    #[test]
    fn query_change_causes_coalesce_without_entity_records_or_allocations() {
        use QueryDependencyCause::{Component, Source};

        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 0u16);
        world.watch_query_dependency_type(TypeId::of::<u16>());
        let allocations = tracked_allocations(|| {
            for (changes, expected) in [
                (&[Component][..], Component),
                (&[Source][..], Source),
                (&[Component, Source][..], Source),
                (&[Source, Component][..], Source),
                (&[Source, Source][..], Source),
            ] {
                for cause in changes {
                    match cause {
                        Component => *world.get_mut::<u16>(entity).unwrap() += 1,
                        Source => world.invalidate_query_dependency::<u16>(),
                    }
                }
                let mut calls = 0;
                world.drain_query_dependency_changes(|_, kind, cause| {
                    assert_eq!(kind, TypeId::of::<u16>());
                    assert_eq!(cause, expected);
                    calls += 1;
                });
                assert_eq!(calls, 1);
                assert!(world.component_changes.is_empty());
                world.drain_query_dependency_changes(|_, _, _| panic!("already delivered"));
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(core::mem::size_of::<Option<QueryDependencyCause>>(), 1);
    }

    #[test]
    fn query_source_changes_during_drain_wait_for_the_next_snapshot() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.watch_query_dependency_type(TypeId::of::<u16>());
        world.watch_query_dependency_type(TypeId::of::<u32>());
        world.insert(entity, 0u16);
        world.insert(entity, 0u32);
        let mut calls = 0;
        world.drain_query_dependency_changes(|world, _, cause| {
            assert_eq!(cause, QueryDependencyCause::Component);
            world.invalidate_query_dependency::<u16>();
            world.invalidate_query_dependency::<u32>();
            calls += 1;
        });
        assert_eq!(calls, 2);
        calls = 0;
        world.drain_query_dependency_changes(|world, _, cause| {
            assert_eq!(cause, QueryDependencyCause::Source);
            *world.get_mut::<u16>(entity).unwrap() += 1;
            calls += 1;
        });
        assert_eq!(calls, 2);
        calls = 0;
        world.drain_query_dependency_changes(|_, kind, cause| {
            assert_eq!(kind, TypeId::of::<u16>());
            assert_eq!(cause, QueryDependencyCause::Component);
            calls += 1;
        });
        assert_eq!(calls, 1);
    }

    #[test]
    fn query_source_invalidation_survives_interrupted_delivery() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 0u16);
        world.watch_query_dependency_type(TypeId::of::<u16>());
        world.invalidate_query_dependency::<u16>();
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.drain_query_dependency_changes(|world, _, cause| {
                assert_eq!(cause, QueryDependencyCause::Source);
                *world.get_mut::<u16>(entity).unwrap() += 1;
                panic!("dependency callback failed");
            });
        }));
        assert!(failed.is_err());
        let mut calls = 0;
        world.drain_query_dependency_changes(|_, _, cause| {
            assert_eq!(cause, QueryDependencyCause::Source);
            calls += 1;
        });
        assert_eq!(calls, 1);
        world.drain_query_dependency_changes(|_, _, _| panic!("already delivered"));
    }

    #[test]
    fn query_dependencies_share_registration_with_late_entity_tracking() {
        let mut world = World::new();
        let kind = TypeId::of::<u16>();
        world.watch_query_dependency_type(kind);
        world.watch_query_dependency_type(kind);
        assert!(world.storages.is_empty());
        assert_eq!(world.watched_component_types.len(), 1);
        assert!(!world.watched_component_types[0].entity_tracking);
        assert!(world.watched_component_types[0].query_tracking);
        world.drain_query_dependency_changes(|_, _, _| panic!("registration is not a write"));

        let entity = world.spawn_empty();
        world.insert(entity, 1u16);
        assert!(world.component_changes.is_empty());
        let mut notifications = 0;
        world.drain_query_dependency_changes(|_, changed, _| {
            assert_eq!(changed, kind);
            notifications += 1;
        });
        assert_eq!(notifications, 1);

        world.watch_component_type(kind);
        world.watch_component_type(kind);
        assert_eq!(world.watched_component_types.len(), 1);
        assert!(world.watched_component_types[0].entity_tracking);
        assert!(world.watched_component_types[0].query_tracking);
        assert_eq!(
            world.component_changes,
            [ComponentChange {
                entity,
                type_id: kind
            }]
        );
    }

    #[test]
    fn get_mut_records_only_successful_watched_component_access() {
        let mut world = World::new();
        let first = world.spawn_empty();
        let absent = world.spawn_empty();
        let kind = TypeId::of::<u16>();
        world.watch_component_type(kind);
        world.watch_query_dependency_type(kind);
        assert!(world.get_mut::<u16>(first).is_none());
        assert!(world.storages.is_empty());
        assert!(world.component_changes.is_empty());
        world.drain_query_dependency_changes(|_, _, _| panic!("absent storage is not a write"));

        world.insert(first, 1u16);
        let retired = world.spawn_empty();
        world.insert(retired, 3u16);
        assert!(world.despawn(retired));
        let replacement = world.spawn_empty();
        assert_eq!(replacement.id, retired.id);
        assert_ne!(replacement.generation, retired.generation);
        world.insert(replacement, 4u16);
        world.drain_component_changes(|_, _| {});
        world.drain_query_dependency_changes(|_, _, _| {});
        assert!(world.get_mut::<u16>(absent).is_none());
        assert!(world.get_mut::<u16>(retired).is_none());
        assert_eq!(world.get::<u16>(first), Some(&1));
        assert_eq!(world.get::<u16>(replacement), Some(&4));
        assert!(world.component_changes.is_empty());
        world.drain_query_dependency_changes(|_, _, _| {
            panic!("reads and absent values are not writes")
        });

        let changes = world.component_changes.as_ptr();
        let capacity = world.component_changes.capacity();
        let mut notifications = 0;
        let allocations = tracked_allocations(|| {
            for value in 2..66 {
                *world.get_mut::<u16>(first).unwrap() = value;
                *world.get_mut::<u16>(first).unwrap() = value + 1;
                assert_eq!(
                    world.component_changes,
                    [ComponentChange {
                        entity: first,
                        type_id: kind,
                    }]
                );
                world.drain_component_changes(|world, change| {
                    assert_eq!(world.get::<u16>(change.entity), Some(&(value + 1)));
                });
                world.drain_query_dependency_changes(|_, changed, _| {
                    assert_eq!(changed, kind);
                    notifications += 1;
                });
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(notifications, 64);
        assert_eq!(world.component_changes.as_ptr(), changes);
        assert_eq!(world.component_changes.capacity(), capacity);
        assert!(world.component_changes.is_empty());
    }

    #[test]
    fn structural_query_dependencies_coalesce_without_extra_entity_records() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let query_only = TypeId::of::<u16>();
        let both = TypeId::of::<u32>();
        world.watch_query_dependency_type(query_only);
        world.watch_query_dependency_type(both);
        world.watch_component_type(both);
        world.watch_component_type(TypeId::of::<bool>());
        world.insert(entity, 1u16);
        world.insert(entity, 2u16);
        world.insert(entity, 3u32);
        world.insert(entity, true);
        let mut changed = alloc::vec::Vec::new();
        world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
        assert_eq!(changed, [query_only, both]);
        assert_eq!(world.component_changes.len(), 2);
        world.drain_component_changes(|_, _| {});

        assert_eq!(world.remove::<u16>(entity), Some(2));
        assert!(world.remove::<u16>(entity).is_none());
        changed.clear();
        world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
        assert_eq!(changed, [query_only]);
        assert!(world.component_changes.is_empty());

        world.insert(entity, 4u16);
        world.drain_query_dependency_changes(|_, _, _| {});
        assert!(world.despawn(entity));
        assert!(!world.despawn(entity));
        changed.clear();
        world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
        assert_eq!(changed, [query_only, both]);
        assert_eq!(world.component_changes.len(), 2);
        assert!(
            world
                .component_changes
                .iter()
                .all(|change| change.entity == entity)
        );
        assert!(
            world
                .component_changes
                .iter()
                .all(|change| change.type_id != query_only)
        );
    }

    #[test]
    fn query_dependency_drain_freezes_all_types_before_callbacks() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let first = TypeId::of::<u16>();
        let second = TypeId::of::<u32>();
        let added = TypeId::of::<bool>();
        world.watch_query_dependency_type(first);
        world.watch_query_dependency_type(second);
        world.insert(entity, 1u16);
        world.insert(entity, 2u32);

        let mut changed = alloc::vec::Vec::new();
        world.drain_query_dependency_changes(|world, kind, _| {
            changed.push(kind);
            if kind == first {
                *world.get_mut::<u16>(entity).unwrap() += 1;
                *world.get_mut::<u32>(entity).unwrap() += 1;
                world.watch_query_dependency_type(added);
                world.insert(entity, true);
            }
        });
        assert_eq!(changed, [first, second]);
        changed.clear();
        world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
        assert_eq!(changed, [first, second, added]);
        world.drain_query_dependency_changes(|_, _, _| panic!("snapshot was already consumed"));
    }

    #[test]
    fn query_dependency_drain_retains_unfinished_snapshot_after_unwind() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let first = TypeId::of::<u16>();
        let second = TypeId::of::<u32>();
        world.watch_query_dependency_type(first);
        world.watch_query_dependency_type(second);
        world.insert(entity, 1u16);
        world.insert(entity, 2u32);
        let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            world.drain_query_dependency_changes(|world, kind, _| {
                assert_eq!(kind, first);
                *world.get_mut::<u16>(entity).unwrap() += 1;
                panic!("dependency callback failed");
            });
        }));
        assert!(failed.is_err());
        let mut changed = alloc::vec::Vec::new();
        world.drain_query_dependency_changes(|_, kind, _| changed.push(kind));
        assert_eq!(changed, [first, second]);
        world.drain_query_dependency_changes(|_, _, _| panic!("snapshot was already consumed"));
    }

    #[test]
    fn erased_stable_iteration_uses_current_storage_without_allocating() {
        let mut world = World::new();
        world.for_each_stable_type(TypeId::of::<u16>(), |_, _| panic!("absent storage"));
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        for entity in [first, second] {
            world.insert(entity, 1u16);
            world.insert(entity, false);
        }
        let allocations = tracked_allocations(|| {
            world.for_each_stable_type(TypeId::of::<u16>(), |world, entity| {
                *world.get_mut::<u16>(entity).unwrap() += 1;
                *world.get_mut::<bool>(entity).unwrap() = true;
            });
        });
        assert_eq!(allocations, 0);
        assert_eq!(world.get::<u16>(first), Some(&2));
        assert_eq!(world.get::<u16>(second), Some(&2));
        assert_eq!(world.get::<bool>(first), Some(&true));
        assert_eq!(world.get::<bool>(second), Some(&true));
    }

    #[test]
    #[should_panic(expected = "inserted or removed the iterated component")]
    fn erased_stable_iteration_rejects_driver_structure_changes() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 1u16);
        world.for_each_stable_type(TypeId::of::<u16>(), |world, entity| {
            world.remove::<u16>(entity);
        });
    }

    #[test]
    fn prepared_query_dependency_writes_and_first_drain_do_not_allocate() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, 1u16);
        world.watch_query_dependency_type(TypeId::of::<u16>());
        let registry = world.watched_component_types.as_ptr();
        let registry_capacity = world.watched_component_types.capacity();
        let entity_capacity = world.component_changes.capacity();
        let mut notifications = 0;
        let allocations = tracked_allocations(|| {
            for _ in 0..64 {
                *world.get_mut::<u16>(entity).unwrap() += 1;
            }
            world.drain_query_dependency_changes(|world, kind, _| {
                assert_eq!(kind, TypeId::of::<u16>());
                world.for_each_stable_type(kind, |world, entity| {
                    assert_eq!(world.get::<u16>(entity), Some(&65));
                });
                notifications += 1;
            });
        });
        assert_eq!(allocations, 0);
        assert_eq!(notifications, 1);
        assert_eq!(world.watched_component_types.as_ptr(), registry);
        assert_eq!(world.watched_component_types.capacity(), registry_capacity);
        assert_eq!(world.component_changes.capacity(), entity_capacity);
        assert!(world.component_changes.is_empty());
    }

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
        world.component_changes.reserve(2);
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
        world.component_changes.reserve(1);
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

    #[test]
    fn replacing_resource_reuses_its_storage() {
        let mut world = World::new();
        world.insert_resource(7u16);
        let original = world.resource::<u16>().unwrap() as *const u16;

        world.insert_resource(8u16);

        assert_eq!(world.resource::<u16>(), Some(&8));
        assert_eq!(world.resource::<u16>().unwrap() as *const u16, original);
    }
}
