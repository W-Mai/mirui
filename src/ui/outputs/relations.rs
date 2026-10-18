use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::Cell;

use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};
use crate::ui::view::ViewInvalidation;

use super::{OutputDependency, OutputDescriptor};

#[derive(Default)]
pub(crate) struct OutputRelations {
    active_registry: Option<u64>,
    next_registry: u64,
    bindings: Vec<Binding>,
    demands: Vec<Demand>,
    descriptors: Vec<&'static OutputDescriptor>,
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
    }

    pub(crate) fn remove(&mut self, consumer: Entity, registration: u64) {
        for binding in &mut self.bindings {
            if binding.consumer == consumer && binding.registration == registration {
                binding.active = false;
            }
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
        self.propagate_sources(world, false);
    }

    fn reconcile_demands(&mut self, world: &mut World) {
        // Apply final aggregate counts before any producer releases storage.
        for demand in &mut self.demands {
            demand.current = 0;
            demand.history = 0;
        }
        for binding in &mut self.bindings {
            if !world.is_alive(binding.consumer) {
                binding.active = false;
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
    }

    fn propagate_invalidations(&mut self, world: &mut World) {
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
        for demand in &self.demands {
            if demand.current == 0 || !world.is_alive(demand.source) {
                continue;
            }
            let visible = !crate::ui::branch::is_hidden_in_tree(world, demand.source);
            let dirty = check_dirty && visible && source_is_dirty(world, demand.source);
            if dirty {
                (demand.descriptor.invalidate)(world, demand.source);
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
                propagated |= binding.update(world, visible, version, dirty);
            }
        }
        for binding in &mut self.bindings {
            if binding.active
                && !binding.dependency.retain_previous
                && world.is_alive(binding.consumer)
                && binding.source.is_none_or(|source| !world.is_alive(source))
            {
                propagated |= binding.update(world, false, None, false);
            }
        }
        propagated
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
        assert_eq!(counts.reads.each_ref().map(Cell::get), [2, 2]);
        assert_eq!(counts.invalidations.each_ref().map(Cell::get), [2, 2]);
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
