//! Dirty tracking — per-entity, no parent→child propagation. A parent's
//! `Dirty` only contributes its own rect; descendants are unaffected
//! unless they're marked too. Use `World::mark_subtree_dirty` when a global
//! change (theme swap, viewport resize) needs every entity flagged. Transform
//! damage follows descendants during collection without invalidating their content.

use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};
use alloc::vec::Vec;

mod coverage;
mod regions;
pub(crate) use regions::RegionSet;
#[cfg(test)]
mod baseline_tests;
#[cfg(test)]
mod traversal_tests;
pub(crate) use coverage::PaintCoverage;

/// Dirty flag component — marks an entity as needing redraw.
pub struct Dirty;

impl crate::ecs::Component for Dirty {}

/// Pending paint work, independent of layout preparation.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum VisualDirty {
    #[default]
    New,
    Prepared,
    Transform,
    NewTransform,
    PreparedTransform,
}

impl crate::ecs::Component for VisualDirty {}

impl VisualDirty {
    pub(crate) fn transforms_subtree(self) -> bool {
        matches!(
            self,
            Self::Transform | Self::NewTransform | Self::PreparedTransform
        )
    }

    fn with_content(self) -> Self {
        if self.transforms_subtree() {
            Self::NewTransform
        } else {
            Self::New
        }
    }

    fn with_transform(self) -> Self {
        if matches!(self, Self::New | Self::NewTransform) {
            Self::NewTransform
        } else {
            Self::Transform
        }
    }

    // A capture excludes its own transform; its containing capture includes it.
    pub(crate) fn content_pending(self) -> Self {
        match self {
            Self::New | Self::NewTransform => Self::New,
            _ => Self::Prepared,
        }
    }

    pub(crate) fn parent_pending(self) -> Self {
        match self {
            Self::New | Self::NewTransform | Self::Transform => Self::New,
            _ => Self::Prepared,
        }
    }

    pub(crate) fn prepare(world: &mut World, entity: Entity) -> Option<Self> {
        let previous = world.get::<Self>(entity).copied();
        let state = if world.remove::<Dirty>(entity).is_some() {
            Some(previous.unwrap_or(Self::New).with_content())
        } else {
            previous
        };
        if let Some(state) = state {
            let prepared = if state.transforms_subtree() {
                Self::PreparedTransform
            } else {
                Self::Prepared
            };
            if state != prepared {
                world.insert(entity, prepared);
            }
        }
        state
    }

    pub(crate) fn acknowledge(world: &mut World, entity: Entity) {
        if matches!(
            world.get::<Self>(entity),
            Some(Self::Prepared | Self::PreparedTransform)
        ) {
            world.remove::<Self>(entity);
        }
    }
}

struct DirtyTraversalStack(Option<Vec<Entity>>);

struct DirtyTraversal<'a> {
    world: &'a mut World,
    stack: Vec<Entity>,
}

impl<'a> DirtyTraversal<'a> {
    fn new(world: &'a mut World, root: Entity) -> Self {
        if world.resource::<DirtyTraversalStack>().is_none() {
            world.insert_resource(DirtyTraversalStack(Some(Vec::new())));
        }
        let stack = world
            .resource_mut::<DirtyTraversalStack>()
            .and_then(|storage| storage.0.take())
            .expect("dirty traversal cannot be nested");
        let mut traversal = Self { world, stack };
        traversal.stack.push(root);
        traversal
    }

    fn walk(mut self, mark: bool) {
        use crate::ui::Children;
        while let Some(entity) = self.stack.pop() {
            if mark && crate::ui::branch::is_effectively_hidden(self.world, entity) {
                continue;
            }
            if mark {
                self.world.insert(entity, Dirty);
            } else {
                self.world.remove::<Dirty>(entity);
                self.world.remove::<VisualDirty>(entity);
            }
            if let Some(children) = self.world.get::<Children>(entity) {
                self.stack.extend(children.0.iter().copied());
            }
        }
    }

    fn prepare(mut self) -> (usize, usize, u32) {
        use crate::ui::Children;
        let mut missing_computed = 0usize;
        let mut missing_prev = 0usize;
        let mut max_entity_id = 0u32;
        while let Some(entity) = self.stack.pop() {
            if self.world.has::<super::Widget>(entity) && self.world.has::<super::Style>(entity) {
                max_entity_id = max_entity_id.max(entity.id);
                missing_computed += usize::from(!self.world.has::<super::ComputedRect>(entity));
                missing_prev += usize::from(!self.world.has::<PaintedCoverage>(entity));
            }
            self.world.insert(entity, Dirty);
            if let Some(children) = self.world.get::<Children>(entity) {
                self.stack.extend(children.0.iter().copied());
            }
        }
        (missing_computed, missing_prev, max_entity_id)
    }

    fn visit(mut self, mut visit: impl FnMut(&mut World, Entity)) {
        while let Some(entity) = self.stack.pop() {
            visit(self.world, entity);
            if let Some(children) = self.world.get::<super::Children>(entity) {
                self.stack.extend(children.0.iter().copied());
            }
        }
    }
}

impl Drop for DirtyTraversal<'_> {
    fn drop(&mut self) {
        self.stack.clear();
        self.world
            .resource_mut::<DirtyTraversalStack>()
            .expect("dirty traversal storage is missing")
            .0 = Some(core::mem::take(&mut self.stack));
    }
}

pub(crate) const EXACT_DIRTY_REGION_CAPACITY: usize = 8;

pub(crate) struct ExactDirtyRegions {
    regions: RegionSet<EXACT_DIRTY_REGION_CAPACITY>,
    prepared: bool,
}

impl Default for ExactDirtyRegions {
    fn default() -> Self {
        Self {
            regions: RegionSet::new(),
            prepared: false,
        }
    }
}

impl ExactDirtyRegions {
    fn mark(&mut self, rect: Rect) {
        self.prepared = false;
        self.regions.insert(rect);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.regions.as_slice().is_empty()
    }

    pub(crate) fn prepare_into(&mut self, target: &mut Vec<Rect>) {
        target.extend_from_slice(self.regions.as_slice());
        self.prepared = true;
    }

    pub(crate) fn acknowledge(&mut self) {
        if self.prepared {
            self.regions.clear();
            self.prepared = false;
        }
    }
}

impl World {
    pub(crate) fn invalidate_painted_coverage(&mut self, entity: Entity) {
        if let Some(painted) = self.get::<PaintedCoverage>(entity).copied() {
            for rect in painted.0.regions() {
                self.invalidate_rect(rect);
            }
        }
    }

    pub(crate) fn invalidate_subtree_paint(&mut self, root: Entity) {
        DirtyTraversal::new(self, root).visit(Self::invalidate_painted_coverage);
    }

    pub(crate) fn invalidate_subtree_visual(&mut self, root: Entity) {
        DirtyTraversal::new(self, root).visit(Self::invalidate_visual);
    }

    pub(crate) fn clear_subtree_paint(&mut self, root: Entity) {
        DirtyTraversal::new(self, root).visit(|world, entity| {
            if let Some(painted) = world.get_mut::<PaintedCoverage>(entity) {
                painted.0 = PaintCoverage::EMPTY;
            }
        });
    }

    pub(crate) fn reserve_dirty_traversal_stack(&mut self) {
        let required = self.live_entity_count();
        if self.resource::<DirtyTraversalStack>().is_none() {
            self.insert_resource(DirtyTraversalStack(Some(Vec::new())));
        }
        let stack = self
            .resource_mut::<DirtyTraversalStack>()
            .and_then(|storage| storage.0.as_mut())
            .expect("dirty traversal cannot be nested");
        if stack.capacity() < required {
            stack.reserve_exact(required.saturating_sub(stack.len()));
        }
    }

    pub(crate) fn prepare_cached_branch_dirty(&mut self, parent: Entity, branches: &[Vec<Entity>]) {
        if self.resource::<ExactDirtyRegions>().is_none() {
            self.insert_resource(ExactDirtyRegions::default());
        }
        self.invalidate(parent);
        let mut missing_computed = 0usize;
        let mut missing_prev = 0usize;
        let mut max_entity_id = 0u32;
        for roots in branches {
            for &root in roots {
                let (branch_computed, branch_prev, branch_max_entity_id) =
                    DirtyTraversal::new(self, root).prepare();
                missing_computed = missing_computed.saturating_add(branch_computed);
                missing_prev = missing_prev.saturating_add(branch_prev);
                max_entity_id = max_entity_id.max(branch_max_entity_id);
            }
        }
        if missing_computed > 0 {
            self.reserve_component_storage::<super::ComputedRect>(max_entity_id, missing_computed);
        }
        if missing_prev > 0 {
            self.reserve_component_storage::<PaintedCoverage>(max_entity_id, missing_prev);
        }
    }

    /// Invalidates an entity's layout and visual output.
    pub fn invalidate(&mut self, entity: Entity) {
        self.insert(entity, Dirty);
    }

    /// Invalidates an entity's visual output while preserving its layout.
    pub fn invalidate_visual(&mut self, entity: Entity) {
        let pending = self
            .get::<VisualDirty>(entity)
            .copied()
            .unwrap_or(VisualDirty::New);
        self.insert(entity, pending.with_content());
    }

    pub(crate) fn invalidate_transform(&mut self, entity: Entity) {
        let pending = self
            .get::<VisualDirty>(entity)
            .copied()
            .unwrap_or(VisualDirty::Transform);
        self.insert(entity, pending.with_transform());
    }

    /// Invalidates a logical rectangle without invalidating layout.
    ///
    /// Use this when retained geometry changes inside known bounds and the
    /// current layout snapshot remains valid.
    pub fn invalidate_rect(&mut self, rect: Rect) {
        if let Some(regions) = self.resource_mut::<ExactDirtyRegions>() {
            regions.mark(rect);
            return;
        }
        let mut regions = ExactDirtyRegions::default();
        regions.mark(rect);
        self.insert_resource(regions);
    }

    pub fn mark_subtree_dirty(&mut self, root: Entity) {
        DirtyTraversal::new(self, root).walk(true);
    }

    /// Sweep layout and visual dirty markers from a subtree before hiding it.
    pub fn clear_subtree_dirty(&mut self, root: Entity) {
        DirtyTraversal::new(self, root).walk(false);
    }
}

pub(crate) fn has_visible_dirty<T: 'static>(world: &World, root: Entity) -> bool {
    world.storage::<T>().is_some_and(|storage| {
        storage
            .entities()
            .iter()
            .copied()
            .any(|entity| is_visible_descendant(world, root, entity))
    })
}

fn is_visible_descendant(world: &World, root: Entity, mut entity: Entity) -> bool {
    loop {
        if crate::ui::branch::is_effectively_hidden(world, entity) {
            return false;
        }
        if entity == root {
            return true;
        }
        let Some(parent) = world.get::<crate::ui::Parent>(entity) else {
            // Parent links are optional for manually built worlds. Preserve
            // their existing conservative dirty behavior.
            return true;
        };
        entity = parent.0;
    }
}

/// Stores the visible paint bounds within the committed clip after a successful render.
#[derive(Clone, Copy)]
pub(crate) struct PaintedCoverage(pub PaintCoverage);

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
    scratch: Vec<Rect>,
}

pub(crate) struct PhysicalDamage {
    rects: [crate::types::PhysicalRect; super::render_system::DIRTY_PLAN_RECT_CAPACITY],
    len: usize,
}

impl PhysicalDamage {
    pub(crate) fn as_slice(&self) -> &[crate::types::PhysicalRect] {
        &self.rects[..self.len]
    }
}

#[cfg(test)]
mod physical_damage_tests {
    use super::*;
    use crate::types::{PhysicalRect, Viewport};

    #[test]
    fn physical_damage_is_disjoint_covers_quantized_edges_and_reuses_scratch() {
        let mut plan =
            DirtyRegions::with_rect_capacity(super::super::render_system::DIRTY_PLAN_RECT_CAPACITY);
        plan.rects.extend_from_slice(&[
            Rect::new(1, 1, 6, 2),
            Rect::new(7, 2, 3, 9),
            Rect::new(1, 10, 8, 2),
            Rect::new(1, 1, 2, 11),
        ]);
        let logical = plan.rects.clone();
        for scale in [Fixed::from_ratio(229, 256), Fixed::from_ratio(3, 2)] {
            let viewport = Viewport::new(23, 21, scale);
            let allocations = crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    let physical = plan.physical_redraw(viewport, false);
                    for (i, rect) in physical.as_slice().iter().enumerate() {
                        assert!(
                            physical.as_slice()[i + 1..]
                                .iter()
                                .all(|other| rect.intersection(*other).is_none())
                        );
                    }
                    for input in &logical {
                        if let Some(input) = viewport.physical_rect(*input) {
                            for y in input.y()..input.bottom() {
                                for x in input.x()..input.right() {
                                    let pixel = PhysicalRect::new(x, y, 1, 1).unwrap();
                                    assert!(
                                        physical
                                            .as_slice()
                                            .iter()
                                            .any(|rect| rect.intersection(pixel).is_some())
                                    );
                                }
                            }
                        }
                    }
                    assert_eq!(plan.rects, logical);
                }
            });
            assert_eq!(allocations, 0);
            assert_eq!(
                plan.physical_redraw(viewport, true).as_slice(),
                &[PhysicalRect::from_size(23, 21)]
            );
        }
    }
}

impl Clone for DirtyRegions {
    fn clone(&self) -> Self {
        Self {
            rects: self.rects.clone(),
            shifts: self.shifts.clone(),
            scratch: Vec::new(),
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

    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_rect_capacity(capacity: usize) -> Self {
        Self {
            rects: Vec::with_capacity(capacity),
            shifts: Vec::new(),
            scratch: Vec::with_capacity(capacity),
        }
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
        self.scratch.clear();
        self.scratch.extend_from_slice(&self.rects);
        regions::normalize(&mut self.scratch, &mut self.rects);
    }

    pub(crate) fn physical_redraw(
        &mut self,
        viewport: crate::types::Viewport,
        full: bool,
    ) -> PhysicalDamage {
        use crate::types::PhysicalRect;
        let mut result = PhysicalDamage {
            rects: [PhysicalRect::EMPTY; super::render_system::DIRTY_PLAN_RECT_CAPACITY],
            len: 0,
        };
        if full {
            let (width, height) = viewport.physical_size();
            if width != 0 && height != 0 {
                result.rects[0] = PhysicalRect::from_size(width, height);
                result.len = 1;
            }
            return result;
        }
        self.scratch.clear();
        for rect in &self.rects {
            if let Some(area) = viewport.physical_rect(*rect) {
                self.scratch
                    .push(Rect::new(area.x(), area.y(), area.width(), area.height()));
            }
        }
        if !self.prefers_split_redraw()
            && let Some(union) = self.scratch.iter().copied().reduce(|a, b| a.union(&b))
        {
            self.scratch.clear();
            self.scratch.push(union);
        }
        let mut normalized = [Rect::ZERO; super::render_system::DIRTY_PLAN_RECT_CAPACITY];
        result.len = regions::normalize_into(&mut self.scratch, &mut normalized);
        for (dst, rect) in result.rects.iter_mut().zip(&normalized[..result.len]) {
            *dst = PhysicalRect::new(
                rect.x.to_int() as u16,
                rect.y.to_int() as u16,
                rect.w.to_int() as u16,
                rect.h.to_int() as u16,
            )
            .expect("normalized target-clipped rectangle");
        }
        result
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

    /// Iterate nonempty redraw and scroll-transfer areas in logical pixels without allocating or consuming the plan. Areas may overlap.
    pub fn affected_regions(&self) -> impl Iterator<Item = Rect> + '_ {
        self.rects
            .iter()
            .copied()
            .chain(self.shifts.iter().map(|shift| shift.area))
            .filter(|rect| rect.w > Fixed::ZERO && rect.h > Fixed::ZERO)
    }

    /// Bounding rect over redraw and scroll-transfer areas, or `None` when no nonempty area remains.
    pub fn bounding_rect(&self) -> Option<Rect> {
        self.affected_regions()
            .reduce(|left, right| left.union(&right))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::ui::{Children, Hidden};

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

        world.mark_subtree_dirty(root);

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
        world.mark_subtree_dirty(only);
        assert!(world.get::<Dirty>(only).is_some());
    }

    #[test]
    fn hidden_subtrees_are_skipped_on_mark_but_cleared() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(root, Children(alloc::vec![child]));
        world.insert(child, Hidden);
        world.insert(child, Dirty);
        world.insert(child, VisualDirty::New);

        world.mark_subtree_dirty(root);
        assert!(world.get::<Dirty>(root).is_some());
        assert!(world.get::<VisualDirty>(child).is_some());

        world.clear_subtree_dirty(root);
        assert!(world.get::<Dirty>(root).is_none());
        assert!(world.get::<Dirty>(child).is_none());
        assert!(world.get::<VisualDirty>(child).is_none());
    }

    #[test]
    fn cached_branch_gate_skips_marks_until_selected() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let branch = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(root, Children(alloc::vec![branch]));
        world.insert(branch, Children(alloc::vec![child]));
        world.insert(
            branch,
            crate::ui::branch::CachedBranchVisibility { selected: false },
        );

        world.mark_subtree_dirty(root);
        assert!(world.has::<Dirty>(root));
        assert!(!world.has::<Dirty>(branch));
        assert!(!world.has::<Dirty>(child));

        world
            .get_mut::<crate::ui::branch::CachedBranchVisibility>(branch)
            .unwrap()
            .selected = true;
        world.mark_subtree_dirty(branch);
        assert!(world.has::<Dirty>(branch));
        assert!(world.has::<Dirty>(child));
    }

    #[test]
    fn reserved_stack_covers_hidden_reveal_and_same_size_reparenting() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let shallow = world.spawn_empty();
        let hidden = world.spawn_empty();
        let children: Vec<_> = (0..8).map(|_| world.spawn_empty()).collect();
        world.insert(root, Children(alloc::vec![shallow, hidden]));
        world.insert(shallow, Children(children.clone()));
        world.insert(hidden, Hidden);

        world.reserve_dirty_traversal_stack();
        let prepared = world
            .resource::<DirtyTraversalStack>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .capacity();
        assert!(prepared >= world.live_entity_count());

        world.insert(shallow, Children(Vec::new()));
        world.insert(hidden, Children(children.clone()));
        world.remove::<Hidden>(hidden);
        world.mark_subtree_dirty(hidden);
        let after = world
            .resource::<DirtyTraversalStack>()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .capacity();
        assert_eq!(after, prepared);
        assert!(children.iter().all(|&entity| world.has::<Dirty>(entity)));
    }

    #[test]
    fn interrupted_traversal_returns_an_empty_stack() {
        let mut world = World::new();
        let root = world.spawn_empty();
        let pending = world.spawn_empty();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut traversal = DirtyTraversal::new(&mut world, root);
            traversal.stack.push(pending);
            panic!("interrupt traversal");
        }));
        assert!(result.is_err());
        assert!(
            world
                .resource::<DirtyTraversalStack>()
                .is_some_and(|storage| storage.0.as_ref().is_some_and(Vec::is_empty))
        );

        world.mark_subtree_dirty(root);
        assert!(world.get::<Dirty>(root).is_some());
        assert!(world.get::<Dirty>(pending).is_none());
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
            ..Default::default()
        };

        plan.coalesce_overlaps();

        assert_eq!(plan.rects, alloc::vec![Rect::new(0, 0, 11, 4)]);
    }

    #[test]
    fn normalization_reuses_prepared_storage_and_rounds_fractional_damage_outward() {
        let mut plan = DirtyRegions::with_rect_capacity(8);
        let rects = [
            Rect::new(Fixed::from_f32(-0.5), Fixed::from_f32(0.25), 5, 2),
            Rect::new(1, 1, 2, 5),
            Rect::ZERO,
        ];
        let pointers = (plan.rects.as_ptr(), plan.scratch.as_ptr());
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                plan.clear();
                plan.rects.extend_from_slice(&rects);
                plan.coalesce_overlaps();
                assert_eq!(plan.bounding_rect(), Some(Rect::new(-1, 0, 6, 6)));
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!((plan.rects.as_ptr(), plan.scratch.as_ptr()), pointers);
        assert!(plan.scratch.is_empty());
    }

    #[test]
    fn sparse_regions_prefer_split_redraw_but_dense_regions_do_not() {
        let sparse = DirtyRegions {
            rects: alloc::vec![Rect::new(0, 0, 8, 8), Rect::new(96, 96, 8, 8)],
            shifts: Vec::new(),
            ..Default::default()
        };
        let dense = DirtyRegions {
            rects: alloc::vec![Rect::new(0, 0, 8, 8), Rect::new(9, 0, 8, 8)],
            shifts: Vec::new(),
            ..Default::default()
        };

        assert!(sparse.prefers_split_redraw());
        assert!(!dense.prefers_split_redraw());
    }

    #[test]
    fn exact_regions_stay_inline_and_preserve_coverage_on_overflow() {
        let mut regions = ExactDirtyRegions::default();
        for x in (0..=8).map(|i| i * 10) {
            regions.mark(Rect::new(x, 0, 2, 2));
        }
        let mut rects = Vec::new();

        regions.prepare_into(&mut rects);

        assert_eq!(rects.len(), EXACT_DIRTY_REGION_CAPACITY);
        for x in (0..=8).map(|i| i * 10) {
            let original = Rect::new(x, 0, 2, 2);
            assert!(
                rects
                    .iter()
                    .any(|rect| rect.intersect(&original) == Some(original))
            );
        }
        assert!(!regions.is_empty());
        regions.acknowledge();
        assert!(regions.is_empty());
    }

    #[test]
    fn acknowledgement_preserves_damage_received_after_preparation() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(entity, Dirty);
        assert!(matches!(
            VisualDirty::prepare(&mut world, entity),
            Some(VisualDirty::New)
        ));
        assert!(matches!(
            VisualDirty::prepare(&mut world, entity),
            Some(VisualDirty::Prepared)
        ));
        world.invalidate_visual(entity);
        VisualDirty::acknowledge(&mut world, entity);
        assert!(world.has::<VisualDirty>(entity));
        VisualDirty::prepare(&mut world, entity);
        VisualDirty::acknowledge(&mut world, entity);
        assert!(!world.has::<VisualDirty>(entity));

        let mut regions = ExactDirtyRegions::default();
        let mut rects = Vec::new();
        regions.mark(Rect::new(0, 0, 2, 2));
        regions.prepare_into(&mut rects);
        regions.mark(Rect::new(10, 0, 2, 2));
        regions.acknowledge();
        assert!(!regions.is_empty());
        rects.clear();
        regions.prepare_into(&mut rects);
        assert_eq!(rects.len(), 2);
        regions.acknowledge();
        assert!(regions.is_empty());
    }

    #[test]
    fn affected_regions_share_summary_coverage_without_consuming_or_allocating() {
        let redraw = Rect::new(-4, 3, 8, 5);
        let shifted = Rect::new(20, 12, 30, 40);
        let plan = DirtyRegions {
            rects: alloc::vec![Rect::ZERO, redraw, Rect::new(1, 2, -1, 8)],
            shifts: alloc::vec![
                RegionShift {
                    area: shifted,
                    dx: Fixed::ZERO,
                    dy: Fixed::from_int(3)
                },
                RegionShift {
                    area: Rect::ZERO,
                    dx: Fixed::ONE,
                    dy: Fixed::ZERO
                },
            ],
            ..Default::default()
        };
        let before = (plan.rects.as_ptr(), plan.shifts.as_ptr());
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..3 {
                    assert!(plan.affected_regions().eq([redraw, shifted]));
                    assert_eq!(plan.bounding_rect(), Some(redraw.union(&shifted)));
                }
            }),
            0
        );
        assert_eq!((plan.rects.as_ptr(), plan.shifts.as_ptr()), before);
        assert_eq!((plan.rects.len(), plan.shifts.len()), (3, 2));

        let empty = DirtyRegions {
            rects: alloc::vec![Rect::ZERO],
            shifts: Vec::new(),
            ..Default::default()
        };
        assert_eq!(empty.affected_regions().next(), None);
        assert_eq!(empty.bounding_rect(), None);
    }
}
