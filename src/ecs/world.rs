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

mod component_lifetimes;
use component_lifetimes::ComponentLifetimes;

type ResourceSlot = Option<Box<dyn Any>>;
type ResourceMap = HashMap<TypeId, ResourceSlot, FxBuildHasher>;

// Share table probes across typed accessors instead of inlining one per type.
#[inline(never)]
fn component_storage(
    storages: &HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    type_id: TypeId,
) -> Option<&dyn ComponentStorage> {
    storages.get(&type_id).map(Box::as_ref)
}

#[inline(never)]
fn component_storage_mut(
    storages: &mut HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    type_id: TypeId,
) -> Option<&mut Box<dyn ComponentStorage>> {
    storages.get_mut(&type_id)
}

#[inline(never)]
fn component_storage_or_insert(
    storages: &mut HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    type_id: TypeId,
    create: fn() -> Box<dyn ComponentStorage>,
) -> &mut Box<dyn ComponentStorage> {
    storages.entry(type_id).or_insert_with(create)
}

#[inline(never)]
fn resource_value(resources: &ResourceMap, type_id: TypeId) -> Option<&dyn Any> {
    resources.get(&type_id)?.as_deref()
}

#[inline(never)]
fn resource_slot_mut(resources: &mut ResourceMap, type_id: TypeId) -> Option<&mut ResourceSlot> {
    resources.get_mut(&type_id)
}

const _: () = {
    assert!(core::mem::size_of::<ResourceSlot>() == core::mem::size_of::<Box<dyn Any>>());
    assert!(core::mem::align_of::<ResourceSlot>() == core::mem::align_of::<Box<dyn Any>>());
};

/// Exclusive World capability used while building typed system parameters.
///
/// The type is an opaque, doc-hidden macro support seam. Construction and raw
/// component access stay crate-private. Snapshots obtained from this cell are
/// only sound when the complete parameter set has already been checked for
/// conflicting access and cannot structurally mutate the World.
#[cfg_attr(not(test), allow(dead_code))]
#[doc(hidden)]
pub struct SystemWorldCell<'world> {
    owner: WorldId,
    storages: NonNull<HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>>,
    resources: NonNull<ResourceMap>,
    watched_component_types: NonNull<alloc::vec::Vec<WatchedComponentType>>,
    component_changes: NonNull<alloc::vec::Vec<ComponentChange>>,
    _marker: PhantomData<&'world mut World>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'world> SystemWorldCell<'world> {
    pub(crate) fn new(world: &'world mut World) -> Self {
        Self {
            owner: world.id(),
            storages: NonNull::from(&mut world.storages),
            resources: NonNull::from(&mut world.resources),
            watched_component_types: NonNull::from(&mut world.watched_component_types),
            component_changes: NonNull::from(&mut world.component_changes),
            _marker: PhantomData,
        }
    }

    pub(crate) fn owner(&self) -> WorldId {
        self.owner
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
        component_storage(storages, TypeId::of::<T>()).map(|storage| {
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
        let value = resource_value(resources, TypeId::of::<T>())?.downcast_ref::<T>()? as *const T;
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
        let value = resource_slot_mut(resources, TypeId::of::<T>())?
            .as_mut()?
            .downcast_mut::<T>()? as *mut T;
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
    pub(crate) fn diagnostic_id(self) -> u64 {
        self.0
    }
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
    pub kind: ComponentChangeKind,
}

/// Classification of a mutable component write for View invalidation.
///
/// `Component` is intentionally conservative: arbitrary mutable component
/// access does not expose which fields changed. Typed framework mutation can
/// provide a narrower classification without changing query dependency
/// tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComponentChangeKind {
    Component,
    Visual,
    Transform,
    Layout,
    InheritedStyle(u8),
}

impl ComponentChangeKind {
    pub(crate) const LOCAL_LAYOUT: u8 = 1 << 7;

    pub(crate) fn merge(self, other: Self) -> Self {
        if matches!(self, Self::Component) || matches!(other, Self::Component) {
            return Self::Component;
        }
        match (self, other) {
            (Self::InheritedStyle(a), Self::InheritedStyle(b)) => Self::InheritedStyle(a | b),
            (Self::InheritedStyle(fields), Self::Layout)
            | (Self::Layout, Self::InheritedStyle(fields)) => {
                Self::InheritedStyle(fields | Self::LOCAL_LAYOUT)
            }
            (Self::InheritedStyle(fields), _) | (_, Self::InheritedStyle(fields)) => {
                Self::InheritedStyle(fields)
            }
            (Self::Layout, _) | (_, Self::Layout) => Self::Layout,
            (Self::Transform, _) | (_, Self::Transform) => Self::Transform,
            _ => Self::Visual,
        }
    }
}

struct FilteredComponentRevision {
    accepts: fn(ComponentChangeKind) -> bool,
    revision: u64,
}

struct WatchedComponentType {
    type_id: TypeId,
    revision: u64,
    membership_revision: u64,
    filtered: Option<Box<FilteredComponentRevision>>,
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
        record_component_change(
            watched,
            changes,
            entity,
            TypeId::of::<T>(),
            ComponentChangeKind::Component,
        );
    }
}

fn record_component_change(
    watched_component_types: &mut [WatchedComponentType],
    component_changes: &mut alloc::vec::Vec<ComponentChange>,
    entity: Entity,
    type_id: TypeId,
    kind: ComponentChangeKind,
) {
    let Some(watched) = watched_component_types
        .iter_mut()
        .find(|watched| watched.type_id == type_id)
    else {
        return;
    };
    watched.record_query_change(QueryDependencyCause::Component);
    watched.revision = watched.revision.wrapping_add(1);
    if let Some(filtered) = watched.filtered.as_mut()
        && (filtered.accepts)(kind)
    {
        filtered.revision = watched.revision;
    }
    if !watched.entity_tracking {
        return;
    }
    let change = ComponentChange {
        entity,
        type_id,
        kind,
    };
    if let Some(existing) = component_changes
        .iter_mut()
        .find(|existing| existing.entity == entity && existing.type_id == type_id)
    {
        existing.kind = existing.kind.merge(kind);
    } else {
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
    retirement_revision: u64,
    allocator: EntityAllocator,
    storages: HashMap<TypeId, Box<dyn ComponentStorage>, FxBuildHasher>,
    resources: ResourceMap,
    watched_component_types: alloc::vec::Vec<WatchedComponentType>,
    component_changes: alloc::vec::Vec<ComponentChange>,
    drop_hooks: RefCell<alloc::vec::Vec<WorldDropHook>>,
    lifetime: Option<Rc<()>>,
    callback_observer: Option<fn(&mut World)>,
    component_lifetimes: Option<Box<ComponentLifetimes>>,
}

impl Default for World {
    fn default() -> Self {
        Self {
            id: WorldId::next(),
            retirement_revision: 0,
            allocator: EntityAllocator::new(),
            storages: HashMap::default(),
            resources: HashMap::default(),
            watched_component_types: alloc::vec::Vec::new(),
            component_changes: alloc::vec::Vec::new(),
            drop_hooks: RefCell::new(alloc::vec::Vec::new()),
            lifetime: Some(Rc::new(())),
            callback_observer: None,
            component_lifetimes: None,
        }
    }
}

impl World {
    pub(crate) fn set_callback_observer(&mut self, observer: fn(&mut World)) {
        self.callback_observer = Some(observer);
    }

    pub(crate) fn callback_completed(&mut self) {
        if let Some(observer) = self.callback_observer {
            observer(self);
        }
    }

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
        self.retirement_revision = self
            .retirement_revision
            .checked_add(1)
            .expect("entity retirement revision exhausted");
        for index in 0..self.watched_component_types.len() {
            let type_id = self.watched_component_types[index].type_id;
            if self.has_type(entity, type_id) {
                self.record_membership_change(type_id);
                self.record_component_change(entity, type_id);
            }
        }
        for storage in self.storages.values_mut() {
            storage.remove_entity(entity);
        }
        if let Some(lifetimes) = &mut self.component_lifetimes {
            lifetimes.despawn(entity);
        }
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.allocator.is_alive(entity)
    }

    pub(crate) fn retirement_revision(&self) -> u64 {
        self.retirement_revision
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
        let added = !storage.contains(entity);
        storage.insert(entity, component);
        if added {
            self.record_membership_change(TypeId::of::<T>());
        }
        if let Some(lifetimes) = &mut self.component_lifetimes {
            lifetimes.replace(entity, TypeId::of::<T>());
        }
        self.record_component_change(entity, TypeId::of::<T>());
    }

    pub fn remove<T: 'static>(&mut self, entity: Entity) -> Option<T> {
        let removed = self.storage_mut_if_exists::<T>()?.remove(entity);
        if removed.is_some() {
            self.record_membership_change(TypeId::of::<T>());
            if let Some(lifetimes) = &mut self.component_lifetimes {
                lifetimes.remove(entity, TypeId::of::<T>());
            }
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

    pub(crate) fn watch_component_lifetime(&mut self, kind: TypeId) {
        let lifetimes = self
            .component_lifetimes
            .get_or_insert_with(|| Box::new(ComponentLifetimes::default()));
        if !lifetimes.watch(kind) {
            return;
        }
        if let Some(storage) = self.storages.get(&kind) {
            for &entity in storage.entities() {
                lifetimes.replace(entity, kind);
            }
        }
    }

    pub(crate) fn component_generation(&self, entity: Entity, kind: TypeId) -> Option<u64> {
        self.component_lifetimes.as_ref()?.generation(entity, kind)
    }

    pub(crate) fn acknowledge_attachment_change(&mut self, entity: Entity) {
        if let Some(lifetimes) = self.component_lifetimes.as_mut() {
            lifetimes.acknowledge(entity);
        }
    }

    pub(crate) fn next_attachment_change(&mut self) -> Option<(Entity, bool)> {
        self.component_lifetimes.as_mut()?.next_pending()
    }

    pub(crate) fn pending_attachment_changes(&self) -> usize {
        self.component_lifetimes
            .as_ref()
            .map_or(0, |lifetimes| lifetimes.pending_len())
    }

    pub(crate) fn queue_existing_attachments(&mut self, kind: TypeId) {
        let lifetimes = self
            .component_lifetimes
            .get_or_insert_with(|| Box::new(ComponentLifetimes::default()));
        if let Some(storage) = self.storages.get(&kind) {
            for &entity in storage.entities() {
                lifetimes.enqueue(entity);
            }
        }
    }

    /// Track type-level writes without recording entities or creating storage.
    pub(crate) fn watch_query_dependency_type(&mut self, type_id: TypeId) {
        self.watch_component_type_entry(type_id).query_tracking = true;
    }

    pub(crate) fn watch_component_revision<T: 'static>(&mut self) {
        self.watch_component_type_entry(TypeId::of::<T>());
    }

    /// Track a subset of writes to a component type, before change records are
    /// merged or drained. All callers for a type must use the same filter.
    pub(crate) fn watch_filtered_component_revision<T: 'static>(
        &mut self,
        accepts: fn(ComponentChangeKind) -> bool,
    ) {
        let watched = self.watch_component_type_entry(TypeId::of::<T>());
        if let Some(filtered) = watched.filtered.as_ref() {
            debug_assert!(core::ptr::fn_addr_eq(filtered.accepts, accepts));
            return;
        }
        watched.filtered = Some(Box::new(FilteredComponentRevision {
            accepts,
            revision: watched.revision,
        }));
    }

    pub(crate) fn filtered_component_revision<T: 'static>(&self) -> Option<u64> {
        self.watched_component_types
            .iter()
            .find(|watched| watched.type_id == TypeId::of::<T>())?
            .filtered
            .as_ref()
            .map(|filtered| filtered.revision)
    }

    pub(crate) fn component_revision<T: 'static>(&self) -> u64 {
        self.tracked_component_revision::<T>().unwrap_or(0)
    }

    pub(crate) fn tracked_component_revision<T: 'static>(&self) -> Option<u64> {
        self.watched_component_types
            .iter()
            .find(|watched| watched.type_id == TypeId::of::<T>())
            .map(|watched| watched.revision)
    }

    pub(crate) fn component_membership_revision<T: 'static>(&self) -> u64 {
        self.watched_component_types
            .iter()
            .find(|watched| watched.type_id == TypeId::of::<T>())
            .map_or(0, |watched| watched.membership_revision)
    }

    pub(crate) fn component_membership_revision_by_id(
        &self,
        type_id: TypeId,
        hint: usize,
    ) -> (usize, u64) {
        if let Some(watched) = self.watched_component_types.get(hint)
            && watched.type_id == type_id
        {
            return (hint, watched.membership_revision);
        }
        self.watched_component_types
            .iter()
            .enumerate()
            .find(|(_, watched)| watched.type_id == type_id)
            .map_or((usize::MAX, 0), |(index, watched)| {
                (index, watched.membership_revision)
            })
    }

    fn record_membership_change(&mut self, type_id: TypeId) {
        if let Some(watched) = self
            .watched_component_types
            .iter_mut()
            .find(|watched| watched.type_id == type_id)
        {
            watched.membership_revision = watched.membership_revision.wrapping_add(1);
        }
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
                membership_revision: 0,
                filtered: None,
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
            ComponentChangeKind::Component,
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
            let watched = &self.watched_component_types[index];
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

    #[inline(never)]
    pub fn get<T: 'static>(&self, entity: Entity) -> Option<&T> {
        self.storage::<T>()?.get(entity)
    }

    /// Successful mutable access records a possible component change and marks query dependencies. Registered bindings reconcile the component's final value before layout and painting.
    pub fn get_mut<T: 'static>(&mut self, entity: Entity) -> Option<&mut T> {
        self.get_mut_with_change(entity, ComponentChangeKind::Component)
    }

    pub(crate) fn get_mut_with_change<T: 'static>(
        &mut self,
        entity: Entity,
        kind: ComponentChangeKind,
    ) -> Option<&mut T> {
        let type_id = TypeId::of::<T>();
        let component = component_storage_mut(&mut self.storages, type_id)?
            .as_any_mut()
            .downcast_mut::<SparseSet<T>>()
            .expect("component storage type")
            .get_mut(entity)?;
        record_component_change(
            &mut self.watched_component_types,
            &mut self.component_changes,
            entity,
            type_id,
            kind,
        );
        Some(component)
    }

    pub fn has<T: 'static>(&self, entity: Entity) -> bool {
        self.has_type(entity, TypeId::of::<T>())
    }

    pub fn has_type(&self, entity: Entity, type_id: TypeId) -> bool {
        component_storage(&self.storages, type_id).is_some_and(|s| s.contains_entity(entity))
    }

    /// True iff *any* live entity owns a component of `type_id`.
    /// Pairs with [`Self::has_type`] for per-entity checks.
    pub fn has_any_by_id(&self, type_id: TypeId) -> bool {
        component_storage(&self.storages, type_id).is_some_and(|s| !s.is_empty())
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
        component_storage(&self.storages, TypeId::of::<T>())
            .map(|s| s.as_any().downcast_ref::<SparseSet<T>>().unwrap())
    }

    fn storage_mut<T: 'static>(&mut self) -> &mut SparseSet<T> {
        component_storage_or_insert(&mut self.storages, TypeId::of::<T>(), || {
            Box::new(SparseSet::<T>::new())
        })
        .as_any_mut()
        .downcast_mut::<SparseSet<T>>()
        .unwrap()
    }

    fn storage_mut_if_exists<T: 'static>(&mut self) -> Option<&mut SparseSet<T>> {
        component_storage_mut(&mut self.storages, TypeId::of::<T>())
            .map(|storage| storage.as_any_mut().downcast_mut::<SparseSet<T>>().unwrap())
    }

    /// Register a resource, reusing its storage when the type is already present.
    pub fn insert_resource<T: 'static>(&mut self, value: T) {
        if let Some(existing) =
            resource_slot_mut(&mut self.resources, TypeId::of::<T>()).and_then(Option::as_mut)
        {
            let slot = existing
                .downcast_mut::<T>()
                .expect("resource type does not match its TypeId");
            drop(core::mem::replace(slot, value));
        } else {
            self.resources
                .insert(TypeId::of::<T>(), Some(Box::new(value)));
        }
    }

    pub fn resource<T: 'static>(&self) -> Option<&T> {
        resource_value(&self.resources, TypeId::of::<T>())?.downcast_ref::<T>()
    }

    pub fn resource_mut<T: 'static>(&mut self) -> Option<&mut T> {
        resource_slot_mut(&mut self.resources, TypeId::of::<T>())
            .and_then(Option::as_mut)
            .and_then(|v| v.downcast_mut::<T>())
    }

    pub fn remove_resource<T: 'static>(&mut self) -> Option<T> {
        self.resources
            .remove(&TypeId::of::<T>())
            .flatten()
            .and_then(|value| value.downcast::<T>().ok())
            .map(|value| *value)
    }

    pub(crate) fn take_resource_box<T: 'static>(&mut self) -> Option<Box<T>> {
        resource_slot_mut(&mut self.resources, TypeId::of::<T>())
            .and_then(Option::take)
            .and_then(|v| v.downcast::<T>().ok())
    }

    pub(crate) fn put_resource_box<T: 'static>(&mut self, value: Box<T>) {
        if let Some(slot) = resource_slot_mut(&mut self.resources, TypeId::of::<T>()) {
            drop(slot.replace(value));
        } else {
            self.resources.insert(TypeId::of::<T>(), Some(value));
        }
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
#[path = "../../tests/unit/ecs/world/tests.rs"]
mod tests;
