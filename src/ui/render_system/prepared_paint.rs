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
    Unclipped,
    Clipped,
}

impl PaintVisibility {
    fn resolve(world: &World, entity: Entity) -> Self {
        if crate::ui::branch::is_effectively_hidden(world, entity) {
            Self::Hidden
        } else if crate::ui::widgets::follow_bounds::clip(world, entity).is_some() {
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
            for (index, view) in registry.paint_iter().enumerate() {
                if present.contains(index)
                    && view
                        .component_filter()
                        .is_none_or(|kind| world.has_type(entity, kind))
                {
                    let word = &row[1 + index / usize::BITS as usize];
                    word.set(word.get() | (1 << (index % usize::BITS as usize)));
                }
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

#[cfg(test)]
mod subtree_tests {
    use super::*;
    use crate::ui::layout::{LayoutNode, LayoutStyle};

    #[test]
    fn spans_follow_structure_and_survive_paint_cache_resets() {
        let mut root = LayoutNode::new(LayoutStyle::default());
        let mut branch = LayoutNode::new(LayoutStyle::default());
        branch.add_child(LayoutNode::new(LayoutStyle::default()));
        branch.add_child(LayoutNode::new(LayoutStyle::default()));
        root.add_child(branch);
        root.add_child(LayoutNode::new(LayoutStyle::default()));
        let mut prepared = PreparedPaint::with_capacity(5);
        prepared.resize(5, 0);
        prepared.prepare_subtrees(&root, true);
        assert_eq!(
            (0..5).map(|i| prepared.subtree_end(i)).collect::<Vec<_>>(),
            [Some(5), Some(4), Some(3), Some(4), Some(5)]
        );
        prepared.clear();
        prepared.resize(5, 0);
        assert_eq!(prepared.subtree_end(1), Some(4));
        root.children.swap(0, 1);
        prepared.prepare_subtrees(&root, true);
        assert_eq!(prepared.subtree_end(1), Some(2));
        assert_eq!(prepared.subtree_end(2), Some(5));
        root.children.remove(1);
        prepared.resize(2, 0);
        prepared.prepare_subtrees(&root, true);
        assert_eq!(prepared.subtree_end(0), Some(2));
        assert_eq!(prepared.subtree_end(2), None);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                prepared.prepare_subtrees(&root, false);
                prepared.clear_frame();
            }),
            0
        );
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
    if let Some(visibility) = entry.map(Cell::get)
        && visibility != PaintVisibility::Unknown
    {
        return visibility;
    }
    let visibility = PaintVisibility::resolve(world, entity);
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

    pub(super) fn contains(&self, index: usize) -> bool {
        self.words[index / usize::BITS as usize].get() & (1 << (index % usize::BITS as usize)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::prelude::*;
    use crate::types::Rect;
    use crate::types::Viewport;
    use crate::ui::view::{PaintMeasurement, SourceGeometry};

    #[crate::component]
    struct Probe {
        measurements: Cell<usize>,
        global: Cell<bool>,
    }

    #[crate::view(component = Probe)]
    fn probe_render(probe: component!(), nodes: query!(Probe), entity: entity!()) {
        if probe.global.get() {
            for _ in nodes {}
        } else {
            let _ = nodes.get(entity);
        }
    }

    fn measure(
        world: &World,
        entity: Entity,
        rect: Rect,
        _: &dyn Fn(Entity) -> Option<SourceGeometry>,
    ) -> Option<PaintMeasurement> {
        let probe = world.get::<Probe>(entity)?;
        probe.measurements.set(probe.measurements.get() + 1);
        Some(PaintMeasurement {
            coverage: PaintCoverage::solid(rect),
            workspace: None,
        })
    }

    #[crate::compose]
    fn scene() {
        ui! {
            View (id: "probe", width: 20, height: 20) [
                Probe { measurements: Cell::new(0), global: Cell::new(false) },
            ]
        };
    }

    #[test]
    fn only_observed_local_measurements_are_reused_within_a_paint_batch() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        let probe = app.world.get::<Probe>(entity).unwrap();
        let mut bytes = [0; 32 * 32 * 4];
        let texture = crate::render::texture::Texture::new(
            &mut bytes,
            32,
            32,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .unwrap();
        let mut renderer = crate::render::sw::SwRenderer::new(texture);
        for (global, reusable) in [(false, true), (true, false), (false, false)] {
            probe.global.set(global);
            super::super::render_region(
                &app.world,
                root,
                &viewport,
                &Rect::new(0, 0, 32, 32),
                &mut renderer,
            )
            .unwrap();
            probe.measurements.set(0);
            for expected in [1, 2] {
                super::super::reset_paint_preparation(&app.world);
                let requests = super::super::prepare_paint_requests(&app.world).unwrap();
                for x in [0, 8, 16] {
                    super::super::render_region_cached(
                        &app.world,
                        app.world
                            .resource::<super::super::LayoutSnapshot>()
                            .unwrap(),
                        &Rect::new(x, 0, 8, 32),
                        &mut renderer,
                        &requests,
                        None,
                    )
                    .unwrap();
                }
                assert_eq!(
                    probe.measurements.get(),
                    expected * if reusable { 1 } else { 3 },
                    "global={global}, reusable={reusable}"
                );
            }
        }
    }

    #[test]
    fn prepared_leaf_culling_preserves_overflow_and_partial_pixels_after_mutation() {
        use crate::render::sw::SwRenderer;
        use crate::render::texture::{ColorFormat, Texture};
        use crate::ui::widgets::WidgetTransform;

        #[crate::compose]
        fn overlapping_leaves() {
            ui! {
                View (width: 48, height: 32) {
                    View (width: 10, height: 10) {
                        View (
                            id: "leaf",
                            position: Position::Absolute,
                            left: 20, top: 8, width: 16, height: 16,
                            bg_color: Color::rgb(200, 20, 80)
                        )
                    }
                    View (
                        position: Position::Absolute,
                        left: 28, top: 12, width: 12, height: 12,
                        bg_color: Color::rgba(20, 160, 220, 170)
                    )
                }
            };
        }
        let mut app = App::headless(48, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, overlapping_leaves());
        app.render().unwrap();
        let entity = app.world.find_by_id("leaf").unwrap();
        let viewport = Viewport::new(48, 32, Fixed::ONE);
        for offset in [Fixed::ZERO, Fixed::HALF, Fixed::from_int(-8)] {
            app.world.insert(
                entity,
                WidgetTransform(Transform::translate(offset, Fixed::HALF)),
            );
            app.world.invalidate_visual(entity);
            app.dirty_regions().unwrap();
            let snapshot = app
                .world
                .resource::<super::super::LayoutSnapshot>()
                .unwrap();
            let position = snapshot.entities.iter().position(|&e| e == entity).unwrap();
            assert!(snapshot.prepared_paint.get(position).is_some());
            for clip in [
                Rect::new(0, 0, 8, 8),
                Rect::new(20, 8, 8, 16),
                Rect::new(28, 12, 8, 8),
                Rect::new(40, 0, 8, 32),
            ] {
                let mut cached_bytes = [0u8; 48 * 32 * 4];
                let mut fresh_bytes = cached_bytes;
                let mut cached = SwRenderer::new(
                    Texture::new(&mut cached_bytes, 48, 32, ColorFormat::RGBA8888).unwrap(),
                );
                let mut fresh = SwRenderer::new(
                    Texture::new(&mut fresh_bytes, 48, 32, ColorFormat::RGBA8888).unwrap(),
                );
                let excluded = snapshot
                    .prepared_paint
                    .get(position)
                    .is_some_and(|coverage| !super::super::rects_intersect(&coverage.outer, &clip));
                snapshot.prepared_paint.nodes[position]
                    .visibility
                    .set(PaintVisibility::Unknown);
                let requests = super::super::prepare_paint_requests(&app.world).unwrap();
                super::super::render_region_cached(
                    &app.world,
                    snapshot,
                    &clip,
                    &mut cached,
                    &requests,
                    None,
                )
                .unwrap();
                if excluded {
                    assert_eq!(
                        snapshot.prepared_paint.nodes[position].visibility.get(),
                        PaintVisibility::Unknown,
                        "an excluded leaf must not prepare visibility or clipping"
                    );
                }
                super::super::render_region(&app.world, root, &viewport, &clip, &mut fresh)
                    .unwrap();
                assert_eq!(
                    cached_bytes, fresh_bytes,
                    "offset={offset:?}, clip={clip:?}"
                );
            }
        }
    }

    #[test]
    fn successful_frames_acknowledge_the_measured_coverage() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        for (offset, full) in [(2, false), (5, true), (1, false), (0, true)] {
            app.world.get::<Probe>(entity).unwrap().measurements.set(0);
            app.world.insert(
                entity,
                crate::ui::widgets::WidgetTransform(Transform::translate(
                    Fixed::from_int(offset),
                    Fixed::ZERO,
                )),
            );
            app.world.invalidate_visual(entity);
            if full {
                app.render().unwrap();
            } else {
                app.render_dirty().unwrap();
            }
            assert_eq!(
                app.world.get::<Probe>(entity).unwrap().measurements.get(),
                1,
                "full={full}"
            );
            assert_eq!(
                app.world
                    .get::<crate::ui::dirty::PaintedCoverage>(entity)
                    .unwrap()
                    .0
                    .outer,
                Rect::new(offset, 0, 20, 20),
                "full={full}"
            );
        }
        let snapshot = app
            .world
            .resource::<super::super::LayoutSnapshot>()
            .unwrap();
        let position = snapshot.entities.iter().position(|&e| e == entity).unwrap();
        snapshot.prepared_paint.set(position, PaintCoverage::EMPTY);
        super::super::seed_prev_rects(&mut app.world, root, &Viewport::new(32, 32, Fixed::ONE));
        assert_eq!(
            app.world.get::<Probe>(entity).unwrap().measurements.get(),
            2
        );
        assert!(
            !app.world
                .get::<crate::ui::dirty::PaintedCoverage>(entity)
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[test]
    fn querying_damage_does_not_publish_or_reuse_geometry_after_mutation() {
        use crate::ui::widgets::WidgetTransform;
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        for (queried, painted) in [(2, 5), (7, 1)] {
            let previous = app
                .world
                .get::<crate::ui::dirty::PaintedCoverage>(entity)
                .unwrap()
                .0;
            app.world.insert(
                entity,
                WidgetTransform(Transform::translate(queried.into(), Fixed::ZERO)),
            );
            app.world.invalidate_visual(entity);
            super::super::collect_dirty_regions(&mut app.world, root, &viewport).unwrap();
            assert_eq!(
                app.world
                    .get::<crate::ui::dirty::PaintedCoverage>(entity)
                    .unwrap()
                    .0,
                previous
            );
            app.world.insert(
                entity,
                WidgetTransform(Transform::translate(painted.into(), Fixed::ZERO)),
            );
            app.world.invalidate_visual(entity);
            app.world.get::<Probe>(entity).unwrap().measurements.set(0);
            app.render_dirty().unwrap();
            assert_eq!(
                app.world.get::<Probe>(entity).unwrap().measurements.get(),
                1
            );
            let coverage = app
                .world
                .get::<crate::ui::dirty::PaintedCoverage>(entity)
                .unwrap()
                .0;
            assert_eq!(coverage.outer, Rect::new(painted, 0, 20, 20));
        }
    }

    #[test]
    fn offscreen_preparation_reuses_absence_and_refreshes_configuration() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let mut prepared = PreparedPaint::with_capacity(1);
        prepared.resize(1, 0);
        assert_eq!(
            core::mem::size_of::<PreparedNode>(),
            16 + core::mem::size_of::<usize>()
        );
        let configuration = |value: Option<OffscreenRender>| value.map(|v| (v.scale, v.opacity));
        for value in [
            None,
            Some(OffscreenRender::with_scale(Fixed::HALF).with_opacity(128)),
            Some(OffscreenRender::new().with_opacity(0)),
            None,
        ] {
            if let Some(value) = value {
                world.insert(entity, value);
            } else {
                world.remove::<OffscreenRender>(entity);
            }
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    prepared.clear();
                    for _ in 0..8 {
                        assert_eq!(
                            configuration(offscreen(&world, entity, 0, Some(&prepared))),
                            configuration(value)
                        );
                    }
                    assert_eq!(
                        configuration(prepared.nodes[0].offscreen.get().unwrap()),
                        configuration(value)
                    );
                }),
                0
            );
        }
    }

    #[test]
    fn offscreen_changes_after_damage_queries_are_applied_before_drawing() {
        use crate::surface::FramebufferAccess;

        for budget in [0, 32 * 1024] {
            let mut app = App::headless(32, 32);
            app.with_default_widgets()
                .with_offscreen_pool_budget(budget);
            let root = app.spawn_root().id();
            app.world.get_mut::<Style>(root).unwrap().bg_color = Some(Color::rgb(0, 0, 0).into());
            app.compose(root, scene());
            let entity = app.world.find_by_id("probe").unwrap();
            app.world.get_mut::<Style>(entity).unwrap().bg_color =
                Some(Color::rgb(255, 0, 0).into());
            app.render().unwrap();
            for opacity in [Some(0), Some(128), None] {
                app.world.invalidate_visual(entity);
                app.dirty_regions().unwrap();
                if let Some(opacity) = opacity {
                    app.world
                        .insert(entity, OffscreenRender::new().with_opacity(opacity));
                } else {
                    app.world.remove::<OffscreenRender>(entity);
                }
                app.world.invalidate_visual(entity);
                app.render_dirty().unwrap();
                let partial = app
                    .backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .bytes()
                    .to_vec();
                app.render().unwrap();
                let texture = app.backend.framebuffer();
                let full = texture.read_pixels().unwrap();
                assert_eq!(
                    partial,
                    full.bytes(),
                    "budget={budget}, opacity={opacity:?}"
                );
                if opacity == Some(0) {
                    assert_eq!(full.get_pixel(1, 1), Color::rgb(0, 0, 0));
                } else if opacity.is_none() {
                    assert_eq!(full.get_pixel(1, 1), Color::rgb(255, 0, 0));
                }
            }
        }
    }

    #[test]
    fn visibility_refreshes_with_the_batch_and_preserves_follow_clipping() {
        use crate::ui::widgets::FollowBounds;
        #[crate::compose]
        fn nodes() {
            ui! {
                View(width: 32, height: 32) {
                    View(id: "target", width: 12, height: 12)
                    View(id: "follower", border_width: 1, border_color: ColorToken::Primary) [
                        FollowBounds::new(id("target")),
                    ]
                }
            };
        }
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, nodes());
        app.render().unwrap();
        let target = app.world.find_by_id("target").unwrap();
        let follower = app.world.find_by_id("follower").unwrap();
        let mut prepared = PreparedPaint::with_capacity(2);
        prepared.resize(2, 0);
        assert_eq!(prepared.cached_visibility(0), None);
        assert_eq!(prepared.cached_visibility(2), None);
        assert_eq!(core::mem::size_of::<Cell<PaintVisibility>>(), 1);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    assert_eq!(
                        visibility(&app.world, target, 0, Some(&prepared)),
                        PaintVisibility::Unclipped
                    );
                    assert_eq!(
                        visibility(&app.world, follower, 1, Some(&prepared)),
                        PaintVisibility::Clipped
                    );
                }
            }),
            0
        );
        app.world.insert(follower, crate::ui::Hidden);
        assert_eq!(
            prepared.cached_visibility(1),
            Some(PaintVisibility::Clipped)
        );
        assert_eq!(
            visibility(&app.world, follower, 1, None),
            PaintVisibility::Hidden
        );
        prepared.clear();
        assert_eq!(prepared.cached_visibility(1), None);
        assert_eq!(
            visibility(&app.world, follower, 1, Some(&prepared)),
            PaintVisibility::Hidden
        );
        app.world.remove::<crate::ui::Hidden>(follower);
        prepared.clear();
        assert_eq!(
            visibility(&app.world, follower, 1, Some(&prepared)),
            PaintVisibility::Clipped
        );
        app.world.remove::<FollowBounds>(follower);
        prepared.clear();
        assert_eq!(
            visibility(&app.world, follower, 1, Some(&prepared)),
            PaintVisibility::Unclipped
        );
        app.world.insert(
            follower,
            crate::ui::branch::CachedBranchVisibility { selected: false },
        );
        prepared.clear();
        assert_eq!(
            visibility(&app.world, follower, 1, Some(&prepared)),
            PaintVisibility::Hidden
        );
    }

    #[test]
    fn state_preparation_preserves_priority_and_resets_without_allocating() {
        use crate::ui::state::{InteractionState, UserState};
        let mut world = World::new();
        let parent = world.spawn_empty();
        let entity = world.spawn_empty();
        world.insert(entity, crate::ui::Parent(parent));
        let mut prepared = PreparedPaint::with_capacity(1);
        prepared.resize(1, 0);
        assert_eq!(
            core::mem::size_of::<PreparedNode>(),
            16 + core::mem::size_of::<usize>()
        );
        for (ancestor_disabled, own_error, interaction, expected) in [
            (false, false, None, WidgetState::Enabled),
            (
                false,
                false,
                Some(InteractionState::Hovered),
                WidgetState::Hovered,
            ),
            (
                false,
                false,
                Some(InteractionState::Pressed),
                WidgetState::Pressed,
            ),
            (
                false,
                true,
                Some(InteractionState::Pressed),
                WidgetState::Error,
            ),
            (
                true,
                true,
                Some(InteractionState::Pressed),
                WidgetState::Disabled,
            ),
            (false, false, None, WidgetState::Enabled),
        ] {
            if ancestor_disabled {
                world.insert(parent, UserState::Disabled);
            } else {
                world.remove::<UserState>(parent);
            }
            if own_error {
                world.insert(entity, UserState::Errored);
            } else {
                world.remove::<UserState>(entity);
            }
            if let Some(interaction) = interaction {
                world.insert(entity, interaction);
            } else {
                world.remove::<InteractionState>(entity);
            }
            assert_eq!(state(&world, entity, 0, None), expected);
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    prepared.clear();
                    assert!(prepared.nodes[0].state.get().is_none());
                    for _ in 0..8 {
                        assert_eq!(state(&world, entity, 0, Some(&prepared)), expected);
                    }
                    assert_eq!(prepared.nodes[0].state.get(), Some(expected));
                }),
                0
            );
        }
        prepared.resize(1, 0);
        assert!(prepared.nodes[0].state.get().is_none());
    }

    #[test]
    fn new_app_batch_discards_state_prepared_before_mutation() {
        use crate::surface::FramebufferAccess;
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, scene());
        app.render().unwrap();
        let entity = app.world.find_by_id("probe").unwrap();
        app.world.get_mut::<Style>(entity).unwrap().bg_color = Some(ColorToken::Primary.into());
        for disabled in [true, false] {
            app.world.invalidate_visual(entity);
            app.dirty_regions().unwrap();
            let snapshot = app
                .world
                .resource::<super::super::LayoutSnapshot>()
                .unwrap();
            let position = snapshot
                .entities
                .iter()
                .position(|&node| node == entity)
                .unwrap();
            state(&app.world, entity, position, Some(&snapshot.prepared_paint));
            if disabled {
                app.world
                    .insert(root, crate::ui::state::UserState::Disabled);
            } else {
                app.world.remove::<crate::ui::state::UserState>(root);
            }
            app.render_dirty().unwrap();
            let expected = if disabled {
                WidgetState::Disabled
            } else {
                WidgetState::Enabled
            };
            assert_eq!(
                app.backend
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(1, 1),
                app.world
                    .resource::<crate::ui::Theme>()
                    .unwrap()
                    .resolve_in(ColorToken::Primary, expected),
            );
            let snapshot = app
                .world
                .resource::<super::super::LayoutSnapshot>()
                .unwrap();
            assert_eq!(
                state(&app.world, entity, position, Some(&snapshot.prepared_paint)),
                if disabled {
                    WidgetState::Disabled
                } else {
                    WidgetState::Enabled
                }
            );
        }
    }

    #[test]
    fn affine_preparation_resets_between_batches_and_preserves_projective_composition() {
        use crate::ui::widgets::{WidgetTransform, WidgetTransform3D};
        let mut world = World::new();
        let entity = world.spawn_empty();
        let mut prepared = PreparedPaint::with_capacity(1);
        prepared.resize(1, 0);
        let rect = Rect::new(7, 11, 29, 31);
        let parent = Transform::rotate_deg(Fixed::from_int(19));
        for local in [
            Transform::translate(Fixed::HALF, Fixed::from_int(-3)),
            Transform::scale(Fixed::HALF, Fixed::from_int(2)),
        ] {
            world.insert(entity, WidgetTransform(local));
            prepared.clear();
            let expected = crate::ui::geometry::render_transforms(
                parent,
                Transform3D::IDENTITY,
                &world,
                entity,
                rect,
            );
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    for _ in 0..16 {
                        assert_eq!(
                            transforms(
                                &parent,
                                &Transform3D::IDENTITY,
                                &world,
                                entity,
                                rect,
                                0,
                                Some(&prepared)
                            ),
                            expected
                        );
                    }
                }),
                0
            );
            assert_eq!(prepared.affine[0].get(), Some(expected.0));
        }
        prepared.clear();
        world.insert(
            entity,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::from_int(35),
                Fixed::from_int(100),
            )),
        );
        let expected = crate::ui::geometry::render_transforms(
            parent,
            Transform3D::IDENTITY,
            &world,
            entity,
            rect,
        );
        assert_eq!(
            transforms(
                &parent,
                &Transform3D::IDENTITY,
                &world,
                entity,
                rect,
                0,
                Some(&prepared)
            ),
            expected
        );
        assert!(prepared.affine[0].get().is_none());
        assert_eq!(core::mem::size_of::<Cell<Option<Transform>>>(), 28);
    }

    #[test]
    fn successful_frames_retain_membership_and_refresh_component_changes() {
        let mut app = App::headless(32, 32);
        app.with_default_widgets();
        app.with_widget(probe_render::view().with_measurement(measure));
        let root = app.spawn_root().id();
        app.compose(root, scene());
        let entity = app.world.find_by_id("probe").unwrap();
        app.render().unwrap();
        for has_probe in [true, false, true] {
            if has_probe && !app.world.has::<Probe>(entity) {
                app.world.insert(
                    entity,
                    Probe {
                        measurements: Cell::new(0),
                        global: Cell::new(false),
                    },
                );
            } else if !has_probe {
                app.world.remove::<Probe>(entity);
            }
            app.world.invalidate_visual(entity);
            app.render_dirty().unwrap();
            let snapshot = app
                .world
                .resource::<super::super::LayoutSnapshot>()
                .unwrap();
            let prepared = &snapshot.prepared_paint;
            let position = snapshot
                .entities
                .iter()
                .position(|&node| node == entity)
                .unwrap();
            let stride = prepared.view_stride();
            assert_eq!(
                prepared.views[0].get(),
                1,
                "type presence survives acknowledgement"
            );
            let row = &prepared.views[(position + 1) * stride..(position + 2) * stride];
            assert_eq!(row[0].get(), 1, "node membership survives acknowledgement");
            let matching = PreparedViews { words: &row[1..] };
            let registry = app.world.resource::<ViewRegistry>().unwrap();
            for (index, view) in registry.paint_iter().enumerate() {
                assert_eq!(
                    matching.contains(index),
                    view.component_filter()
                        .is_none_or(|kind| app.world.has_type(entity, kind))
                );
            }
        }
    }

    #[test]
    fn view_membership_spans_words_and_refreshes_between_batches() {
        use crate::ui::view::View;
        struct Absent;
        impl crate::ecs::Component for Absent {}
        let mut world = World::new();
        let entity = world.spawn_empty();
        let other = world.spawn_empty();
        world.insert(other, crate::ui::Style::default());
        world.insert(
            entity,
            Probe {
                measurements: Cell::new(0),
                global: Cell::new(false),
            },
        );
        let mut registry = ViewRegistry::default();
        for index in 0..usize::BITS as usize * 2 + 3 {
            let view = View::new("probe", (index % 7) as u8, |_, _, _| {});
            registry.insert(match index % 4 {
                0 => view.with_filter::<Probe>(),
                1 => view.with_filter::<crate::ui::Style>(),
                2 => view.with_filter::<Absent>(),
                _ => view,
            });
        }
        let mut prepared = PreparedPaint::with_capacity(2);
        prepared.resize(2, registry.paint_iter().len());
        let check = |prepared: &PreparedPaint, world: &World, registry: &ViewRegistry| {
            for (position, entity) in [entity, other].into_iter().enumerate() {
                let matching = prepared
                    .matching_views(world, registry, entity, position)
                    .unwrap();
                let mut next = matching.next_index(0);
                for (index, view) in registry.paint_iter().enumerate() {
                    assert_eq!(
                        matching.contains(index),
                        view.component_filter()
                            .is_none_or(|kind| world.has_type(entity, kind))
                    );
                    if matching.contains(index) {
                        assert_eq!(next, Some(index));
                        assert!(core::ptr::eq(registry.paint_at(index).unwrap(), view));
                        next = matching.next_index(index + 1);
                    }
                }
                assert_eq!(next, None);
                assert_eq!(matching.next_index(usize::MAX), None);
            }
        };
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                check(&prepared, &world, &registry);
                check(&prepared, &world, &registry);
            }),
            0
        );
        world.remove::<Probe>(entity);
        world.insert(entity, crate::ui::Style::default());
        prepared.clear();
        check(&prepared, &world, &registry);
        world.insert(other, Absent);
        prepared.clear();
        check(&prepared, &world, &registry);
        registry.insert(View::new("extra", 2, |_, _, _| {}).with_filter::<crate::ui::Style>());
        assert!(
            prepared
                .matching_views(&world, &registry, entity, 0)
                .is_none()
        );
        prepared.resize(2, registry.paint_iter().len());
        check(&prepared, &world, &registry);
    }

    #[test]
    fn batch_reset_and_retained_node_capacity_do_not_allocate() {
        let mut prepared = PreparedPaint::with_capacity(8);
        prepared.resize(4, 17);
        let coverage = PaintCoverage::solid(Rect::new(1, 2, 3, 4));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..16 {
                    prepared.resize(8, 17);
                    prepared.set(7, coverage);
                    assert_eq!(prepared.get(7), Some(coverage));
                    prepared.clear();
                    assert_eq!(prepared.get(7), None);
                    prepared.resize(4, 17);
                    prepared.resize(8, 17);
                    assert_eq!(prepared.get(7), None);
                }
            }),
            0
        );
        assert_eq!(core::mem::size_of::<Cell<Option<PaintCoverage>>>(), 36);
    }
}
