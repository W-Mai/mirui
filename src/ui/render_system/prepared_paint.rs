use crate::ecs::{Entity, World};
use crate::types::{Rect, Transform, Transform3D};
use crate::ui::OffscreenRender;
use crate::ui::theme::WidgetState;
use crate::ui::view::ViewRegistry;
use alloc::vec::Vec;
use core::cell::Cell;

use super::PaintCoverage;

pub(super) struct PreparedPaint {
    coverage: Vec<Cell<Option<PaintCoverage>>>,
    // The first row records type presence; remaining rows record node membership.
    // Both are invalidated together at the existing paint-batch boundary.
    views: Vec<Cell<usize>>,
    view_count: usize,
    view_revisions: Vec<Cell<ViewMembership>>,
    affine: Vec<Cell<Option<Transform>>>,
    nodes: Vec<PreparedNode>,
}

#[derive(Clone, Copy)]
struct ViewMembership {
    revision: u64,
    // A checked lookup hint, not an identity or an invalidation version.
    watch_index: usize,
}

#[derive(Default)]
struct PreparedNode {
    subtree_end: usize,
    visibility: Cell<PaintVisibility>,
    state: Cell<Option<WidgetState>>,
    offscreen: Cell<Option<Option<OffscreenRender>>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PaintVisibility {
    #[default]
    Unknown,
    Hidden,
    // Visibility is known; clipping remains lazy.
    Visible,
    Unclipped,
    Clipped,
}

impl PaintVisibility {
    fn resolve(world: &World, entity: Entity) -> Self {
        if crate::ui::branch::is_effectively_hidden(world, entity) {
            Self::Hidden
        } else {
            Self::resolve_clip(world, entity)
        }
    }

    fn resolve_clip(world: &World, entity: Entity) -> Self {
        if crate::ui::widgets::follow_bounds::clip(world, entity).is_some() {
            Self::Clipped
        } else {
            Self::Unclipped
        }
    }
}

impl PreparedPaint {
    pub(super) fn prepare_subtrees(
        &mut self,
        tree: &crate::ui::layout::LayoutNode,
        structure_changed: bool,
    ) {
        if !structure_changed && self.subtree_end(0) == Some(self.nodes.len()) {
            return;
        }
        fn visit(
            node: &crate::ui::layout::LayoutNode,
            entries: &mut [PreparedNode],
            index: &mut usize,
        ) {
            let start = *index;
            *index += 1;
            for child in &node.children {
                visit(child, entries, index);
            }
            entries[start].subtree_end = *index;
        }
        visit(tree, &mut self.nodes, &mut 0);
    }

    pub(super) fn subtree_end(&self, index: usize) -> Option<usize> {
        let end = self.nodes.get(index)?.subtree_end;
        (end > index && end <= self.nodes.len()).then_some(end)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            coverage: Vec::with_capacity(capacity),
            views: Vec::new(),
            view_count: 0,
            view_revisions: Vec::new(),
            affine: Vec::with_capacity(capacity),
            nodes: Vec::with_capacity(capacity),
        }
    }

    pub(super) fn resize(&mut self, len: usize, view_count: usize) {
        self.coverage.resize_with(len, || Cell::new(None));
        self.affine.resize_with(len, || Cell::new(None));
        self.nodes.resize_with(len, PreparedNode::default);
        self.view_count = view_count;
        self.view_revisions.resize_with(view_count, || {
            Cell::new(ViewMembership {
                revision: 0,
                watch_index: usize::MAX,
            })
        });
        let view_capacity = (self.coverage.capacity() + 1) * self.view_stride();
        if self.views.capacity() < view_capacity {
            self.views.reserve_exact(view_capacity - self.views.len());
        }
        self.views
            .resize_with((len + 1) * self.view_stride(), || Cell::new(0));
        self.clear();
    }

    pub(super) fn clear(&self) {
        self.clear_frame();
        for entry in &self.views {
            entry.set(0);
        }
    }

    pub(super) fn clear_frame(&self) {
        self.clear_coverage();
        for entry in &self.affine {
            entry.set(None);
        }
        for entry in &self.nodes {
            entry.visibility.set(PaintVisibility::Unknown);
            entry.state.set(None);
            entry.offscreen.set(None);
        }
    }

    pub(super) fn clear_for_frame(&self, world: &World, registry: &ViewRegistry) {
        self.clear_frame();
        if registry.paint_iter().len() != self.view_count {
            for entry in &self.views {
                entry.set(0);
            }
            return;
        }
        let mut membership_changed = false;
        for (index, view) in registry.paint_iter().enumerate() {
            let previous = self.view_revisions[index].get();
            let (watch_index, revision) =
                view.component_filter().map_or((usize::MAX, 0), |type_id| {
                    world.component_membership_revision_by_id(type_id, previous.watch_index)
                });
            self.view_revisions[index].set(ViewMembership {
                revision,
                watch_index,
            });
            membership_changed |= previous.revision != revision;
        }
        if membership_changed {
            for entry in &self.views {
                entry.set(0);
            }
        }
    }

    pub(super) fn clear_coverage(&self) {
        for entry in &self.coverage {
            entry.set(None);
        }
    }

    fn view_stride(&self) -> usize {
        1 + self.view_count.div_ceil(usize::BITS as usize)
    }

    pub(super) fn matching_views(
        &self,
        world: &World,
        registry: &ViewRegistry,
        entity: Entity,
        position: usize,
    ) -> Option<PreparedViews<'_>> {
        if self.view_count != registry.paint_iter().len() {
            return None;
        }
        let stride = self.view_stride();
        let present = self.views.get(..stride)?;
        let row = self
            .views
            .get((position + 1) * stride..(position + 2) * stride)?;
        if present[0].get() == 0 {
            for (index, view) in registry.paint_iter().enumerate() {
                if view
                    .component_filter()
                    .is_none_or(|kind| world.has_any_by_id(kind))
                {
                    let word = &present[1 + index / usize::BITS as usize];
                    word.set(word.get() | (1 << (index % usize::BITS as usize)));
                }
            }
            present[0].set(1);
        }
        if row[0].get() == 0 {
            let present = PreparedViews {
                words: &present[1..],
            };
            let mut next = present.next_index(0);
            while let Some(index) = next {
                let view = registry.paint_at(index)?;
                if view
                    .component_filter()
                    .is_none_or(|kind| world.has_type(entity, kind))
                {
                    let word = &row[1 + index / usize::BITS as usize];
                    word.set(word.get() | (1 << (index % usize::BITS as usize)));
                }
                next = present.next_index(index + 1);
            }
            row[0].set(1);
        }
        Some(PreparedViews { words: &row[1..] })
    }

    pub(super) fn get(&self, position: usize) -> Option<PaintCoverage> {
        self.coverage.get(position).and_then(Cell::get)
    }

    pub(super) fn cached_visibility(&self, position: usize) -> Option<PaintVisibility> {
        let visibility = self.nodes.get(position)?.visibility.get();
        (visibility != PaintVisibility::Unknown).then_some(visibility)
    }

    pub(super) fn set(&self, position: usize, coverage: PaintCoverage) {
        if let Some(entry) = self.coverage.get(position) {
            entry.set(Some(coverage));
        }
    }
}

pub(super) fn hidden(
    world: &World,
    entity: Entity,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> bool {
    let entry = preparation
        .and_then(|prepared| prepared.nodes.get(position))
        .map(|node| &node.visibility);
    match entry.map(Cell::get) {
        Some(PaintVisibility::Hidden) => true,
        Some(PaintVisibility::Visible | PaintVisibility::Unclipped | PaintVisibility::Clipped) => {
            false
        }
        Some(PaintVisibility::Unknown) | None => {
            let hidden = crate::ui::branch::is_effectively_hidden(world, entity);
            if let Some(entry) = entry {
                entry.set(if hidden {
                    PaintVisibility::Hidden
                } else {
                    PaintVisibility::Visible
                });
            }
            hidden
        }
    }
}

pub(super) fn visibility(
    world: &World,
    entity: Entity,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> PaintVisibility {
    let entry = preparation
        .and_then(|prepared| prepared.nodes.get(position))
        .map(|node| &node.visibility);
    let visibility = match entry.map(Cell::get) {
        Some(PaintVisibility::Unknown) | None => PaintVisibility::resolve(world, entity),
        Some(PaintVisibility::Visible) => PaintVisibility::resolve_clip(world, entity),
        Some(visibility) => return visibility,
    };
    if let Some(entry) = entry {
        entry.set(visibility);
    }
    visibility
}

pub(super) fn state(
    world: &World,
    entity: Entity,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> WidgetState {
    let entry = preparation
        .and_then(|prepared| prepared.nodes.get(position))
        .map(|node| &node.state);
    if let Some(state) = entry.and_then(Cell::get) {
        return state;
    }
    let state = super::resolve_widget_state(world, entity);
    if let Some(entry) = entry {
        entry.set(Some(state));
    }
    state
}

pub(super) fn offscreen(
    world: &World,
    entity: Entity,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> Option<OffscreenRender> {
    let entry = preparation
        .and_then(|prepared| prepared.nodes.get(position))
        .map(|node| &node.offscreen);
    if let Some(config) = entry.and_then(Cell::get) {
        return config;
    }
    let config = crate::ui::offscreen::config(world, entity);
    if let Some(entry) = entry {
        entry.set(Some(config));
    }
    config
}

pub(super) fn transforms(
    parent_affine: &Transform,
    parent_projective: &Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
    position: usize,
    preparation: Option<&PreparedPaint>,
) -> (Transform, Transform3D) {
    let entry = preparation.and_then(|prepared| prepared.affine.get(position));
    if let Some(affine) = entry.and_then(Cell::get) {
        return (affine, Transform3D::IDENTITY);
    }
    let (affine, projective) =
        super::render_transforms(*parent_affine, *parent_projective, world, entity, rect);
    if projective.is_identity()
        && let Some(entry) = entry
    {
        entry.set(Some(affine));
    }
    (affine, projective)
}

pub(super) struct PreparedViews<'a> {
    words: &'a [Cell<usize>],
}

impl PreparedViews<'_> {
    pub(super) fn next_index(&self, start: usize) -> Option<usize> {
        let bits = usize::BITS as usize;
        let mut word = start / bits;
        let mut available = self.words.get(word)?.get() & (usize::MAX << (start % bits));
        loop {
            if available != 0 {
                return Some(word * bits + available.trailing_zeros() as usize);
            }
            word += 1;
            available = self.words.get(word)?.get();
        }
    }

    #[cfg(test)]
    fn contains(&self, index: usize) -> bool {
        self.words[index / usize::BITS as usize].get() & (1 << (index % usize::BITS as usize)) != 0
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/ui/render_system/prepared_paint/subtree_tests.rs"]
mod subtree_tests;
#[cfg(test)]
#[path = "../../../tests/unit/ui/render_system/prepared_paint/tests.rs"]
mod tests;
