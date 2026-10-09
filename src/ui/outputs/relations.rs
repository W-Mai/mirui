use alloc::vec::Vec;
use core::any::TypeId;
use core::cell::Cell;

use crate::ecs::{Entity, World};
use crate::ui::dirty::{Dirty, VisualDirty};

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
    active: bool,
    visible: bool,
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
                    world.invalidate_visual(binding.consumer);
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
            let mut propagated = false;
            for binding in &mut self.bindings {
                if !binding.active || binding.dependency.retain_previous {
                    continue;
                }
                let source = binding.source.filter(|&source| world.is_alive(source));
                let visible = source
                    .is_some_and(|source| !crate::ui::branch::is_hidden_in_tree(world, source));
                let dirty = visible && source.is_some_and(|source| source_is_dirty(world, source));
                if dirty {
                    (binding.dependency.descriptor.invalidate)(world, source.expect("live source"));
                }
                let version = source
                    .filter(|_| visible)
                    .and_then(|source| (binding.dependency.descriptor.version)(world, source));
                let changed =
                    binding.visible != visible || binding.version.get() != version || dirty;
                binding.visible = visible;
                if changed && !world.has::<VisualDirty>(binding.consumer) {
                    world.invalidate_visual(binding.consumer);
                    propagated = true;
                }
            }
            if !propagated {
                break;
            }
        }
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
    contains::<Dirty>(world, source) || contains::<VisualDirty>(world, source)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn publication_versions_require_each_consumers_acknowledgement() {
        let mut world = World::default();
        let source = world.spawn_empty();
        let first = world.spawn_empty();
        let second = world.spawn_empty();
        world.insert(source, 7_u64);
        let mut relations = OutputRelations::default();
        for consumer in [first, second] {
            world.insert(consumer, source);
            relations.bind(&world, consumer, 0, 0, &CURRENT);
        }

        relations.propagate_invalidations(&mut world);
        assert!(
            relations
                .bindings
                .iter()
                .all(|binding| binding.version.get().is_none())
        );
        world.remove::<VisualDirty>(first);
        world.remove::<VisualDirty>(second);

        relations.acknowledge(&world, first);
        relations.propagate_invalidations(&mut world);
        assert!(!world.has::<VisualDirty>(first));
        assert!(world.has::<VisualDirty>(second));
        assert_eq!(relations.bindings[0].version.get(), Some(7));
        assert_eq!(relations.bindings[1].version.get(), None);

        world.insert(source, 8_u64);
        relations.propagate_invalidations(&mut world);
        assert!(world.has::<VisualDirty>(first));
        assert_eq!(relations.bindings[0].version.get(), Some(7));

        world.insert(source, crate::ui::Hidden);
        relations.acknowledge(&world, first);
        assert_eq!(relations.bindings[0].version.get(), None);
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
            relations.bind(&world, consumer, 0, slot, dependency);
        }
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
