//! Dirty tracking — per-entity, no parent→child propagation. A parent's
//! `Dirty` only contributes its own rect; descendants are unaffected
//! unless they're marked too. Use `mark_subtree_dirty` when a global
//! change (theme swap, viewport resize) needs every entity flagged.

use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};
use alloc::vec::Vec;

/// Dirty flag component — marks an entity as needing redraw.
pub struct Dirty;

/// Marks paint geometry dirty while preserving the existing layout snapshot.
pub(crate) struct VisualDirty;

pub(crate) struct ExactDirtyRegions {
    rects: [Rect; 4],
    len: u8,
}

impl Default for ExactDirtyRegions {
    fn default() -> Self {
        Self {
            rects: [Rect::ZERO; 4],
            len: 0,
        }
    }
}

impl ExactDirtyRegions {
    #[cfg_attr(not(feature = "gallery"), allow(dead_code))]
    fn mark(&mut self, rect: Rect) {
        let index = usize::from(self.len);
        if index < self.rects.len() {
            self.rects[index] = rect;
            self.len += 1;
        } else {
            let last = self.rects.len() - 1;
            self.rects[last] = self.rects[last].union(&rect);
        }
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(crate) fn drain_into(&mut self, target: &mut Vec<Rect>) {
        target.extend_from_slice(&self.rects[..usize::from(self.len)]);
        self.len = 0;
    }
}

#[cfg_attr(not(feature = "gallery"), allow(dead_code))]
pub(crate) fn mark_exact_dirty(world: &mut World, rect: Rect) {
    if let Some(regions) = world.resource_mut::<ExactDirtyRegions>() {
        regions.mark(rect);
        return;
    }
    let mut regions = ExactDirtyRegions::default();
    regions.mark(rect);
    world.insert_resource(regions);
}

pub fn mark_subtree_dirty(world: &mut World, root: Entity) {
    use crate::ui::{Children, Hidden};
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
    use crate::ui::Children;
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

/// One framebuffer self-blit op. `area` is the container's logical
/// rect, `(dx, dy)` is the shift in logical pixels. The walker emits
/// these in DFS post-order: a nested inner shift runs in the child's
/// local frame, then the outer shift carries the moved pixels along.
#[derive(Clone, Debug)]
pub struct RegionShift {
    pub area: Rect,
    pub dx: Fixed,
    pub dy: Fixed,
}

/// Plan returned by the dirty walker: `rects` to redraw + `shifts`
/// to memmove in place inside the framebuffer.
#[derive(Debug, Default)]
pub struct DirtyRegions {
    pub rects: Vec<Rect>,
    pub shifts: Vec<RegionShift>,
}

impl Clone for DirtyRegions {
    fn clone(&self) -> Self {
        Self {
            rects: self.rects.clone(),
            shifts: self.shifts.clone(),
        }
    }

    fn clone_from(&mut self, source: &Self) {
        self.rects.clear();
        self.rects.extend_from_slice(&source.rects);
        self.shifts.clear();
        self.shifts.extend_from_slice(&source.shifts);
    }
}

impl DirtyRegions {
    fn rect_pixel_area(rect: Rect) -> u64 {
        let (x0, y0, x1, y1) = rect.pixel_bounds();
        let width = i64::from(x1).saturating_sub(i64::from(x0)).max(0) as u64;
        let height = i64::from(y1).saturating_sub(i64::from(y0)).max(0) as u64;
        width.saturating_mul(height)
    }

    fn pixels_overlap(a: Rect, b: Rect) -> bool {
        let (ax0, ay0, ax1, ay1) = a.pixel_bounds();
        let (bx0, by0, bx1, by1) = b.pixel_bounds();
        ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1
    }

    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark(&mut self, rect: Rect) {
        self.rects.push(rect);
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.shifts.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty() && self.shifts.is_empty()
    }

    pub(crate) fn coalesce_overlaps(&mut self) {
        self.rects.retain(|rect| Self::rect_pixel_area(*rect) != 0);
        'coalesce: loop {
            for left in 0..self.rects.len() {
                for right in left + 1..self.rects.len() {
                    if Self::pixels_overlap(self.rects[left], self.rects[right]) {
                        let other = self.rects.swap_remove(right);
                        self.rects[left] = self.rects[left].union(&other);
                        continue 'coalesce;
                    }
                }
            }
            break;
        }
    }

    pub(crate) fn prefers_split_redraw(&self) -> bool {
        let Some(union) = self
            .rects
            .iter()
            .copied()
            .reduce(|left, right| left.union(&right))
        else {
            return false;
        };
        if self.rects.len() < 2 {
            return false;
        }
        let covered = self
            .rects
            .iter()
            .copied()
            .map(Self::rect_pixel_area)
            .fold(0_u64, u64::saturating_add);
        Self::rect_pixel_area(union).saturating_mul(2) > covered.saturating_mul(3)
    }

    /// Fold every shift's area into a redraw rect; the resulting plan
    /// has no shifts. Use when the active renderer can't self-blit.
    pub fn flatten_shifts(mut self) -> Self {
        for sop in self.shifts.drain(..) {
            self.rects.push(sop.area);
        }
        self
    }

    /// Bounding rect over `rects` and `shifts`'s areas, or `None`
    /// when the plan is empty.
    pub fn bounding_rect(&self) -> Option<Rect> {
        let mut min_x = Fixed::MAX;
        let mut min_y = Fixed::MAX;
        let mut max_x = Fixed::MIN;
        let mut max_y = Fixed::MIN;
        let mut any = false;
        let mut absorb = |r: &Rect| {
            if r.w <= Fixed::ZERO || r.h <= Fixed::ZERO {
                return;
            }
            any = true;
            if r.x < min_x {
                min_x = r.x;
            }
            if r.y < min_y {
                min_y = r.y;
            }
            let rx2 = r.x + r.w;
            let ry2 = r.y + r.h;
            if rx2 > max_x {
                max_x = rx2;
            }
            if ry2 > max_y {
                max_y = ry2;
            }
        };
        for r in &self.rects {
            absorb(r);
        }
        for s in &self.shifts {
            absorb(&s.area);
        }
        if !any {
            return None;
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
    use crate::ui::Children;

    #[test]
    fn mark_subtree_walks_descendants() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let child_a = world.spawn_empty();
        let child_b = world.spawn_empty();
        let grandchild = world.spawn_empty();
        world.insert(root, Children(alloc::vec![child_a, child_b]));
        world.insert(child_a, Children(alloc::vec![grandchild]));
        let outsider = world.spawn_empty();

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
        let only = world.spawn_empty();
        mark_subtree_dirty(&mut world, only);
        assert!(world.get::<Dirty>(only).is_some());
    }

    #[test]
    fn coalescing_uses_physical_pixel_overlap_and_closes_transitively() {
        let mut plan = DirtyRegions {
            rects: alloc::vec![
                Rect::new(0, 0, 4, 4),
                Rect::new(7, 0, 4, 4),
                Rect::new(3, 0, 5, 4),
            ],
            shifts: Vec::new(),
        };

        plan.coalesce_overlaps();

        assert_eq!(plan.rects, alloc::vec![Rect::new(0, 0, 11, 4)]);
    }

    #[test]
    fn sparse_regions_prefer_split_redraw_but_dense_regions_do_not() {
        let sparse = DirtyRegions {
            rects: alloc::vec![Rect::new(0, 0, 8, 8), Rect::new(96, 96, 8, 8)],
            shifts: Vec::new(),
        };
        let dense = DirtyRegions {
            rects: alloc::vec![Rect::new(0, 0, 8, 8), Rect::new(9, 0, 8, 8)],
            shifts: Vec::new(),
        };

        assert!(sparse.prefers_split_redraw());
        assert!(!dense.prefers_split_redraw());
    }

    #[test]
    fn exact_regions_stay_inline_and_union_overflow() {
        let mut regions = ExactDirtyRegions::default();
        for x in [0, 10, 20, 30, 40] {
            regions.mark(Rect::new(x, 0, 2, 2));
        }
        let mut rects = Vec::new();

        regions.drain_into(&mut rects);

        assert_eq!(rects.len(), 4);
        assert_eq!(rects[3], Rect::new(30, 0, 12, 2));
        assert!(regions.is_empty());
    }
}
