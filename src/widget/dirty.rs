//! Dirty tracking — per-entity, no parent→child propagation. A parent's
//! `Dirty` only contributes its own rect; descendants are unaffected
//! unless they're marked too. Use `mark_subtree_dirty` when a global
//! change (theme swap, viewport resize) needs every entity flagged.

use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};
use alloc::vec::Vec;

/// Dirty flag component — marks an entity as needing redraw.
pub struct Dirty;

pub fn mark_subtree_dirty(world: &mut World, root: Entity) {
    use crate::widget::{Children, Hidden};
    let mut stack = alloc::vec![root];
    while let Some(e) = stack.pop() {
        // Skip Hidden: the walker won't descend into it, so a marker
        // placed here would stick forever and defeat the empty-storage
        // fast path in `collect_dirty_region`.
        if world.get::<Hidden>(e).is_some() {
            continue;
        }
        world.insert(e, Dirty);
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.0.iter().copied());
        }
    }
}

/// Sweep `Dirty` from the subtree at `root`. Call before hiding the
/// subtree so the marker doesn't strand once the walker stops
/// descending through it.
pub fn clear_subtree_dirty(world: &mut World, root: Entity) {
    use crate::widget::Children;
    let mut stack = alloc::vec![root];
    while let Some(e) = stack.pop() {
        world.remove::<Dirty>(e);
        if let Some(children) = world.get::<Children>(e) {
            stack.extend(children.0.iter().copied());
        }
    }
}

/// Stores the previous rect before a position change
pub struct PrevRect(pub Rect);

/// Optional per-entity expansion of the dirty-region rect that the
/// walker writes when `Dirty` triggers. Effects whose paint area
/// extends past the entity's logical layout rect attach this so the
/// walker re-paints the full visual bounds when the source moves.
/// Each field is in *logical* pixels and adds to the corresponding
/// side. A `Default` value (all zeros) is a no-op.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaintInflate {
    pub left: Fixed,
    pub top: Fixed,
    pub right: Fixed,
    pub bottom: Fixed,
}

impl PaintInflate {
    pub const fn uniform(px: Fixed) -> Self {
        Self {
            left: px,
            top: px,
            right: px,
            bottom: px,
        }
    }
}

/// Tracks dirty regions for partial refresh
#[derive(Default)]
pub struct DirtyRegions {
    pub rects: Vec<Rect>,
}

impl DirtyRegions {
    pub fn new() -> Self {
        Self { rects: Vec::new() }
    }

    pub fn mark(&mut self, rect: Rect) {
        self.rects.push(rect);
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Merge all dirty rects into one bounding rect
    pub fn bounding_rect(&self) -> Option<Rect> {
        if self.rects.is_empty() {
            return None;
        }
        let mut min_x = Fixed::from_int(i32::MAX >> 8);
        let mut min_y = Fixed::from_int(i32::MAX >> 8);
        let mut max_x = Fixed::from_int(i32::MIN >> 8);
        let mut max_y = Fixed::from_int(i32::MIN >> 8);
        for r in &self.rects {
            let rx = r.x;
            let ry = r.y;
            let rx2 = (r.x + r.w).ceil();
            let ry2 = (r.y + r.h).ceil();
            if rx < min_x {
                min_x = rx;
            }
            if ry < min_y {
                min_y = ry;
            }
            if rx2 > max_x {
                max_x = rx2;
            }
            if ry2 > max_y {
                max_y = ry2;
            }
        }
        Some(Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::widget::Children;

    #[test]
    fn mark_subtree_walks_descendants() {
        let mut world = World::new();
        let root = world.spawn();
        let child_a = world.spawn();
        let child_b = world.spawn();
        let grandchild = world.spawn();
        world.insert(root, Children(alloc::vec![child_a, child_b]));
        world.insert(child_a, Children(alloc::vec![grandchild]));
        let outsider = world.spawn();

        mark_subtree_dirty(&mut world, root);

        assert!(world.get::<Dirty>(root).is_some());
        assert!(world.get::<Dirty>(child_a).is_some());
        assert!(world.get::<Dirty>(child_b).is_some());
        assert!(world.get::<Dirty>(grandchild).is_some());
        // Entities outside the rooted subtree must stay untouched, otherwise
        // a global mark would over-invalidate detached scenes.
        assert!(world.get::<Dirty>(outsider).is_none());
    }

    #[test]
    fn mark_subtree_handles_leaf() {
        let mut world = World::new();
        let only = world.spawn();
        mark_subtree_dirty(&mut world, only);
        assert!(world.get::<Dirty>(only).is_some());
    }
}
