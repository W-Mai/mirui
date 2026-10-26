use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::ui::Parent;
use crate::ui::dirty::{Dirty, VisualDirty};

#[derive(Default)]
pub(super) struct SourceDirtyIndex {
    entries: Vec<Entry>,
    revisions: Option<[u64; 6]>,
    available: bool,
    #[cfg(test)]
    pub(super) rebuilds: usize,
}

struct Entry {
    entity: Entity,
    dirty: bool,
    requested: bool,
}

impl SourceDirtyIndex {
    pub(super) fn begin(&mut self, world: &mut World, sources: impl Iterator<Item = Entity>) {
        for entry in &mut self.entries {
            entry.requested = false;
        }
        for entity in sources.filter(|&entity| world.is_alive(entity)) {
            match self
                .entries
                .binary_search_by_key(&key(entity), |entry| key(entry.entity))
            {
                Ok(index) => self.entries[index].requested = true,
                Err(index) => {
                    if self.entries.try_reserve(1).is_err() {
                        *self = Self::default();
                        return;
                    }
                    self.entries.insert(
                        index,
                        Entry {
                            entity,
                            dirty: false,
                            requested: true,
                        },
                    );
                    self.revisions = None;
                }
            }
        }
        let previous_len = self.entries.len();
        self.entries.retain(|entry| entry.requested);
        if self.entries.len() != previous_len {
            self.revisions = None;
        }
        if self.entries.is_empty() {
            *self = Self::default();
            return;
        }
        if !self.available {
            world.watch_component_revision::<Dirty>();
            world.watch_component_revision::<VisualDirty>();
            world.watch_component_revision::<Parent>();
            world.watch_component_revision::<crate::ui::Hidden>();
            world.watch_component_revision::<crate::ui::branch::CachedBranchVisibility>();
        }
        self.available = true;
    }

    pub(super) fn refresh(&mut self, world: &World) {
        if !self.available {
            return;
        }
        let revisions = [
            // Dirty is a presence-only marker. Repeated writes still notify
            // geometry consumers, but cannot change this membership summary.
            world.component_membership_revision::<Dirty>(),
            world.component_revision::<VisualDirty>(),
            world.component_revision::<Parent>(),
            world.component_revision::<crate::ui::Hidden>(),
            world.component_revision::<crate::ui::branch::CachedBranchVisibility>(),
            world.retirement_revision(),
        ];
        if self.revisions != Some(revisions) {
            #[cfg(test)]
            {
                self.rebuilds += 1;
            }
            for entry in &mut self.entries {
                entry.dirty = false;
            }
            if let Some(storage) = world.storage::<Dirty>() {
                for &entity in storage.entities() {
                    self.mark(world, entity, true, true);
                }
            }
            if let Some(storage) = world.storage::<VisualDirty>() {
                for (entity, state) in storage.iter() {
                    self.mark(
                        world,
                        entity,
                        state.content_pending() == VisualDirty::New,
                        state.parent_pending() == VisualDirty::New,
                    );
                }
            }
            self.revisions = Some(revisions);
        }
    }

    pub(super) fn get(&self, world: &World, source: Entity) -> bool {
        if !self.available {
            return super::source_is_dirty(world, source);
        }
        self.entries
            .binary_search_by_key(&key(source), |entry| key(entry.entity))
            .is_ok_and(|index| self.entries[index].dirty)
    }

    #[cfg(test)]
    fn is_dirty(&mut self, world: &World, source: Entity) -> bool {
        self.refresh(world);
        self.get(world, source)
    }

    fn mark(&mut self, world: &World, mut entity: Entity, content: bool, ancestors: bool) {
        if !content && !ancestors {
            return;
        }
        let origin = entity;
        let mut visibility_checked = false;
        for depth in 0..=world.live_entity_count() {
            if ((depth == 0 && content) || (depth != 0 && ancestors))
                && let Ok(index) = self
                    .entries
                    .binary_search_by_key(&key(entity), |entry| key(entry.entity))
                && !self.entries[index].dirty
            {
                if !visibility_checked {
                    if crate::ui::branch::is_hidden_in_tree(world, origin) {
                        return;
                    }
                    visibility_checked = true;
                }
                self.entries[index].dirty = true;
            }
            if !ancestors {
                break;
            }
            let Some(parent) = world.get::<Parent>(entity) else {
                break;
            };
            entity = parent.0;
        }
    }
}

fn key(entity: Entity) -> (u32, u32) {
    (entity.id, entity.generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_visibility_matches_live_scan_for_nested_and_unrelated_sources() {
        let mut world = World::new();
        let nodes = core::array::from_fn::<_, 7, _>(|_| world.spawn_empty());
        for (child, parent) in [(1, 0), (2, 1), (3, 2), (4, 0), (6, 5)] {
            world.insert(nodes[child], Parent(nodes[parent]));
        }
        let sources = [nodes[0], nodes[1], nodes[2], nodes[5]];
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, sources.into_iter());
        for hidden in 0..(1 << nodes.len()) {
            for (bit, &entity) in nodes.iter().enumerate() {
                if hidden & (1 << bit) != 0 {
                    world.insert(entity, crate::ui::Hidden);
                } else {
                    world.remove::<crate::ui::Hidden>(entity);
                }
            }
            for &origin in &nodes {
                for state in [
                    None,
                    Some(VisualDirty::New),
                    Some(VisualDirty::Transform),
                    Some(VisualDirty::NewTransform),
                    Some(VisualDirty::Prepared),
                    Some(VisualDirty::PreparedTransform),
                ] {
                    if let Some(state) = state {
                        world.insert(origin, state);
                    } else {
                        world.insert(origin, Dirty);
                    }
                    index.refresh(&world);
                    for source in sources {
                        assert_eq!(
                            index.get(&world, source),
                            super::super::source_is_dirty(&world, source),
                            "visibility {hidden}, origin {origin:?}, source {source:?}"
                        );
                    }
                    world.remove::<Dirty>(origin);
                    world.remove::<VisualDirty>(origin);
                }
            }
        }
    }

    #[test]
    fn repeated_layout_damage_reuses_membership_without_hiding_writes() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(source));
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, [source].into_iter());
        assert!(!index.is_dirty(&world, source));
        world.invalidate(child);
        assert!(index.is_dirty(&world, source));
        let rebuilds = index.rebuilds;
        let written = world.component_revision::<Dirty>();
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..73 {
                    world.invalidate(child);
                    assert!(index.is_dirty(&world, source));
                }
            }),
            0
        );
        assert_ne!(world.component_revision::<Dirty>(), written);
        assert_eq!(index.rebuilds, rebuilds);

        world.remove::<Dirty>(child);
        assert!(!index.is_dirty(&world, source));
        world.invalidate(child);
        assert!(index.is_dirty(&world, source));
        world.despawn(child);
        assert!(!index.is_dirty(&world, source));
    }

    #[test]
    fn unchanged_membership_reuses_the_summary_until_inputs_change() {
        let mut world = World::new();
        let roots = [world.spawn_empty(), world.spawn_empty()];
        let child = world.spawn_empty();
        world.insert(child, Parent(roots[0]));
        world.insert(child, Dirty);
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, roots.into_iter());
        assert!(index.is_dirty(&world, roots[0]));
        let rebuilds = index.rebuilds;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..73 {
                    index.begin(&mut world, [roots[1], roots[0], roots[0]].into_iter());
                    assert!(index.is_dirty(&world, roots[0]));
                    assert!(!index.is_dirty(&world, roots[1]));
                }
            }),
            0
        );
        assert_eq!(index.rebuilds, rebuilds);

        // Topology and visibility can change while the same Dirty marker remains.
        world.insert(child, Parent(roots[1]));
        assert!(!index.is_dirty(&world, roots[0]));
        assert!(index.is_dirty(&world, roots[1]));
        world.insert(roots[1], crate::ui::Hidden);
        assert!(!index.is_dirty(&world, roots[1]));
        world.remove::<crate::ui::Hidden>(roots[1]);
        world.insert(
            child,
            crate::ui::branch::CachedBranchVisibility { selected: false },
        );
        assert!(!index.is_dirty(&world, roots[1]));
        world
            .get_mut::<crate::ui::branch::CachedBranchVisibility>(child)
            .unwrap()
            .selected = true;
        assert!(index.is_dirty(&world, roots[1]));
        world.remove::<Dirty>(child);
        assert!(!index.is_dirty(&world, roots[1]));
        world.invalidate_visual(child);
        assert!(index.is_dirty(&world, roots[1]));
        world.despawn(child);
        assert!(!index.is_dirty(&world, roots[1]));

        index.begin(&mut world, [roots[1]].into_iter());
        assert_eq!(index.entries.len(), 1);
        world.despawn(roots[1]);
        let replacement = world.spawn_empty();
        world.insert(replacement, Dirty);
        index.begin(&mut world, [replacement].into_iter());
        assert!(index.is_dirty(&world, replacement));
        assert!(!index.is_dirty(&world, roots[1]));
    }

    #[test]
    fn summary_matches_live_scan_across_visibility_motion_and_reparenting() {
        let mut world = World::new();
        let roots = [world.spawn_empty(), world.spawn_empty()];
        let child = world.spawn_empty();
        world.insert(child, Parent(roots[0]));
        let mut index = SourceDirtyIndex::default();
        for step in 0..8 {
            match step {
                0 => world.invalidate_transform(roots[0]),
                1 => world.invalidate_transform(child),
                2 => {
                    world.insert(child, crate::ui::Hidden);
                }
                3 => {
                    world.remove::<crate::ui::Hidden>(child);
                    world.insert(child, Parent(roots[1]));
                }
                4 => {
                    VisualDirty::prepare(&mut world, child);
                }
                5 => world.invalidate_visual(roots[0]),
                6 => {
                    world.insert(child, Dirty);
                }
                _ => {
                    world.despawn(child);
                }
            }
            index.begin(&mut world, roots.into_iter());
            for root in roots {
                assert_eq!(
                    index.is_dirty(&world, root),
                    super::super::source_is_dirty(&world, root),
                    "step {step}"
                );
            }
        }
        index.begin(&mut world, roots.into_iter());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                index.begin(&mut world, roots.into_iter());
                for root in roots {
                    index.is_dirty(&world, root);
                }
            }),
            0
        );
        world.invalidate_visual(roots[1]);
        assert!(index.is_dirty(&world, roots[1]));
        index.begin(&mut world, core::iter::empty());
        assert_eq!(index.entries.capacity(), 0);
    }
}
