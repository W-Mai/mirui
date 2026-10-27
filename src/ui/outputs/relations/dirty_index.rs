use alloc::vec::Vec;

use crate::ecs::{Entity, SparseSet, World};
use crate::ui::Parent;
use crate::ui::dirty::{Dirty, VisualDirty};

#[derive(Default)]
pub(super) struct SourceDirtyIndex {
    entries: Vec<Entry>,
    revisions: Option<[u64; 7]>,
    available: bool,
    layout_unrelated: bool,
    visual_unrelated: bool,
    #[cfg(test)]
    pub(super) rebuilds: usize,
    #[cfg(test)]
    layout_scans: usize,
    #[cfg(test)]
    visual_scans: usize,
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
            world.component_membership_revision::<VisualDirty>(),
        ];
        if self.revisions != Some(revisions) {
            let topology_changed = self
                .revisions
                .is_none_or(|previous| previous[2..6] != revisions[2..6]);
            let layout_changed = self
                .revisions
                .is_none_or(|previous| previous[0] != revisions[0] || topology_changed);
            let visual_changed = self
                .revisions
                .is_none_or(|previous| previous[6] != revisions[6] || topology_changed);
            #[cfg(test)]
            {
                self.rebuilds += 1;
            }
            for entry in &mut self.entries {
                entry.dirty = false;
            }
            let parents = world.storage::<Parent>();
            if layout_changed || !self.layout_unrelated {
                if layout_changed {
                    self.layout_unrelated = true;
                }
                #[cfg(test)]
                {
                    self.layout_scans += 1;
                }
                if let Some(storage) = world.storage::<Dirty>() {
                    for &entity in storage.entities() {
                        if self.layout_unrelated {
                            if !self.may_dirty_source(world, parents, entity) {
                                continue;
                            }
                            self.layout_unrelated = false;
                        }
                        self.mark(world, parents, entity, true, true);
                    }
                }
            }
            if visual_changed || !self.visual_unrelated {
                if visual_changed {
                    self.visual_unrelated = true;
                }
                #[cfg(test)]
                {
                    self.visual_scans += 1;
                }
                if let Some(storage) = world.storage::<VisualDirty>() {
                    for (entity, state) in storage.iter() {
                        if self.visual_unrelated {
                            if !self.may_dirty_source(world, parents, entity) {
                                continue;
                            }
                            self.visual_unrelated = false;
                        }
                        self.mark(
                            world,
                            parents,
                            entity,
                            state.content_pending() == VisualDirty::New,
                            state.parent_pending() == VisualDirty::New,
                        );
                    }
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

    fn mark(
        &mut self,
        world: &World,
        parents: Option<&SparseSet<Parent>>,
        mut entity: Entity,
        content: bool,
        ancestors: bool,
    ) {
        if (!content && !ancestors) || crate::ui::branch::is_hidden_in_tree(world, entity) {
            return;
        }
        for depth in 0..=world.live_entity_count() {
            if (depth == 0 && content) || (depth != 0 && ancestors) {
                if let Ok(index) = self
                    .entries
                    .binary_search_by_key(&key(entity), |entry| key(entry.entity))
                {
                    self.entries[index].dirty = true;
                }
            }
            if !ancestors {
                break;
            }
            let Some(parent) = parents.and_then(|parents| parents.get(entity)) else {
                break;
            };
            entity = parent.0;
        }
    }

    // Only revision-tracked authored visibility can prove a branch unrelated.
    // Derived visibility remains part of mark's live check.
    fn may_dirty_source(
        &self,
        world: &World,
        parents: Option<&SparseSet<Parent>>,
        mut entity: Entity,
    ) -> bool {
        let mut related = false;
        for _ in 0..=world.live_entity_count() {
            if crate::ui::branch::is_authored_hidden(world, entity) {
                return false;
            }
            related = related
                || self
                    .entries
                    .binary_search_by_key(&key(entity), |entry| key(entry.entity))
                    .is_ok();
            let Some(parent) = parents.and_then(|parents| parents.get(entity)) else {
                break;
            };
            entity = parent.0;
        }
        related
    }
}

fn key(entity: Entity) -> (u32, u32) {
    (entity.id, entity.generation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_visual_members_reuse_their_proof_until_membership_or_visibility_changes() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let hidden = world.spawn_empty();
        let child = world.spawn_empty();
        let unrelated = world.spawn_empty();
        world.insert(hidden, Parent(source));
        world.insert(hidden, crate::ui::Hidden);
        world.insert(child, Parent(hidden));
        world.insert(child, VisualDirty::Prepared);
        world.insert(unrelated, VisualDirty::New);
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, [source].into_iter());
        assert!(!index.is_dirty(&world, source));
        let scans = index.visual_scans;
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..73 {
                    world.invalidate_visual(child);
                    world.invalidate_transform(unrelated);
                    assert!(!index.is_dirty(&world, source));
                }
            }),
            0
        );
        assert_eq!(index.visual_scans, scans);

        // Layout membership changes do not invalidate an independent proof.
        world.insert(unrelated, Dirty);
        assert!(!index.is_dirty(&world, source));
        assert_eq!(index.visual_scans, scans);
        world.remove::<Dirty>(unrelated);
        assert!(!index.is_dirty(&world, source));
        assert_eq!(index.visual_scans, scans);

        world.remove::<crate::ui::Hidden>(hidden);
        assert!(index.is_dirty(&world, source));
        VisualDirty::prepare(&mut world, child);
        assert!(!index.is_dirty(&world, source));
        world.invalidate_visual(child);
        assert!(index.is_dirty(&world, source));
        world.insert(hidden, crate::ui::Hidden);
        assert!(!index.is_dirty(&world, source));
        world.insert(unrelated, Parent(source));
        assert!(index.is_dirty(&world, source));
        world.despawn(unrelated);
        assert!(!index.is_dirty(&world, source));

        // A source's transform is not its content, but it cannot prove that
        // the same member will remain unrelated after a content write.
        world.insert(source, VisualDirty::Transform);
        assert!(!index.is_dirty(&world, source));
        world.invalidate_visual(source);
        assert!(index.is_dirty(&world, source));
        world.remove::<VisualDirty>(source);
        assert!(!index.is_dirty(&world, source));
        index.begin(&mut world, [hidden].into_iter());
        assert!(!index.is_dirty(&world, hidden));
        world.remove::<crate::ui::Hidden>(hidden);
        assert!(index.is_dirty(&world, hidden));
        index.begin(&mut world, core::iter::empty());
        assert!(!index.available);
        assert_eq!(index.entries.capacity(), 0);
    }

    #[test]
    fn unrelated_layout_members_reuse_their_proof_across_visual_changes() {
        let mut world = World::new();
        let source = world.spawn_empty();
        let child = world.spawn_empty();
        let unrelated = world.spawn_empty();
        world.insert(child, Parent(source));
        world.insert(unrelated, Dirty);
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, [source].into_iter());
        assert!(!index.is_dirty(&world, source));
        let scans = index.layout_scans;
        world.invalidate_visual(unrelated);
        assert!(!index.is_dirty(&world, source));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..73 {
                    world.invalidate_visual(unrelated);
                    assert!(!index.is_dirty(&world, source));
                }
            }),
            0
        );
        assert_eq!(index.layout_scans, scans);

        world.insert(child, Dirty);
        assert!(index.is_dirty(&world, source));
        world.remove::<Dirty>(child);
        assert!(!index.is_dirty(&world, source));
        world.insert(unrelated, Parent(child));
        assert!(index.is_dirty(&world, source));
        world.insert(child, crate::ui::Hidden);
        assert!(!index.is_dirty(&world, source));
        assert!(index.layout_unrelated);
        world.remove::<crate::ui::Hidden>(child);
        assert!(index.is_dirty(&world, source));
        world.despawn(unrelated);
        assert!(!index.is_dirty(&world, source));

        world.insert(child, Dirty);
        index.begin(&mut world, [child].into_iter());
        assert!(index.is_dirty(&world, child));
        index.begin(&mut world, core::iter::empty());
        assert!(!index.available);
        assert!(!index.layout_unrelated);
        assert_eq!(index.entries.capacity(), 0);
    }

    #[test]
    fn hidden_layout_members_invalidate_the_proof_when_revealed() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let source = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(source, Parent(root));
        world.insert(child, Parent(source));
        world.insert(child, Dirty);
        world.insert(
            child,
            crate::ui::branch::CachedBranchVisibility { selected: false },
        );
        let mut index = SourceDirtyIndex::default();
        index.begin(&mut world, [source].into_iter());
        assert!(!index.is_dirty(&world, source));
        assert!(index.layout_unrelated);
        let scans = index.layout_scans;
        world.invalidate_visual(root);
        assert!(!index.is_dirty(&world, source));
        assert_eq!(index.layout_scans, scans);

        world
            .get_mut::<crate::ui::branch::CachedBranchVisibility>(child)
            .unwrap()
            .selected = true;
        assert!(index.is_dirty(&world, source));
        world.insert(root, crate::ui::Hidden);
        assert!(!index.is_dirty(&world, source));
        assert!(index.layout_unrelated);
        let scans = index.layout_scans;
        world.invalidate_visual(root);
        assert!(!index.is_dirty(&world, source));
        assert_eq!(index.layout_scans, scans);
        world.remove::<crate::ui::Hidden>(root);
        assert!(index.is_dirty(&world, source));
    }

    #[test]
    fn summary_matches_live_scan_for_nested_and_unrelated_sources() {
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
