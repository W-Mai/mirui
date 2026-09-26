//! Retained roots for reactive `ui!` conditionals.

use alloc::vec::Vec;

use crate::ecs::{Entity, World};

use super::{Children, Hidden, Parent, dirty::PrevRect};

pub(crate) struct CachedBranchVisibility {
    pub selected: bool,
}

#[doc(hidden)]
pub fn is_effectively_hidden(world: &World, entity: Entity) -> bool {
    world.has::<Hidden>(entity)
        || world
            .get::<CachedBranchVisibility>(entity)
            .is_some_and(|branch| !branch.selected)
}

pub(crate) fn is_hidden_in_tree(world: &World, mut entity: Entity) -> bool {
    loop {
        if is_effectively_hidden(world, entity) {
            return true;
        }
        let Some(parent) = world.get::<Parent>(entity) else {
            return false;
        };
        entity = parent.0;
    }
}

/// Prepare all branch roots before the first selection. Branch bodies run once
/// while their parent is composed, including bodies that start out hidden.
#[doc(hidden)]
pub fn prepare(world: &mut World, parent: Entity, branches: &[Vec<Entity>]) {
    world.prepare_cached_branch_dirty(parent, branches);
    for roots in branches {
        for &root in roots {
            world.insert(root, CachedBranchVisibility { selected: false });
        }
    }
    for roots in branches {
        for &root in roots {
            world.clear_subtree_dirty(root);
        }
    }
}

/// Switch visibility without changing the parent-child list or rebuilding a
/// branch. Its entities, components, and nested widget state remain intact.
#[doc(hidden)]
pub fn select(
    world: &mut World,
    branches: &[Vec<Entity>],
    previous: Option<usize>,
    next: Option<usize>,
) {
    if previous == next {
        return;
    }
    if let Some(index) = previous {
        if let Some(roots) = branches.get(index) {
            for &root in roots {
                set_selected(world, root, false);
            }
        }
    }
    if let Some(index) = next {
        if let Some(roots) = branches.get(index) {
            for &root in roots {
                set_selected(world, root, true);
            }
        }
    }
    super::state::clear_hidden_interaction_states(world);
}

fn set_selected(world: &mut World, root: Entity, selected: bool) {
    let Some(branch) = world.get_mut::<CachedBranchVisibility>(root) else {
        return;
    };
    if branch.selected == selected {
        return;
    }
    if !selected {
        let mut old_bounds = None;
        collect_old_bounds(world, root, &mut old_bounds);
        if let Some(bounds) = old_bounds {
            world.invalidate_rect(bounds);
        }
    }
    world
        .get_mut::<CachedBranchVisibility>(root)
        .expect("cached branch root")
        .selected = selected;
    crate::input::event::hit_test::invalidate_hit_test_geometry(world);
    if selected {
        world.mark_subtree_dirty(root);
    } else {
        world.clear_subtree_dirty(root);
    }
    if let Some(parent) = world.get::<Parent>(root).map(|parent| parent.0) {
        world.invalidate(parent);
    }
}

fn collect_old_bounds(world: &World, entity: Entity, bounds: &mut Option<crate::types::Rect>) {
    if let Some(rect) = world.get::<PrevRect>(entity).map(|rect| rect.0) {
        *bounds = Some(bounds.map_or(rect, |current| current.union(&rect)));
    }
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            collect_old_bounds(world, child, bounds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Rect;
    use crate::ui::dirty::ExactDirtyRegions;

    #[test]
    fn selection_does_not_remove_an_independent_hidden_gate() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let root = world.spawn_empty();
        world.insert(parent, Children(alloc::vec![root]));
        world.insert(root, Parent(parent));
        world.insert(root, Hidden);
        let branches = [alloc::vec![root]];
        prepare(&mut world, parent, &branches);

        select(&mut world, &branches, None, Some(0));
        assert!(world.has::<Hidden>(root));
        assert!(is_effectively_hidden(&world, root));
        select(&mut world, &branches, Some(0), None);
        select(&mut world, &branches, None, Some(0));
        assert!(world.has::<Hidden>(root));
        assert!(is_effectively_hidden(&world, root));
    }

    #[test]
    fn hiding_invalidates_painted_descendants_outside_the_root_rect() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let root = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(parent, Children(alloc::vec![root]));
        world.insert(root, Parent(parent));
        world.insert(root, Children(alloc::vec![child]));
        world.insert(child, Parent(root));
        let branches = [alloc::vec![root]];
        prepare(&mut world, parent, &branches);
        select(&mut world, &branches, None, Some(0));
        world.insert(root, PrevRect(Rect::new(0, 0, 10, 10)));
        world.insert(child, PrevRect(Rect::new(25, 3, 5, 4)));

        select(&mut world, &branches, Some(0), None);
        let mut rects = alloc::vec::Vec::new();
        world
            .resource_mut::<ExactDirtyRegions>()
            .unwrap()
            .drain_into(&mut rects);
        assert_eq!(rects, [Rect::new(0, 0, 30, 10)]);
    }
}
