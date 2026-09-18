use alloc::rc::Rc;

use crate::ecs::{Entity, World};
use crate::types::Fixed;

use super::{ComputedRect, IdMap, Parent};

pub const LAYOUT_DEPENDENCY_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutAxis {
    Width,
    Height,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutSource {
    Entity(Entity),
    Named(&'static str),
    NearestContainer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutDependency {
    source: LayoutSource,
    axis: LayoutAxis,
}

impl LayoutDependency {
    pub const fn new(source: LayoutSource, axis: LayoutAxis) -> Self {
        Self { source, axis }
    }

    pub const fn entity(entity: Entity, axis: LayoutAxis) -> Self {
        Self::new(LayoutSource::Entity(entity), axis)
    }

    pub const fn container(axis: LayoutAxis) -> Self {
        Self::new(LayoutSource::NearestContainer, axis)
    }

    pub const fn named(name: &'static str, axis: LayoutAxis) -> Self {
        Self::new(LayoutSource::Named(name), axis)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayoutValues {
    values: [Fixed; LAYOUT_DEPENDENCY_CAPACITY],
    len: u8,
}

impl LayoutValues {
    const EMPTY: Self = Self {
        values: [Fixed::ZERO; LAYOUT_DEPENDENCY_CAPACITY],
        len: 0,
    };

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, index: usize) -> Fixed {
        assert!(index < self.len(), "layout value index out of bounds");
        self.values[index]
    }
}

type LayoutApplyFn = fn(&mut World, Entity, &LayoutValues) -> bool;
type SharedLayoutApply = Rc<dyn Fn(&mut World, Entity, &LayoutValues)>;

const INVALID_ENTITY: Entity = Entity {
    id: u32::MAX,
    generation: u32::MAX,
};

pub struct LayoutBinding {
    dependencies: &'static [LayoutDependency],
    resolved: [Entity; LAYOUT_DEPENDENCY_CAPACITY],
    values: LayoutValues,
    initialized: bool,
    apply: LayoutApplyFn,
}

impl LayoutBinding {
    pub fn new(dependencies: &'static [LayoutDependency], apply: LayoutApplyFn) -> Self {
        assert!(
            !dependencies.is_empty(),
            "layout binding requires at least one dependency"
        );
        assert!(
            dependencies.len() <= LAYOUT_DEPENDENCY_CAPACITY,
            "layout binding supports at most {LAYOUT_DEPENDENCY_CAPACITY} dependencies"
        );
        Self {
            dependencies,
            resolved: [INVALID_ENTITY; LAYOUT_DEPENDENCY_CAPACITY],
            values: LayoutValues {
                len: dependencies.len() as u8,
                ..LayoutValues::EMPTY
            },
            initialized: false,
            apply,
        }
    }

    fn dependency_count(&self) -> usize {
        self.values.len()
    }
}

pub struct SharedLayoutBinding {
    dependencies: [LayoutDependency; LAYOUT_DEPENDENCY_CAPACITY],
    resolved: [Entity; LAYOUT_DEPENDENCY_CAPACITY],
    values: LayoutValues,
    initialized: bool,
    apply: SharedLayoutApply,
}

impl SharedLayoutBinding {
    pub fn new(
        dependencies: &[LayoutDependency],
        apply: impl Fn(&mut World, Entity, &LayoutValues) + 'static,
    ) -> Self {
        assert!(
            !dependencies.is_empty(),
            "layout binding requires at least one dependency"
        );
        assert!(
            dependencies.len() <= LAYOUT_DEPENDENCY_CAPACITY,
            "layout binding supports at most {LAYOUT_DEPENDENCY_CAPACITY} dependencies"
        );
        let filler = LayoutDependency::entity(INVALID_ENTITY, LayoutAxis::Width);
        let mut stored = [filler; LAYOUT_DEPENDENCY_CAPACITY];
        stored[..dependencies.len()].copy_from_slice(dependencies);
        Self {
            dependencies: stored,
            resolved: [INVALID_ENTITY; LAYOUT_DEPENDENCY_CAPACITY],
            values: LayoutValues {
                len: dependencies.len() as u8,
                ..LayoutValues::EMPTY
            },
            initialized: false,
            apply: Rc::new(apply),
        }
    }

    fn dependency_count(&self) -> usize {
        self.values.len()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutContainer;

impl crate::ecs::Component for LayoutContainer {}

fn nearest_container(world: &World, entity: Entity) -> Option<Entity> {
    let mut current = world.get::<Parent>(entity).map(|parent| parent.0);
    while let Some(candidate) = current {
        if world.has::<LayoutContainer>(candidate) {
            return Some(candidate);
        }
        current = world.get::<Parent>(candidate).map(|parent| parent.0);
    }
    None
}

fn resolve_cached_source(
    world: &World,
    owner: Entity,
    source: LayoutSource,
    cached: Entity,
) -> Option<Entity> {
    match source {
        LayoutSource::Entity(entity) => (entity == cached && world.is_alive(cached))
            .then_some(cached)
            .or_else(|| world.is_alive(entity).then_some(entity)),
        LayoutSource::Named(name) => {
            let current = world.resource::<IdMap>()?.get(name)?;
            world.is_alive(current).then_some(current)
        }
        LayoutSource::NearestContainer => nearest_container(world, owner),
    }
}

pub(crate) fn apply_layout_bindings(world: &mut World, invoke: bool) -> bool {
    let mut any_changed = false;
    world.for_each_stable::<LayoutBinding>(|world, owner| {
        let (dependencies, mut resolved, count) = {
            let binding = world
                .get::<LayoutBinding>(owner)
                .expect("stable layout binding disappeared");
            (
                binding.dependencies,
                binding.resolved,
                binding.dependency_count(),
            )
        };

        let mut next = LayoutValues {
            len: count as u8,
            ..LayoutValues::EMPTY
        };
        for index in 0..count {
            let dependency = dependencies[index];
            let Some(entity) =
                resolve_cached_source(world, owner, dependency.source, resolved[index])
            else {
                return;
            };
            resolved[index] = entity;
            let Some(rect) = world.get::<ComputedRect>(entity).map(|computed| computed.0) else {
                return;
            };
            next.values[index] = match dependency.axis {
                LayoutAxis::Width => rect.w,
                LayoutAxis::Height => rect.h,
            };
        }

        let apply = {
            let binding = world
                .get_mut::<LayoutBinding>(owner)
                .expect("stable layout binding disappeared");
            binding.resolved = resolved;
            if binding.initialized && binding.values == next {
                None
            } else if !invoke {
                any_changed = true;
                None
            } else {
                binding.values = next;
                binding.initialized = true;
                Some(binding.apply)
            }
        };
        if let Some(apply) = apply {
            any_changed |= apply(world, owner, &next);
        }
    });
    world.for_each_stable::<SharedLayoutBinding>(|world, owner| {
        let (dependencies, mut resolved, count) = {
            let binding = world
                .get::<SharedLayoutBinding>(owner)
                .expect("stable shared layout binding disappeared");
            (
                binding.dependencies,
                binding.resolved,
                binding.dependency_count(),
            )
        };

        let mut next = LayoutValues {
            len: count as u8,
            ..LayoutValues::EMPTY
        };
        for index in 0..count {
            let dependency = dependencies[index];
            let Some(entity) =
                resolve_cached_source(world, owner, dependency.source, resolved[index])
            else {
                return;
            };
            resolved[index] = entity;
            let Some(rect) = world.get::<ComputedRect>(entity).map(|computed| computed.0) else {
                return;
            };
            next.values[index] = match dependency.axis {
                LayoutAxis::Width => rect.w,
                LayoutAxis::Height => rect.h,
            };
        }

        let apply = {
            let binding = world
                .get_mut::<SharedLayoutBinding>(owner)
                .expect("stable shared layout binding disappeared");
            binding.resolved = resolved;
            if binding.initialized && binding.values == next {
                None
            } else if !invoke {
                any_changed = true;
                None
            } else {
                binding.values = next;
                binding.initialized = true;
                Some(Rc::clone(&binding.apply))
            }
        };
        if let Some(apply) = apply {
            apply(world, owner, &next);
            any_changed = true;
        }
    });
    any_changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Dimension, Rect, Viewport};
    use crate::ui::layout::LayoutStyle;
    use crate::ui::{Children, Style, Widget};

    #[derive(Default)]
    struct Probe {
        calls: usize,
        last: Fixed,
    }

    fn widget(world: &mut World, width: i32, height: i32) -> Entity {
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        world.insert(
            entity,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(width),
                    height: Dimension::px(height),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        entity
    }

    fn record_width(world: &mut World, _: Entity, values: &LayoutValues) -> bool {
        let probe = world.resource_mut::<Probe>().unwrap();
        probe.calls += 1;
        probe.last = values.get(0);
        false
    }

    #[test]
    fn unchanged_layout_does_not_invoke_binding_again() {
        let mut world = World::new();
        world.insert_resource(Probe::default());
        let root = widget(&mut world, 320, 240);
        let target = widget(&mut world, 40, 20);
        world.insert(root, Children(vec![target]));
        world.insert(target, Parent(root));
        world.insert(
            target,
            SharedLayoutBinding::new(
                &[LayoutDependency::entity(root, LayoutAxis::Width)],
                |world, entity, values| {
                    let _ = record_width(world, entity, values);
                },
            ),
        );
        let viewport = Viewport::new(320, 240, Fixed::ONE);

        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(world.resource::<Probe>().unwrap().calls, 1);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(320)
        );

        world.get_mut::<Style>(root).unwrap().layout.width = Dimension::px(280);
        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(world.resource::<Probe>().unwrap().calls, 2);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(280)
        );
    }

    fn size_child(world: &mut World, target: Entity, values: &LayoutValues) -> bool {
        let width = Dimension::Px(values.get(0) / Fixed::from_int(2));
        crate::ui::property::apply_to_world::<crate::ui::property::prop::Width>(
            world, target, width,
        )
        .changed()
    }

    #[test]
    fn binding_changes_converge_in_the_same_update() {
        let mut world = World::new();
        let root = widget(&mut world, 300, 200);
        let target = widget(&mut world, 20, 20);
        world.insert(root, Children(vec![target]));
        world.insert(target, Parent(root));
        world.insert(
            target,
            SharedLayoutBinding::new(
                &[LayoutDependency::entity(root, LayoutAxis::Width)],
                |world, entity, values| {
                    let _ = size_child(world, entity, values);
                },
            ),
        );

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(300, 200, Fixed::ONE),
        );
        assert_eq!(
            world.get::<ComputedRect>(target).unwrap().0.w,
            Fixed::from_int(150)
        );
    }

    #[test]
    fn dirty_layout_reconciles_bindings_before_rendering() {
        let mut world = World::new();
        let root = widget(&mut world, 300, 200);
        let target = widget(&mut world, 20, 20);
        world.insert(root, Children(vec![target]));
        world.insert(target, Parent(root));
        world.insert(
            target,
            SharedLayoutBinding::new(
                &[LayoutDependency::entity(root, LayoutAxis::Width)],
                |world, entity, values| {
                    let _ = size_child(world, entity, values);
                },
            ),
        );
        world.insert(root, crate::ui::dirty::Dirty);
        world.insert(target, crate::ui::dirty::Dirty);

        let viewport = Viewport::new(300, 200, Fixed::ONE);
        let mut plan = crate::ui::dirty::DirtyRegions::default();
        crate::ui::render_system::collect_dirty_regions_into(
            &mut world, root, &viewport, &mut plan,
        );

        assert_eq!(
            world.get::<ComputedRect>(target).unwrap().0.w,
            Fixed::from_int(150)
        );
        assert!(!plan.is_empty());
    }

    fn oscillate(world: &mut World, target: Entity, _: &LayoutValues) -> bool {
        let probe = world.resource_mut::<Probe>().unwrap();
        probe.calls += 1;
        let next = if probe.calls % 2 == 0 { 100 } else { 200 };
        world.get_mut::<Style>(target).unwrap().layout.width = Dimension::px(next);
        true
    }

    #[test]
    #[should_panic(expected = "layout bindings did not settle")]
    fn cyclic_binding_is_reported() {
        let mut world = World::new();
        world.insert_resource(Probe::default());
        let root = widget(&mut world, 100, 100);
        world.insert(
            root,
            SharedLayoutBinding::new(
                &[LayoutDependency::entity(root, LayoutAxis::Width)],
                |world, entity, values| {
                    let _ = oscillate(world, entity, values);
                },
            ),
        );

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(300, 200, Fixed::ONE),
        );
    }

    #[test]
    fn dependency_chain_longer_than_three_passes_converges() {
        let mut world = World::new();
        let root = widget(&mut world, 320, 200);
        let first = widget(&mut world, 20, 20);
        let second = widget(&mut world, 20, 20);
        let third = widget(&mut world, 20, 20);
        let fourth = widget(&mut world, 20, 20);
        world.insert(root, Children(vec![first, second, third, fourth]));
        for child in [first, second, third, fourth] {
            world.insert(child, Parent(root));
        }
        for (target, source) in [
            (first, root),
            (second, first),
            (third, second),
            (fourth, third),
        ] {
            world.insert(
                target,
                SharedLayoutBinding::new(
                    &[LayoutDependency::entity(source, LayoutAxis::Width)],
                    |world, entity, values| {
                        let _ = size_child(world, entity, values);
                    },
                ),
            );
        }

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(320, 200, Fixed::ONE),
        );
        assert_eq!(
            world.get::<ComputedRect>(fourth).unwrap().0.w,
            Fixed::from_int(20)
        );
    }

    #[test]
    fn named_and_nearest_container_sources_resolve_once_available() {
        const DEPENDENCIES: &[LayoutDependency] = &[
            LayoutDependency::container(LayoutAxis::Width),
            LayoutDependency::named("stage", LayoutAxis::Height),
        ];
        let mut world = World::new();
        world.insert_resource(Probe::default());
        world.insert_resource(IdMap::new());
        let root = widget(&mut world, 320, 240);
        let container = widget(&mut world, 180, 120);
        let target = widget(&mut world, 40, 20);
        world.insert(root, Children(vec![container]));
        world.insert(container, Parent(root));
        world.insert(container, Children(vec![target]));
        world.insert(container, LayoutContainer);
        world.insert(target, Parent(container));
        world
            .resource_mut::<IdMap>()
            .unwrap()
            .insert("stage", container);
        world.insert(
            target,
            LayoutBinding::new(DEPENDENCIES, |world, _, values| {
                let probe = world.resource_mut::<Probe>().unwrap();
                probe.calls += 1;
                probe.last = values.get(0) + values.get(1);
                false
            }),
        );

        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &Viewport::new(320, 240, Fixed::ONE),
        );
        let probe = world.resource::<Probe>().unwrap();
        assert_eq!(probe.calls, 1);
        assert_eq!(probe.last, Fixed::from_int(300));
        assert_eq!(
            world.get::<ComputedRect>(target).unwrap().0,
            Rect::new(0, 0, 40, 20)
        );
    }

    #[test]
    fn named_source_follows_an_explicit_id_rebind() {
        const DEPENDENCIES: &[LayoutDependency] =
            &[LayoutDependency::named("stage", LayoutAxis::Width)];
        let mut world = World::new();
        world.insert_resource(Probe::default());
        world.insert_resource(IdMap::new());
        let root = widget(&mut world, 320, 240);
        let first = widget(&mut world, 120, 20);
        let second = widget(&mut world, 220, 20);
        let target = widget(&mut world, 40, 20);
        world.insert(root, Children(vec![first, second, target]));
        for child in [first, second, target] {
            world.insert(child, Parent(root));
        }
        world
            .resource_mut::<IdMap>()
            .unwrap()
            .insert("stage", first);
        world.insert(
            target,
            LayoutBinding::new(DEPENDENCIES, |world, _, values| {
                world.resource_mut::<Probe>().unwrap().last = values.get(0);
                false
            }),
        );
        let viewport = Viewport::new(320, 240, Fixed::ONE);

        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(120)
        );

        world.resource_mut::<IdMap>().unwrap().remove("stage");
        world
            .resource_mut::<IdMap>()
            .unwrap()
            .insert("stage", second);
        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(220)
        );
    }

    #[test]
    fn nearest_container_source_follows_reparenting() {
        const DEPENDENCIES: &[LayoutDependency] = &[LayoutDependency::container(LayoutAxis::Width)];
        let mut world = World::new();
        world.insert_resource(Probe::default());
        let root = widget(&mut world, 320, 240);
        let first = widget(&mut world, 120, 100);
        let second = widget(&mut world, 220, 100);
        let target = widget(&mut world, 40, 20);
        world.insert(root, Children(vec![first, second]));
        world.insert(first, Parent(root));
        world.insert(second, Parent(root));
        world.insert(first, Children(vec![target]));
        world.insert(second, Children(Vec::new()));
        world.insert(first, LayoutContainer);
        world.insert(second, LayoutContainer);
        world.insert(target, Parent(first));
        world.insert(
            target,
            LayoutBinding::new(DEPENDENCIES, |world, _, values| {
                world.resource_mut::<Probe>().unwrap().last = values.get(0);
                false
            }),
        );
        let viewport = Viewport::new(320, 240, Fixed::ONE);

        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(120)
        );

        world.get_mut::<Children>(first).unwrap().0.clear();
        world.get_mut::<Children>(second).unwrap().0.push(target);
        world.insert(target, Parent(second));
        crate::ui::render_system::update_layout(&mut world, root, &viewport);
        assert_eq!(
            world.resource::<Probe>().unwrap().last,
            Fixed::from_int(220)
        );
    }

    #[test]
    fn static_binding_keeps_the_common_component_compact() {
        assert!(core::mem::size_of::<LayoutBinding>() <= 96);
    }
}
