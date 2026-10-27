use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::Cell;

use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};
use crate::ui::view::ViewInvalidation;

use super::{OutputDependency, OutputDescriptor};

mod dirty_index;
use dirty_index::SourceDirtyIndex;

#[derive(Default)]
pub(crate) struct OutputRelations {
    active_registry: Option<u64>,
    next_registry: u64,
    structure_revision: u64,
    bindings: Vec<Binding>,
    demands: Vec<Demand>,
    descriptors: Vec<&'static OutputDescriptor>,
    dirty_index: SourceDirtyIndex,
}

struct Binding {
    consumer: Entity,
    registration: u64,
    slot: usize,
    source: Option<Entity>,
    dependency: &'static OutputDependency,
    invalidation: ViewInvalidation,
    active: bool,
    visible: bool,
    // Propagated or painted source version; pending paint remains on the consumer.
    version: Cell<Option<u64>>,
}

struct Demand {
    source: Entity,
    kind: TypeId,
    descriptor: &'static OutputDescriptor,
    current: usize,
    history: usize,
    requested: bool,
    propagated: bool,
}

impl Binding {
    fn update(
        &mut self,
        world: &mut World,
        visible: bool,
        version: Option<u64>,
        dirty: bool,
    ) -> bool {
        let changed = self.visible != visible || self.version.get() != version || dirty;
        self.visible = visible;
        if !changed {
            return false;
        }
        let propagated = self.invalidate(world);
        self.version.set(version);
        propagated
    }

    fn invalidate(&self, world: &mut World) -> bool {
        match self.invalidation {
            ViewInvalidation::Visual
                if !world
                    .get::<VisualDirty>(self.consumer)
                    .is_some_and(|dirty| dirty.content_pending() == VisualDirty::New) =>
            {
                world.invalidate_visual(self.consumer);
            }
            ViewInvalidation::Layout if !world.has::<Dirty>(self.consumer) => {
                world.invalidate(self.consumer);
            }
            _ => return false,
        }
        true
    }
}

impl OutputRelations {
    pub(crate) fn has_current_bindings(&self) -> bool {
        self.bindings
            .iter()
            .any(|binding| binding.active && !binding.dependency.retain_previous)
    }

    pub(crate) fn structure_revision(&self) -> u64 {
        self.structure_revision
    }

    fn changed_structure(&mut self) {
        self.structure_revision = self
            .structure_revision
            .checked_add(1)
            .expect("output relationship revision exhausted");
    }

    pub(crate) fn activate(&mut self, world: &mut World, registry: &mut Option<u64>) -> bool {
        if registry.is_some() && *registry == self.active_registry {
            return false;
        }
        self.clear(world);
        let identity = self.next_registry;
        self.next_registry = identity
            .checked_add(1)
            .expect("output registration identity exhausted");
        self.active_registry = Some(identity);
        *registry = Some(identity);
        true
    }

    pub(crate) fn clear(&mut self, world: &mut World) {
        self.trim_memory();
        let changed = self.bindings.iter().any(|binding| binding.active);
        for binding in &mut self.bindings {
            if binding.active {
                if binding.dependency.retain_previous {
                    (binding.dependency.descriptor.reset)(world, binding.consumer);
                }
                if world.is_alive(binding.consumer) {
                    binding.invalidate(world);
                }
            }
            binding.active = false;
        }
        self.active_registry = None;
        if changed {
            self.changed_structure();
        }
    }

    pub(crate) fn register(&mut self, descriptor: &'static OutputDescriptor) {
        let kind = (descriptor.kind)();
        if !self
            .descriptors
            .iter()
            .any(|existing| (existing.kind)() == kind)
        {
            self.descriptors.push(descriptor);
        }
    }

    pub(crate) fn bind(
        &mut self,
        world: &World,
        consumer: Entity,
        registration: u64,
        slot: usize,
        dependency: &'static OutputDependency,
        invalidation: ViewInvalidation,
    ) {
        let source = (dependency.source)(world, consumer);
        let existing = self.bindings.iter().position(|binding| {
            binding.active
                && binding.consumer == consumer
                && binding.registration == registration
                && binding.slot == slot
        });
        if let Some(index) = existing {
            if self.bindings[index].source != source {
                self.reset_history(world, consumer);
                let binding = &mut self.bindings[index];
                binding.source = source;
                binding.visible = false;
                binding.version.set(None);
                self.changed_structure();
            }
            return;
        }
        let binding = Binding {
            consumer,
            registration,
            slot,
            source,
            dependency,
            invalidation,
            active: true,
            visible: false,
            version: Cell::new(None),
        };
        if let Some(inactive) = self.bindings.iter_mut().find(|binding| !binding.active) {
            *inactive = binding;
        } else {
            self.bindings.push(binding);
        }
        self.changed_structure();
    }

    pub(crate) fn remove(&mut self, consumer: Entity, registration: u64) {
        let mut changed = false;
        for binding in &mut self.bindings {
            if binding.active
                && binding.consumer == consumer
                && binding.registration == registration
            {
                binding.active = false;
                changed = true;
            }
        }
        if changed {
            self.changed_structure();
        }
    }

    pub(crate) fn visit_current_dependencies(
        &self,
        mut visit: impl FnMut(Entity, Entity, &'static OutputDescriptor),
    ) {
        for binding in &self.bindings {
            if !binding.active || !binding.visible || binding.dependency.retain_previous {
                continue;
            }
            if let Some(source) = binding.source {
                visit(binding.consumer, source, &binding.dependency.descriptor);
            }
        }
    }

    pub(crate) fn acknowledge(&self, world: &World, consumer: Entity) {
        if !world.is_alive(consumer) {
            return;
        }
        for binding in &self.bindings {
            if !binding.active || binding.consumer != consumer || binding.dependency.retain_previous
            {
                continue;
            }
            let version = binding
                .source
                .filter(|&source| world.is_alive(source))
                .filter(|&source| !crate::ui::branch::is_hidden_in_tree(world, source))
                .and_then(|source| (binding.dependency.descriptor.version)(world, source));
            binding.version.set(version);
        }
    }

    fn reset_history(&self, world: &World, consumer: Entity) {
        for binding in &self.bindings {
            if binding.active && binding.consumer == consumer && binding.dependency.retain_previous
            {
                (binding.dependency.descriptor.reset)(world, consumer);
            }
        }
    }

    pub(crate) fn reconcile(&mut self, world: &mut World) {
        self.reconcile_demands(world);
        self.propagate_invalidations(world);
    }

    pub(crate) fn propagate_publications(&mut self, world: &mut World) {
        if self.bindings.is_empty() {
            return;
        }
        self.propagate_sources(world, false);
    }

    fn reconcile_demands(&mut self, world: &mut World) {
        let mut changed = false;
        // Apply final aggregate counts before any producer releases storage.
        for demand in &mut self.demands {
            demand.current = 0;
            demand.history = 0;
        }
        for binding in &mut self.bindings {
            if binding.active && !world.is_alive(binding.consumer) {
                binding.active = false;
                changed = true;
            }
            if !binding.active {
                continue;
            }
            let Some(source) = binding.source.filter(|&source| world.is_alive(source)) else {
                continue;
            };
            let descriptor = &binding.dependency.descriptor;
            let kind = (descriptor.kind)();
            let index = self
                .demands
                .iter()
                .position(|demand| demand.source == source && demand.kind == kind)
                .unwrap_or_else(|| {
                    self.demands.push(Demand {
                        source,
                        kind,
                        descriptor,
                        current: 0,
                        history: 0,
                        requested: false,
                        propagated: false,
                    });
                    self.demands.len() - 1
                });
            let demand = &mut self.demands[index];
            if binding.dependency.retain_previous {
                demand.history += 1;
            } else {
                demand.current += 1;
            }
        }
        for demand in &mut self.demands {
            (demand.descriptor.set_demand)(world, demand.source, demand.current, demand.history);
            let requested = demand.current != 0 || demand.history != 0;
            changed |= requested != demand.requested;
            if requested != demand.requested && world.is_alive(demand.source) {
                world.invalidate_visual(demand.source);
            }
            demand.requested = requested;
        }
        self.demands
            .retain(|demand| demand.current != 0 || demand.history != 0);
        for descriptor in &self.descriptors {
            (descriptor.reconcile)(world);
        }
        self.prepare_dirty_sources(world);
        if changed {
            self.changed_structure();
        }
    }

    fn prepare_dirty_sources(&mut self, world: &mut World) {
        self.dirty_index.begin(
            world,
            self.demands
                .iter()
                .filter_map(|demand| (demand.current != 0).then_some(demand.source)),
        );
    }

    fn propagate_invalidations(&mut self, world: &mut World) {
        if self.bindings.is_empty() {
            return;
        }
        for demand in &mut self.demands {
            demand.propagated = false;
        }
        // Marker propagation is bounded by the number of current relationships.
        // History retention never introduces a current dependency edge.
        for _ in 0..=self.bindings.len() {
            if !self.propagate_sources(world, true) {
                break;
            }
        }
    }

    fn propagate_sources(&mut self, world: &mut World, check_dirty: bool) -> bool {
        let mut propagated = false;
        let mut changed = false;
        let mut summary_ready = !check_dirty;
        for demand in &mut self.demands {
            if demand.current == 0
                || !world.is_alive(demand.source)
                || (check_dirty && demand.propagated)
            {
                continue;
            }
            if !summary_ready {
                // Only prepare a summary when a live source will read it. Each
                // pass still sees the preceding pass's writes, including chains.
                self.dirty_index.refresh(world);
                summary_ready = true;
            }
            let visible = !crate::ui::branch::is_hidden_in_tree(world, demand.source);
            let dirty = check_dirty && visible && self.dirty_index.get(world, demand.source);
            if dirty {
                (demand.descriptor.invalidate)(world, demand.source);
                demand.propagated = true;
            }
            let version = visible
                .then(|| (demand.descriptor.version)(world, demand.source))
                .flatten();
            for binding in &mut self.bindings {
                if !binding.active
                    || binding.dependency.retain_previous
                    || binding.source != Some(demand.source)
                    || (binding.dependency.descriptor.kind)() != demand.kind
                    || !world.is_alive(binding.consumer)
                {
                    continue;
                }
                changed |= binding.visible != visible;
                propagated |= binding.update(world, visible, version, dirty);
            }
        }
        for binding in &mut self.bindings {
            if binding.active
                && !binding.dependency.retain_previous
                && world.is_alive(binding.consumer)
                && binding.source.is_none_or(|source| !world.is_alive(source))
            {
                changed |= binding.visible;
                propagated |= binding.update(world, false, None, false);
            }
        }
        if changed {
            self.changed_structure();
        }
        propagated
    }

    pub(crate) fn trim_memory(&mut self) {
        self.dirty_index = SourceDirtyIndex::default();
    }
}

fn source_is_dirty(world: &World, source: Entity) -> bool {
    fn contains<T: 'static>(world: &World, source: Entity) -> bool {
        world.storage::<T>().is_some_and(|storage| {
            storage.entities().iter().copied().any(|entity| {
                crate::ui::branch::is_in_subtree(world, source, entity)
                    && !crate::ui::branch::is_hidden_in_tree(world, entity)
            })
        })
    }
    contains::<Dirty>(world, source)
        || world.storage::<VisualDirty>().is_some_and(|storage| {
            storage.iter().any(|(entity, state)| {
                (if entity == source {
                    state.content_pending()
                } else {
                    state.parent_pending()
                }) == VisualDirty::New
                    && crate::ui::branch::is_in_subtree(world, source, entity)
                    && !crate::ui::branch::is_hidden_in_tree(world, entity)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_content_excludes_own_motion_but_includes_descendant_motion() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, crate::ui::Parent(source));
        world.invalidate_transform(source);
        assert!(!source_is_dirty(&world, source));
        world.invalidate_transform(child);
        assert!(source_is_dirty(&world, source));
        VisualDirty::prepare(&mut world, child);
        assert!(!source_is_dirty(&world, source));
        world.invalidate_visual(source);
        assert!(source_is_dirty(&world, source));
        assert!(
            world
                .get::<VisualDirty>(source)
                .unwrap()
                .transforms_subtree()
        );
    }

    const CURRENT: OutputDependency = OutputDependency {
        descriptor: OutputDescriptor {
            kind: TypeId::of::<u64>,
            install: |_| {},
            set_demand: |_, _, _, _| {},
            invalidate: |_, _| {},
            reset: |_, _| {},
            version: |world, source| world.get::<u64>(source).copied(),
            reconcile: |_| {},
        },
        source: |world, consumer| world.get::<Entity>(consumer).copied(),
        retain_previous: false,
    };

    const HISTORY: OutputDependency = OutputDependency {
        retain_previous: true,
        ..CURRENT
    };

    #[test]
    fn producer_maintenance_survives_intervals_without_consumers() {
        const OUTPUT: OutputDependency = OutputDependency {
            descriptor: OutputDescriptor {
                reconcile: |world| {
                    let calls = world.resource::<Cell<usize>>().unwrap();
                    calls.set(calls.get() + 1);
                },
                ..CURRENT.descriptor
            },
            ..CURRENT
        };
        let mut world = World::new();
        world.insert_resource(Cell::new(0_usize));
        let mut relations = OutputRelations::default();
        relations.register(&OUTPUT.descriptor);
        relations.reconcile(&mut world);
        relations.propagate_publications(&mut world);
        assert_eq!(world.resource::<Cell<usize>>().unwrap().get(), 1);

        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(source, 42_u64);
        world.insert(consumer, source);
        relations.bind(&world, consumer, 0, 0, &OUTPUT, ViewInvalidation::Visual);
        relations.reconcile(&mut world);
        assert_eq!(relations.bindings[0].version.get(), Some(42));
        assert!(world.has::<VisualDirty>(consumer));
        world.despawn(consumer);
        relations.reconcile(&mut world);
        assert!(relations.demands.is_empty());
        assert_eq!(world.resource::<Cell<usize>>().unwrap().get(), 3);
    }

    #[test]
    fn exhausted_sources_do_not_prepare_an_unused_summary() {
        for chained in [false, true] {
            let mut world = World::new();
            let source = world.spawn_empty();
            let middle = world.spawn_empty();
            let last = world.spawn_empty();
            let mut relations = OutputRelations::default();
            if chained {
                world.insert(last, middle);
                // Visit the downstream source before upstream damage reaches it.
                relations.bind(&world, last, 0, 0, &CURRENT, ViewInvalidation::Layout);
            }
            world.insert(middle, source);
            relations.bind(&world, middle, 0, 0, &CURRENT, ViewInvalidation::Visual);
            relations.reconcile(&mut world);
            for entity in [source, middle, last] {
                world.remove::<Dirty>(entity);
                world.remove::<VisualDirty>(entity);
            }
            let rebuilds = relations.dirty_index.rebuilds;
            world.invalidate_visual(source);
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    relations.propagate_invalidations(&mut world);
                }),
                0
            );
            assert!(world.has::<VisualDirty>(middle));
            assert_eq!(world.has::<Dirty>(last), chained);
            assert_eq!(
                relations.dirty_index.rebuilds - rebuilds,
                1 + usize::from(chained)
            );

            // A later call must still observe changed markers in the same frame.
            for entity in [source, middle, last] {
                world.remove::<Dirty>(entity);
                world.remove::<VisualDirty>(entity);
            }
            relations.propagate_invalidations(&mut world);
            assert!(!world.has::<VisualDirty>(middle));
            assert!(!world.has::<Dirty>(last));
            world.invalidate_visual(source);
            relations.propagate_invalidations(&mut world);
            assert!(world.has::<VisualDirty>(middle));
            assert_eq!(world.has::<Dirty>(last), chained);
        }
    }

    #[test]
    fn repeated_reconciliation_keeps_the_summary_and_batches_new_damage() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let middle = world.spawn_empty();
        let last = world.spawn_empty();
        let mut relations = OutputRelations::default();
        for (consumer, source) in [(last, middle), (middle, source)] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, &CURRENT, ViewInvalidation::Visual);
        }
        relations.reconcile(&mut world);
        // Settle the final consumer's damage before checking stable reuse.
        relations.reconcile(&mut world);
        let count = relations.dirty_index.rebuilds;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..73 {
                    relations.reconcile(&mut world);
                }
            }),
            0
        );
        assert_eq!(relations.dirty_index.rebuilds, count);
        for entity in [source, middle, last] {
            world.remove::<VisualDirty>(entity);
        }
        relations.reconcile(&mut world);
        assert!(!world.has::<VisualDirty>(last));
        world.invalidate_visual(source);
        relations.reconcile(&mut world);
        assert!(world.has::<VisualDirty>(middle));
        assert!(world.has::<VisualDirty>(last));
        assert!(relations.dirty_index.rebuilds > count);
    }

    #[test]
    fn output_changes_preserve_each_consumers_view_invalidation() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let visual = world.spawn_empty();
        let layout = world.spawn_empty();
        world.insert(source, 1_u64);
        let mut relations = OutputRelations::default();
        for (consumer, invalidation) in [
            (visual, ViewInvalidation::Visual),
            (layout, ViewInvalidation::Layout),
        ] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, &CURRENT, invalidation);
        }
        relations.reconcile_demands(&mut world);
        world.remove::<VisualDirty>(source);

        for stage in 0..4 {
            match stage {
                0 => relations.propagate_publications(&mut world),
                1 => {
                    world.insert(source, 2_u64);
                    world.insert(layout, VisualDirty::New);
                    relations.propagate_publications(&mut world);
                }
                2 => {
                    world.insert(source, Dirty);
                    relations.propagate_invalidations(&mut world);
                }
                _ => relations.clear(&mut world),
            }
            assert!(world.has::<VisualDirty>(visual));
            assert!(!world.has::<Dirty>(visual));
            assert!(world.has::<Dirty>(layout), "stage {stage}");
            for consumer in [visual, layout] {
                relations.acknowledge(&world, consumer);
                world.remove::<Dirty>(consumer);
                world.remove::<VisualDirty>(consumer);
            }
        }
    }

    #[test]
    fn mixed_output_dependency_chains_propagate_without_allocating() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let layout = world.spawn_empty();
        let visual = world.spawn_empty();
        let mut relations = OutputRelations::default();
        world.insert(layout, source);
        world.insert(visual, layout);
        // Bind the downstream reader first to require transitive propagation.
        relations.bind(&world, visual, 0, 0, &CURRENT, ViewInvalidation::Visual);
        relations.bind(&world, layout, 0, 0, &CURRENT, ViewInvalidation::Layout);
        relations.reconcile_demands(&mut world);
        world.remove::<VisualDirty>(source);
        relations.propagate_invalidations(&mut world);
        for consumer in [layout, visual] {
            relations.acknowledge(&world, consumer);
            world.remove::<Dirty>(consumer);
            world.remove::<VisualDirty>(consumer);
        }
        world.insert(source, VisualDirty::New);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_invalidations(&mut world);
            }),
            0
        );
        assert!(world.has::<Dirty>(layout));
        assert!(world.has::<VisualDirty>(visual));
        assert!(!world.has::<Dirty>(visual));
    }

    #[test]
    fn trimmed_dirty_summary_falls_back_until_demands_are_prepared_again() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, source);
        let mut relations = OutputRelations::default();
        relations.bind(&world, consumer, 0, 0, &CURRENT, ViewInvalidation::Visual);
        relations.reconcile(&mut world);
        world.remove::<VisualDirty>(consumer);
        relations.trim_memory();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_invalidations(&mut world);
            }),
            0
        );
        assert!(world.has::<VisualDirty>(consumer));
        relations.reconcile_demands(&mut world);
        world.remove::<VisualDirty>(consumer);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_invalidations(&mut world);
            }),
            0
        );
        assert!(world.has::<VisualDirty>(consumer));
    }

    #[test]
    fn publication_damage_requires_each_consumers_acknowledgement() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(source, 7_u64);
        let mut relations = OutputRelations::default();
        for consumer in [first, second] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, &CURRENT, ViewInvalidation::Visual);
        }
        relations.reconcile_demands(&mut world);
        world.remove::<VisualDirty>(source);

        relations.propagate_invalidations(&mut world);
        assert!(
            relations
                .bindings
                .iter()
                .all(|binding| binding.version.get() == Some(7))
        );
        VisualDirty::prepare(&mut world, first);
        VisualDirty::prepare(&mut world, second);

        relations.acknowledge(&world, first);
        VisualDirty::acknowledge(&mut world, first);
        relations.propagate_invalidations(&mut world);
        assert!(!world.has::<VisualDirty>(first));
        assert!(matches!(
            world.get::<VisualDirty>(second),
            Some(VisualDirty::Prepared)
        ));
        assert_eq!(relations.bindings[0].version.get(), Some(7));
        assert_eq!(relations.bindings[1].version.get(), Some(7));

        world.insert(source, 8_u64);
        relations.propagate_invalidations(&mut world);
        assert!(world.has::<VisualDirty>(first));
        assert!(matches!(
            world.get::<VisualDirty>(second),
            Some(VisualDirty::New)
        ));
        assert_eq!(relations.bindings[0].version.get(), Some(8));

        world.insert(source, crate::ui::Hidden);
        relations.acknowledge(&world, first);
        assert_eq!(relations.bindings[0].version.get(), None);
    }

    #[test]
    fn publication_propagation_preserves_producers_and_requires_successful_consumption() {
        const PUBLISHED: OutputDependency = OutputDependency {
            descriptor: OutputDescriptor {
                kind: TypeId::of::<Cell<Option<u64>>>,
                invalidate: |world, source| {
                    if let Some(version) = world.get::<Cell<Option<u64>>>(source) {
                        version.set(None);
                    }
                },
                version: |world, source| world.get::<Cell<Option<u64>>>(source).and_then(Cell::get),
                ..CURRENT.descriptor
            },
            ..CURRENT
        };
        const RETAINED: OutputDependency = OutputDependency {
            retain_previous: true,
            ..PUBLISHED
        };
        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        let retained = world.spawn_empty();
        world.insert(source, Cell::new(Some(1_u64)));
        world.insert(source, Dirty);
        let mut relations = OutputRelations::default();
        for (consumer, dependency) in [
            (first, &PUBLISHED),
            (second, &PUBLISHED),
            (retained, &RETAINED),
        ] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, dependency, ViewInvalidation::Visual);
        }
        relations.reconcile_demands(&mut world);

        relations.propagate_publications(&mut world);
        assert_eq!(
            world.get::<Cell<Option<u64>>>(source).unwrap().get(),
            Some(1)
        );
        assert!(world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));
        assert!(!world.has::<VisualDirty>(retained));
        assert_eq!(relations.bindings[0].version.get(), Some(1));
        assert_eq!(relations.bindings[1].version.get(), Some(1));
        assert_eq!(relations.bindings[2].version.get(), None);
        for consumer in [first, second] {
            relations.acknowledge(&world, consumer);
            world.remove::<VisualDirty>(consumer);
        }

        world.get::<Cell<Option<u64>>>(source).unwrap().set(Some(2));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_publications(&mut world);
            }),
            0
        );
        assert_eq!(
            world.get::<Cell<Option<u64>>>(source).unwrap().get(),
            Some(2)
        );
        assert_eq!(relations.bindings[0].version.get(), Some(2));
        assert_eq!(relations.bindings[1].version.get(), Some(2));
        assert!(world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));
        assert!(!world.has::<VisualDirty>(retained));

        relations.acknowledge(&world, first);
        world.remove::<VisualDirty>(first);
        relations.propagate_publications(&mut world);
        assert!(!world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));
        relations.acknowledge(&world, second);
        world.remove::<VisualDirty>(second);

        world.get::<Cell<Option<u64>>>(source).unwrap().set(None);
        relations.propagate_publications(&mut world);
        for consumer in [first, second] {
            assert!(world.has::<VisualDirty>(consumer));
            relations.acknowledge(&world, consumer);
            world.remove::<VisualDirty>(consumer);
        }
        world.get::<Cell<Option<u64>>>(source).unwrap().set(Some(2));
        relations.propagate_publications(&mut world);
        assert!(world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));
        assert!(!world.has::<VisualDirty>(retained));
    }

    #[test]
    fn current_dependency_visitor_excludes_inactive_hidden_and_history_bindings() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let consumer = world.spawn_empty();
        world.insert(consumer, source);
        let mut relations = OutputRelations::default();
        for (slot, dependency) in [&CURRENT, &CURRENT, &CURRENT, &HISTORY]
            .into_iter()
            .enumerate()
        {
            relations.bind(
                &world,
                consumer,
                0,
                slot,
                dependency,
                ViewInvalidation::Visual,
            );
        }
        relations.reconcile_demands(&mut world);
        relations.propagate_invalidations(&mut world);
        relations.bindings[1].visible = false;
        relations.bindings[2].active = false;
        relations.bindings[3].visible = true;

        let mut count = 0;
        relations.visit_current_dependencies(|actual_consumer, actual_source, descriptor| {
            assert_eq!(actual_consumer, consumer);
            assert_eq!(actual_source, source);
            assert_eq!((descriptor.kind)(), TypeId::of::<u64>());
            count += 1;
        });
        assert_eq!(count, 1);
    }

    #[derive(Default)]
    struct SourceReads {
        versions: [Cell<Option<u64>>; 2],
        reads: [Cell<usize>; 2],
        invalidations: [Cell<usize>; 2],
    }

    impl SourceReads {
        fn version(&self, kind: usize) -> Option<u64> {
            self.reads[kind].set(self.reads[kind].get() + 1);
            self.versions[kind].get()
        }

        fn invalidate(&self, kind: usize) {
            self.invalidations[kind].set(self.invalidations[kind].get() + 1);
            self.versions[kind].set(None);
        }

        fn clear_counts(&self) {
            for counter in self.reads.iter().chain(&self.invalidations) {
                counter.set(0);
            }
        }
    }

    const COUNTED_FIRST: OutputDependency = OutputDependency {
        descriptor: OutputDescriptor {
            invalidate: |world, source| world.get::<SourceReads>(source).unwrap().invalidate(0),
            version: |world, source| world.get::<SourceReads>(source).unwrap().version(0),
            ..CURRENT.descriptor
        },
        ..CURRENT
    };

    const COUNTED_SECOND: OutputDependency = OutputDependency {
        descriptor: OutputDescriptor {
            kind: TypeId::of::<u32>,
            invalidate: |world, source| world.get::<SourceReads>(source).unwrap().invalidate(1),
            version: |world, source| world.get::<SourceReads>(source).unwrap().version(1),
            ..CURRENT.descriptor
        },
        ..CURRENT
    };

    const COUNTED_HISTORY: OutputDependency = OutputDependency {
        retain_previous: true,
        ..COUNTED_FIRST
    };

    #[test]
    fn source_checks_are_shared_by_current_demand_not_consumer_or_history_count() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let history_source = world.spawn_empty();
        world.insert(source, SourceReads::default());
        world.insert(history_source, SourceReads::default());
        world.get::<SourceReads>(source).unwrap().versions[0].set(Some(7));
        world.get::<SourceReads>(source).unwrap().versions[1].set(Some(19));
        let consumers: [Entity; 12] = core::array::from_fn(|_| world.spawn_empty());
        let mut relations = OutputRelations::default();
        for (index, consumer) in consumers.into_iter().enumerate() {
            let (source, dependency) = match index {
                0..8 => (source, &COUNTED_FIRST),
                8..11 => (source, &COUNTED_SECOND),
                _ => (history_source, &COUNTED_HISTORY),
            };
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, dependency, ViewInvalidation::Visual);
        }
        relations.reconcile_demands(&mut world);
        assert_eq!(relations.demands.len(), 3);
        for source in [source, history_source] {
            world.remove::<VisualDirty>(source);
        }
        relations.propagate_publications(&mut world);
        for (index, binding) in relations.bindings.iter().enumerate() {
            assert_eq!(
                binding.version.get(),
                match index {
                    0..8 => Some(7),
                    8..11 => Some(19),
                    _ => None,
                }
            );
        }
        for consumer in consumers {
            VisualDirty::prepare(&mut world, consumer);
            relations.acknowledge(&world, consumer);
            VisualDirty::acknowledge(&mut world, consumer);
        }
        world.get::<SourceReads>(source).unwrap().clear_counts();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_publications(&mut world);
            }),
            0
        );
        let counts = world.get::<SourceReads>(source).unwrap();
        assert_eq!(counts.reads.each_ref().map(Cell::get), [1, 1]);
        assert_eq!(counts.invalidations.each_ref().map(Cell::get), [0, 0]);
        counts.clear_counts();
        world.invalidate_visual(source);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                relations.propagate_invalidations(&mut world);
            }),
            0
        );
        let counts = world.get::<SourceReads>(source).unwrap();
        assert_eq!(counts.reads.each_ref().map(Cell::get), [1, 1]);
        assert_eq!(counts.invalidations.each_ref().map(Cell::get), [1, 1]);
        assert!(
            consumers[..11]
                .iter()
                .all(|&entity| world.has::<VisualDirty>(entity))
        );
        assert!(!world.has::<VisualDirty>(consumers[11]));
        let history = world.get::<SourceReads>(history_source).unwrap();
        assert_eq!(history.reads.each_ref().map(Cell::get), [0, 0]);
        assert_eq!(history.invalidations.each_ref().map(Cell::get), [0, 0]);
    }

    #[test]
    fn reverse_diamond_dependencies_propagate_again_after_preparation_without_allocating() {
        struct SecondSource(Entity);
        const SECOND_EDGE: OutputDependency = OutputDependency {
            source: |world, consumer| world.get::<SecondSource>(consumer).map(|source| source.0),
            ..CURRENT
        };
        let mut world = World::default();
        let source = world.spawn_empty();
        let left = world.spawn_empty();
        let right = world.spawn_empty();
        let last = world.spawn_empty();
        world.insert(last, left);
        world.insert(last, SecondSource(right));
        world.insert(left, source);
        world.insert(right, source);
        let mut relations = OutputRelations::default();
        for (consumer, slot, dependency, invalidation) in [
            (last, 0, &CURRENT, ViewInvalidation::Visual),
            (last, 1, &SECOND_EDGE, ViewInvalidation::Visual),
            (left, 0, &CURRENT, ViewInvalidation::Layout),
            (right, 0, &CURRENT, ViewInvalidation::Visual),
        ] {
            relations.bind(&world, consumer, 0, slot, dependency, invalidation);
        }
        relations.reconcile(&mut world);
        for entity in [source, left, right, last] {
            VisualDirty::prepare(&mut world, entity);
            relations.acknowledge(&world, entity);
            VisualDirty::acknowledge(&mut world, entity);
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..2 {
                    world.invalidate_visual(source);
                    relations.propagate_invalidations(&mut world);
                    assert!(world.has::<Dirty>(left));
                    assert!(world.has::<VisualDirty>(right));
                    assert!(world.has::<VisualDirty>(last));
                    for entity in [source, left, right, last] {
                        VisualDirty::prepare(&mut world, entity);
                    }
                    relations.propagate_invalidations(&mut world);
                    assert!(!world.has::<Dirty>(left));
                    for entity in [source, left, right, last] {
                        assert!(matches!(
                            world.get::<VisualDirty>(entity),
                            Some(VisualDirty::Prepared)
                        ));
                        relations.acknowledge(&world, entity);
                        VisualDirty::acknowledge(&mut world, entity);
                    }
                }
            }),
            0
        );
    }

    #[test]
    fn grouped_sources_preserve_hidden_damage_and_reject_reused_entity_slots() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(source, 1_u64);
        let mut relations = OutputRelations::default();
        for consumer in [first, second] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, &CURRENT, ViewInvalidation::Visual);
        }
        relations.reconcile(&mut world);
        for entity in [source, first, second] {
            VisualDirty::prepare(&mut world, entity);
            relations.acknowledge(&world, entity);
            VisualDirty::acknowledge(&mut world, entity);
        }

        world.insert(source, crate::ui::Hidden);
        world.invalidate_visual(source);
        relations.propagate_invalidations(&mut world);
        assert!(matches!(
            world.get::<VisualDirty>(source),
            Some(VisualDirty::New)
        ));
        for consumer in [first, second] {
            assert!(world.has::<VisualDirty>(consumer));
            VisualDirty::prepare(&mut world, consumer);
            relations.acknowledge(&world, consumer);
            VisualDirty::acknowledge(&mut world, consumer);
        }
        assert!(
            relations
                .bindings
                .iter()
                .all(|binding| !binding.visible && binding.version.get().is_none())
        );

        world.remove::<crate::ui::Hidden>(source);
        relations.propagate_invalidations(&mut world);
        assert!(
            relations
                .bindings
                .iter()
                .all(|binding| binding.visible && binding.version.get() == Some(1))
        );
        for entity in [source, first, second] {
            VisualDirty::prepare(&mut world, entity);
            relations.acknowledge(&world, entity);
            VisualDirty::acknowledge(&mut world, entity);
        }

        assert!(world.despawn(source));
        let replacement = world.spawn_empty();
        assert_eq!(replacement.id, source.id);
        assert_ne!(replacement.generation, source.generation);
        world.insert(replacement, 99_u64);
        relations.propagate_publications(&mut world);
        assert!(
            relations
                .bindings
                .iter()
                .all(|binding| !binding.visible && binding.version.get().is_none())
        );
        assert!(world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));

        world.insert(first, replacement);
        relations.bind(&world, first, 0, 0, &CURRENT, ViewInvalidation::Visual);
        relations.reconcile_demands(&mut world);
        world.remove::<VisualDirty>(replacement);
        relations.propagate_publications(&mut world);
        assert_eq!(relations.bindings[0].version.get(), Some(99));
        assert_eq!(relations.bindings[1].version.get(), None);
        assert!(!relations.bindings[1].visible);

        assert!(world.despawn(first));
        relations.propagate_publications(&mut world);
        assert!(!world.has::<VisualDirty>(first));
        relations.reconcile(&mut world);
        assert!(!relations.bindings[0].active);
        assert!(relations.demands.is_empty());
    }

    #[test]
    fn relationship_metadata_borrows_static_descriptors() {
        assert!(core::mem::size_of::<Binding>() <= 64);
        assert!(core::mem::size_of::<Demand>() <= 64);
        std::println!(
            "output metadata: binding={} bytes, source={} bytes, kind={} bytes",
            core::mem::size_of::<Binding>(),
            core::mem::size_of::<Demand>(),
            core::mem::size_of::<&OutputDescriptor>(),
        );
    }
}
