use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::ui::Parent;
use crate::ui::dirty::{Dirty, VisualDirty};

#[derive(Default)]
pub(super) struct SourceDirtyIndex {
    entries: Vec<Entry>,
    revisions: Option<[u64; 2]>,
    available: bool,
}

struct Entry {
    entity: Entity,
    dirty: bool,
}

impl SourceDirtyIndex {
    pub(super) fn invalidate(&mut self) {
        self.revisions = None;
    }

    pub(super) fn begin(&mut self, world: &mut World, sources: impl Iterator<Item = Entity>) {
        self.entries.clear();
        self.revisions = None;
        self.available = false;
        for entity in sources.filter(|&entity| world.is_alive(entity)) {
            if self.entries.try_reserve(1).is_err() {
                self.entries.clear();
                return;
            }
            self.entries.push(Entry {
                entity,
                dirty: false,
            });
        }
        self.entries.sort_unstable_by_key(|entry| key(entry.entity));
        self.entries.dedup_by_key(|entry| key(entry.entity));
        if self.entries.is_empty() {
            *self = Self::default();
            return;
        }
        world.watch_component_revision::<Dirty>();
        world.watch_component_revision::<VisualDirty>();
        self.available = true;
    }

    pub(super) fn is_dirty(&mut self, world: &World, source: Entity) -> bool {
        if !self.available {
            return super::source_is_dirty(world, source);
        }
        let revisions = [
            world.component_revision::<Dirty>(),
            world.component_revision::<VisualDirty>(),
        ];
        if self.revisions != Some(revisions) {
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
        self.entries
            .binary_search_by_key(&key(source), |entry| key(entry.entity))
            .is_ok_and(|index| self.entries[index].dirty)
    }

    fn mark(&mut self, world: &World, mut entity: Entity, content: bool, ancestors: bool) {
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
