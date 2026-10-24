use crate::core::reactive::PaintRequestScope;
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::{DrawRequest, FallbackRegion, RenderError, RenderRoute, Renderer};
use crate::render::texture::Texture;
use crate::render::{BackendTarget, PlaneLayout, TargetPlan};
use crate::text::TextGeometry;
use crate::types::{Dimension, Fixed, Point, Rect, Transform, Transform3D, Viewport};
use crate::ui::layout::{LayoutNode, LayoutStyle, compute_layout};
#[cfg(test)]
use crate::ui::widgets::transform::WidgetTransform;
#[cfg(test)]
use crate::ui::widgets::transform_3d::WidgetTransform3D;
use alloc::boxed::Box;
use alloc::vec::Vec;

use super::dirty::{DirtyRegions, PaintCoverage, RegionShift};
#[cfg(test)]
use super::geometry::{effective_transform, effective_transform_3d};
use super::geometry::{render_transforms, transformed_clip_bounds, widget_render_geometry};
use super::state::{InteractionState, UserState};
use super::theme::WidgetState;
use super::view::{FrameRequest, PaintCtx, ViewRegistry};
use super::{Children, Parent, Style, Widget};
use super::{Outputs, WidgetTexture};

pub(crate) mod output_plan;
mod prepared_paint;
mod scroll;

use prepared_paint::PreparedPaint;

pub(crate) use scroll::has_pending_scroll;

#[cfg(test)]
mod coverage_tests;
#[cfg(test)]
mod native_target_tests;
#[cfg(test)]
mod overflow_tests;
#[cfg(test)]
mod paint_request_tests;
#[cfg(test)]
mod transform_cache_tests;

/// Stage at which a text widget failed to prepare its layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutStage {
    Measure,
    Layout,
}

/// Cause of a text preparation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextLayoutFailureKind {
    Text(crate::text::TextLayoutError),
    FontStackCapacity { required: usize, limit: usize },
    PathBaselineBudget { required: usize, budget: usize },
    PathBaselineAllocation,
    PathBaseline,
    MissingLayout,
    OutputPublication,
}

/// The text widget and operation that stopped a layout pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextLayoutFailure {
    pub entity: Entity,
    pub stage: TextLayoutStage,
    pub kind: TextLayoutFailureKind,
}

impl TextLayoutFailure {
    pub(crate) const fn text(
        entity: Entity,
        stage: TextLayoutStage,
        error: crate::text::TextLayoutError,
    ) -> Self {
        Self {
            entity,
            stage,
            kind: TextLayoutFailureKind::Text(error),
        }
    }

    pub(crate) const fn font_stack(
        entity: Entity,
        stage: TextLayoutStage,
        required: usize,
        limit: usize,
    ) -> Self {
        Self {
            entity,
            stage,
            kind: TextLayoutFailureKind::FontStackCapacity { required, limit },
        }
    }

    const fn baseline(
        entity: Entity,
        stage: TextLayoutStage,
        error: crate::text::baseline::PathBaselineError,
    ) -> Self {
        use crate::text::baseline::PathBaselineError;

        let kind = match error {
            PathBaselineError::CacheBudget { required, budget } => {
                TextLayoutFailureKind::PathBaselineBudget { required, budget }
            }
            PathBaselineError::Allocation => TextLayoutFailureKind::PathBaselineAllocation,
            _ => TextLayoutFailureKind::PathBaseline,
        };
        Self {
            entity,
            stage,
            kind,
        }
    }
}

#[cfg(test)]
mod text_layout_failure_tests {
    use super::{TextLayoutFailure, TextLayoutFailureKind, TextLayoutStage};
    use crate::ecs::Entity;
    use crate::text::baseline::PathBaselineError;

    #[test]
    fn path_baseline_capacity_diagnostics_keep_the_required_budget() {
        let entity = Entity {
            id: 1,
            generation: 0,
        };
        let failure = TextLayoutFailure::baseline(
            entity,
            TextLayoutStage::Layout,
            PathBaselineError::CacheBudget {
                required: 256,
                budget: 128,
            },
        );
        assert_eq!(
            failure.kind,
            TextLayoutFailureKind::PathBaselineBudget {
                required: 256,
                budget: 128,
            }
        );
        assert_eq!(
            TextLayoutFailure::baseline(
                entity,
                TextLayoutStage::Measure,
                PathBaselineError::Allocation,
            )
            .kind,
            TextLayoutFailureKind::PathBaselineAllocation,
        );
    }
}

pub(crate) struct ScopedRenderer<'a> {
    physical_clip: Option<crate::types::PhysicalRect>,
    inner: &'a mut dyn Renderer,
    transform: Transform3D,
}

impl<'a> ScopedRenderer<'a> {
    pub(crate) fn new(
        inner: &'a mut dyn Renderer,
        transform: Transform3D,
        physical_clip: Option<crate::types::PhysicalRect>,
    ) -> Self {
        Self {
            inner,
            transform,
            physical_clip,
        }
    }

    fn request<'cmd, 'data>(&self, request: &DrawRequest<'cmd, 'data>) -> DrawRequest<'cmd, 'data> {
        let physical_clip = match (self.physical_clip, request.physical_clip) {
            (Some(left), Some(right)) => Some(
                left.intersection(right)
                    .unwrap_or(crate::types::PhysicalRect::EMPTY),
            ),
            (left, right) => left.or(right),
        };
        DrawRequest {
            projective: self.transform.compose(&request.projective),
            physical_clip,
            ..*request
        }
    }
}

impl Renderer for ScopedRenderer<'_> {
    type TargetState = ();

    fn poll_draw_request(
        &mut self,
        request: DrawRequest<'_, '_>,
        text_run: Option<crate::render::TextRunIdentity>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<(), RenderError>> {
        self.inner
            .poll_draw_request(self.request(&request), text_run, cx)
    }

    fn plan_target(&self, layout: PlaneLayout) -> Result<Option<TargetPlan>, RenderError> {
        self.inner.plan_target(layout)
    }

    fn can_reuse_target(
        &self,
        target: &dyn BackendTarget,
        plan: &TargetPlan,
    ) -> Result<bool, RenderError> {
        self.inner.can_reuse_target(target, plan)
    }

    fn create_target(&mut self, plan: &TargetPlan) -> Result<Box<dyn BackendTarget>, RenderError> {
        self.inner.create_target(plan)
    }

    fn with_target(
        &mut self,
        target: &mut dyn BackendTarget,
        viewport: Viewport,
        update: crate::render::TargetUpdate,
        workspace: &mut [u8],
        body: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        self.inner
            .with_target(target, viewport, update, workspace, body)
    }

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        self.inner.route(&self.request(request))
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        self.inner.submit(draw)
    }

    fn submit_text_run(
        &mut self,
        draw: &::mirui::render::PreparedDraw<'_>,
        identity: crate::render::renderer::TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.inner.submit_text_run(draw, identity)
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<crate::render::PreparedDraw<'data>, RenderError>> {
        self.inner.prepare(self.request(&request), route, cx)
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        self.inner.flush()
    }

    fn output_scale(&self) -> Fixed {
        self.inner.output_scale()
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        self.inner.plan_scope(bounds)
    }

    fn render_scope(
        &mut self,
        region: FallbackRegion,
        draw: &mut dyn FnMut(&mut dyn Renderer) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        let transform = self.transform;
        let physical_clip = self.physical_clip;
        self.inner.render_scope(region, &mut |local| {
            let mut projected = ScopedRenderer::new(local, transform, physical_clip);
            draw(&mut projected)
        })
    }

    fn prepare_readback(&mut self, src: &Rect) -> Result<(), RenderError> {
        self.inner.prepare_readback(src)
    }

    fn read_target_region(&self, src: &Rect, dst: &mut Texture) -> Result<(), RenderError> {
        self.inner.read_target_region(src, dst)
    }

    fn sample_target_region(&self, src: &Rect) -> Result<Option<Texture<'static>>, RenderError> {
        self.inner.sample_target_region(src)
    }
}

/// Disabled (subtree) > Errored (self) > Pressed > Hovered > Enabled.
fn resolve_widget_state(world: &World, entity: Entity) -> WidgetState {
    let mut cur = Some(entity);
    while let Some(e) = cur {
        if matches!(world.get::<UserState>(e), Some(UserState::Disabled)) {
            return WidgetState::Disabled;
        }
        cur = world.get::<Parent>(e).map(|p| p.0);
    }
    if matches!(world.get::<UserState>(entity), Some(UserState::Errored)) {
        return WidgetState::Error;
    }
    match world.get::<InteractionState>(entity) {
        Some(InteractionState::Pressed) => WidgetState::Pressed,
        Some(InteractionState::Hovered) => WidgetState::Hovered,
        None => WidgetState::Enabled,
    }
}

fn quad_bbox(q: [Point; 4]) -> Rect {
    Rect::bounding_quad(&q)
}

fn affine_visual_bounds(
    world: &World,
    entity: Entity,
    rect: Rect,
    transform: Transform,
    output_scale: Fixed,
) -> Rect {
    let layout = transform.apply_rect_bbox(style_paint_bounds(world, entity, rect));
    crate::ui::widgets::text::path_text_ink_bounds(world, entity, rect, transform, output_scale)
        .map(|ink| layout.union(&ink))
        .unwrap_or(layout)
}

fn projective_visual_bounds(
    world: &World,
    entity: Entity,
    rect: Rect,
    transform: Transform3D,
    output_scale: Fixed,
) -> Option<Rect> {
    let layout = transform
        .apply_rect(style_paint_bounds(world, entity, rect))
        .map(quad_bbox)?;
    let ink = crate::ui::widgets::text::path_text_ink_bounds(
        world,
        entity,
        rect,
        Transform::IDENTITY,
        output_scale,
    )
    .and_then(|bounds| transform.apply_rect(bounds))
    .map(quad_bbox);
    Some(ink.map(|bounds| layout.union(&bounds)).unwrap_or(layout))
}

fn style_paint_bounds(world: &World, entity: Entity, rect: Rect) -> Rect {
    let Some(style) = world.get::<Style>(entity) else {
        return rect;
    };
    if style.border_color.is_none() || style.border_width <= Fixed::ZERO {
        return rect;
    }
    rect.inflate(style.border_width / Fixed::from_int(2))
}

fn visual_bounds(
    world: &World,
    entity: Entity,
    rect: Rect,
    affine: Transform,
    projective: Transform3D,
    output_scale: Fixed,
) -> Rect {
    let bounds = if projective.is_identity() {
        affine_visual_bounds(world, entity, rect, affine, output_scale)
    } else {
        projective_visual_bounds(world, entity, rect, projective, output_scale).unwrap_or(rect)
    };
    if let Some(blur) = world.get::<crate::ui::widgets::BackgroundBlur>(entity) {
        let padding = blur.radius + blur.spread;
        if padding > Fixed::ZERO {
            return bounds.inflate(padding);
        }
    }
    bounds
}

#[allow(clippy::too_many_arguments)]
fn planned_visual_bounds(
    world: &World,
    entity: Entity,
    rect: Rect,
    affine: Transform,
    projective: Transform3D,
    output_scale: Fixed,
    position: usize,
    plan: Option<&output_plan::TexturePlan>,
    preparation: Option<&PreparedPaint>,
) -> Rect {
    visible_paint_coverage(
        world,
        entity,
        rect,
        affine,
        projective,
        output_scale,
        position,
        plan,
        preparation,
    )
    .outer
}

#[allow(clippy::too_many_arguments)]
fn visible_paint_coverage(
    world: &World,
    entity: Entity,
    rect: Rect,
    affine: Transform,
    projective: Transform3D,
    output_scale: Fixed,
    position: usize,
    plan: Option<&output_plan::TexturePlan>,
    preparation: Option<&PreparedPaint>,
) -> PaintCoverage {
    let preparation = preparation.filter(|_| plan.is_none());
    if let Some(coverage) = preparation.and_then(|prepared| prepared.get(position)) {
        return coverage;
    }
    let fallback = || {
        PaintCoverage::solid(visual_bounds(
            world,
            entity,
            rect,
            affine,
            projective,
            output_scale,
        ))
    };
    if !projective.is_identity()
        || affine.m00 != Fixed::ONE
        || affine.m11 != Fixed::ONE
        || affine.m01 != Fixed::ZERO
        || affine.m10 != Fixed::ZERO
    {
        return plan
            .and_then(|plan| plan.local_paint_bounds(position, rect))
            .and_then(|extra| transformed_clip_bounds(extra, affine, projective))
            .map_or_else(fallback, |extra| {
                fallback().union(PaintCoverage::solid(extra))
            });
    }
    let Some(registry) = world.resource::<ViewRegistry>() else {
        return fallback();
    };
    let mut coverage = PaintCoverage::EMPTY;
    let mut reusable = true;
    for view in prepared_paint::paint_views(world, registry, entity, position, preparation) {
        reusable &= view.measurement_reusable();
        let measured = view.measurement().and_then(|measure| {
            measure(world, entity, rect, &|source| {
                plan.and_then(|plan| plan.output_layout(source))
            })
        });
        let next = match measured {
            Some(measured) if measured.coverage.is_empty() => PaintCoverage::EMPTY,
            Some(measured) if output_scale < Fixed::ONE => {
                PaintCoverage::solid(affine.apply_rect_bbox(measured.coverage.outer))
            }
            Some(measured) => PaintCoverage::frame(
                affine.apply_rect_bbox(measured.coverage.outer),
                affine.apply_rect_bbox(measured.coverage.excluded),
            ),
            None => fallback(),
        };
        coverage = coverage.union(next);
    }
    if let Some(capture) = plan.and_then(|plan| plan.captured_paint_bounds(position, rect)) {
        coverage = coverage.union(PaintCoverage::solid(affine.apply_rect_bbox(capture)));
    }
    if reusable && let Some(preparation) = preparation {
        preparation.set(position, coverage);
    }
    coverage
}

#[allow(clippy::too_many_arguments)]
fn seed_prev_rect_walk(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    parent_transform: &Transform,
    parent_3d: &Transform3D,
    output_scale: Fixed,
    scroll_offset: (Fixed, Fixed),
    scroll_clip: Option<Rect>,
    parent_scroll: Option<Entity>,
    scroll_pixels: bool,
    ancestors_visible: bool,
    output_plan: Option<&output_plan::TexturePlan>,
    preparation: Option<&PreparedPaint>,
) {
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let visible = ancestors_visible && !super::branch::is_effectively_hidden(world, entity);
    let shifted_rect = Rect {
        x: node.rect.x - scroll_offset.0,
        y: node.rect.y - scroll_offset.1,
        ..node.rect
    };
    let (tf, tf_3d) = prepared_paint::transforms(
        *parent_transform,
        *parent_3d,
        world,
        entity,
        shifted_rect,
        *idx,
        preparation,
    );
    let current_scroll = scroll::PaintedScroll::capture(
        world,
        entity,
        shifted_rect,
        scroll_clip,
        tf,
        tf_3d,
        parent_scroll,
    );
    if let Some(mut painted) = current_scroll {
        painted.reusable &= scroll_pixels;
        world.insert(entity, painted);
    } else {
        world.remove::<scroll::PaintedScroll>(entity);
    }
    let mut coverage = if visible {
        preparation
            .and_then(|prepared| prepared.get(*idx))
            .unwrap_or_else(|| {
                visible_paint_coverage(
                    world,
                    entity,
                    shifted_rect,
                    tf,
                    tf_3d,
                    output_scale,
                    *idx,
                    output_plan,
                    None,
                )
            })
    } else {
        PaintCoverage::EMPTY
    };
    coverage = inflated_coverage(world, entity, shifted_rect, coverage);
    if let Some(clip) = scroll_clip {
        coverage = coverage.clipped(clip);
    }
    let coverage = coverage.pixel_aligned();
    if let Some(painted) = world.get_mut::<super::dirty::PaintedCoverage>(entity) {
        painted.0 = coverage;
    } else {
        world.insert(entity, super::dirty::PaintedCoverage(coverage));
    }
    super::dirty::VisualDirty::acknowledge(world, entity);
    *idx += 1;
    if node.children.is_empty() {
        return;
    }
    let child_scroll = current_scroll.map_or(scroll_offset, |scroll| {
        (
            scroll_offset.0 + scroll.offset.x,
            scroll_offset.1 + scroll.offset.y,
        )
    });
    let is_scroll = current_scroll.is_some();
    let child_clip = if is_scroll
        || world
            .get::<Style>(entity)
            .is_some_and(|style| style.clip_children)
    {
        let area = transformed_clip_bounds(shifted_rect, tf, tf_3d).unwrap_or(Rect::ZERO);
        Some(scroll_clip.map_or(area, |clip| area.intersect(&clip).unwrap_or(Rect::ZERO)))
    } else {
        scroll_clip
    };
    let child_scroll_pixels = scroll_pixels && scroll::children_are_transferable(world, entity);
    for child in &node.children {
        seed_prev_rect_walk(
            child,
            world,
            entities,
            idx,
            &tf,
            &tf_3d,
            output_scale,
            child_scroll,
            child_clip,
            if is_scroll {
                Some(entity)
            } else {
                parent_scroll
            },
            child_scroll_pixels,
            visible,
            output_plan,
            preparation,
        );
    }
}

/// Record paint bounds and acknowledge prepared damage after successfully drawing the full pending plan, including descendants moved by scroll transfers. Call before subsequent state changes; collecting damage does not advance these bounds.
pub fn seed_prev_rects(world: &mut World, root: Entity, transform: &Viewport) {
    acknowledge_paint(world, root, transform, false);
}

pub(crate) fn acknowledge_painted_frame(world: &mut World, root: Entity, transform: &Viewport) {
    acknowledge_paint(world, root, transform, true);
}

fn acknowledge_paint(world: &mut World, root: Entity, transform: &Viewport, prepared: bool) {
    scroll::PaintedScroll::invalidate_all(world);
    let retired_paint = world
        .resource::<super::dirty::ExactDirtyRegions>()
        .is_some_and(|regions| !regions.is_empty());
    if let Some(regions) = world.resource_mut::<super::dirty::ExactDirtyRegions>() {
        regions.acknowledge();
    }
    let (logical_w, logical_h) = transform.logical_size();
    if let Some(snapshot) = world.take_resource_box::<LayoutSnapshot>() {
        if snapshot.matches(root, logical_w, logical_h) {
            if retired_paint {
                for node in &snapshot.parked {
                    if let Some(entity) = node.entity {
                        world.clear_subtree_paint(entity);
                    }
                }
            }
            let output_plan = world
                .resource::<Outputs<WidgetTexture>>()
                .and_then(Outputs::take_texture_plan);
            let mut idx = 0;
            seed_prev_rect_walk(
                &snapshot.layout_tree,
                world,
                &snapshot.entities,
                &mut idx,
                &Transform::IDENTITY,
                &Transform3D::IDENTITY,
                transform.scale(),
                (Fixed::ZERO, Fixed::ZERO),
                Some(Rect::new(0, 0, logical_w, logical_h)),
                None,
                true,
                !super::branch::is_hidden_in_tree(world, root),
                output_plan.as_ref().filter(|plan| plan.matches_root(root)),
                prepared.then_some(&snapshot.prepared_paint),
            );
            snapshot.prepared_paint.clear();
            output_plan::return_plan(world, output_plan);
            world.put_resource_box(snapshot);
            return;
        }
        world.put_resource_box(snapshot);
    }
    let layout_tree = match build_layout_tree(world, root) {
        Ok(layout_tree) => layout_tree,
        Err(failure) => {
            record_text_layout_failure(world, root, failure);
            return;
        }
    };
    let Some(mut layout_tree) = layout_tree else {
        if super::branch::is_authored_hidden(world, root) {
            world.clear_subtree_paint(root);
        }
        return;
    };
    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);
    let mut idx = 0;
    seed_prev_rect_walk(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
        transform.scale(),
        (Fixed::ZERO, Fixed::ZERO),
        Some(Rect::new(0, 0, logical_w, logical_h)),
        None,
        true,
        !super::branch::is_hidden_in_tree(world, root),
        None,
        None,
    );
}

fn layout_node(
    world: &World,
    entity: Entity,
    active: bool,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    let Some(_) = world.get::<Widget>(entity) else {
        return Ok(None);
    };
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(None);
    };
    let mut node = LayoutNode::new(layout_style(world, entity, style.layout));
    node.entity = Some(entity);
    if active {
        apply_text_intrinsic(world, entity, &mut node)?;
        apply_static_glyph_intrinsic(world, entity, style, &mut node);
    }
    Ok(Some(node))
}

/// Recursively build a LayoutNode tree from ECS entities.
fn build_layout_tree(
    world: &World,
    entity: Entity,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    if super::branch::is_authored_hidden(world, entity) {
        return Ok(None);
    }
    let Some(mut node) = layout_node(world, entity, true)? else {
        return Ok(None);
    };

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if let Some(child_node) = build_layout_tree(world, child)? {
                node.add_child(child_node);
            }
        }
    }
    Ok(Some(node))
}

#[derive(Default)]
struct LayoutStorageDemand {
    nodes: usize,
    parked_roots: usize,
    max_entity_id: u32,
    missing_computed: usize,
    missing_visual_dirty: usize,
    missing_prev: usize,
    missing_text_handles: usize,
    missing_input_handles: usize,
    missing_scroll_bounds: usize,
}

fn count_layout_storage(world: &World, entity: Entity, demand: &mut LayoutStorageDemand) {
    if world.get::<Widget>(entity).is_none() || world.get::<Style>(entity).is_none() {
        return;
    }
    demand.nodes = demand.nodes.saturating_add(1);
    if world.has::<super::Hidden>(entity)
        || world.has::<super::branch::CachedBranchVisibility>(entity)
    {
        demand.parked_roots = demand.parked_roots.saturating_add(1);
    }
    demand.max_entity_id = demand.max_entity_id.max(entity.id);
    demand.missing_computed += usize::from(world.get::<super::ComputedRect>(entity).is_none());
    demand.missing_visual_dirty += usize::from(!world.has::<super::dirty::VisualDirty>(entity));
    demand.missing_prev +=
        usize::from(world.get::<super::dirty::PaintedCoverage>(entity).is_none());
    demand.missing_scroll_bounds += usize::from(
        world.has::<crate::input::event::scroll::ScrollOffset>(entity)
            && !world.has::<crate::input::event::scroll::ScrollBounds>(entity),
    );
    demand.missing_text_handles += usize::from(
        world.get::<crate::ui::widgets::Text>(entity).is_some()
            && world.get::<crate::text::TextLayoutHandle>(entity).is_none(),
    );
    demand.missing_input_handles += usize::from(
        world.has::<super::widgets::TextEditing>(entity)
            && !world
                .has::<super::widgets::text_editing::geometry::TextEditingLayoutHandle>(entity),
    );
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            count_layout_storage(world, child, demand);
        }
    }
}

fn build_retained_layout_tree(
    world: &World,
    entity: Entity,
    active: bool,
    parked: &mut Vec<LayoutNode>,
    parked_changed: &mut bool,
) -> Result<Option<LayoutNode>, TextLayoutFailure> {
    let Some(mut node) = layout_node(world, entity, active)? else {
        return Ok(None);
    };
    if let Some(children) = world.get::<Children>(entity) {
        node.children.reserve_exact(children.0.len());
        for &child in &children.0 {
            let child_active = active && !super::branch::is_authored_hidden(world, child);
            if let Some(child_node) =
                build_retained_layout_tree(world, child, child_active, parked, parked_changed)?
            {
                if child_active {
                    node.children.push(child_node);
                } else if active {
                    parked.push(child_node);
                    *parked_changed = true;
                } else {
                    node.children.push(child_node);
                }
            }
        }
    }
    Ok(Some(node))
}

fn take_parked_layout_node(
    parked: &mut Vec<LayoutNode>,
    entity: Entity,
    parked_changed: &mut bool,
) -> Option<LayoutNode> {
    parked
        .iter()
        .position(|node| node.entity == Some(entity))
        .map(|index| {
            *parked_changed = true;
            parked.swap_remove(index)
        })
}

fn park_layout_node(
    world: &mut World,
    parked: &mut Vec<LayoutNode>,
    node: LayoutNode,
    parked_changed: &mut bool,
) {
    if let Some(entity) = node.entity {
        world.invalidate_subtree_paint(entity);
    }
    if let Some(index) = parked.iter().position(|other| other.entity == node.entity) {
        parked[index] = node;
    } else {
        parked.push(node);
    }
    *parked_changed = true;
}

fn layout_entity_reachable(world: &World, root: Entity, target: Entity) -> bool {
    if !world.has::<Widget>(root) || !world.has::<Style>(root) {
        return false;
    }
    if root == target {
        return true;
    }
    world.get::<Children>(root).is_some_and(|children| {
        children
            .0
            .iter()
            .any(|&child| layout_entity_reachable(world, child, target))
    })
}

fn collect_layout_reachability(world: &World, root: Entity, out: &mut Vec<Entity>) -> bool {
    if !world.has::<Widget>(root) || !world.has::<Style>(root) {
        return true;
    }
    if out.try_reserve(1).is_err() {
        return false;
    }
    out.push(root);
    world.get::<Children>(root).is_none_or(|children| {
        children
            .0
            .iter()
            .all(|&child| collect_layout_reachability(world, child, out))
    })
}

fn prune_parked_children(world: &World, node: &mut LayoutNode) {
    let Some(parent) = node.entity else {
        return;
    };
    node.children.retain(|child| {
        child.entity.is_some_and(|entity| {
            world.is_alive(entity)
                && world.has::<Widget>(entity)
                && world.has::<Style>(entity)
                && world
                    .get::<Children>(parent)
                    .is_some_and(|children| children.0.contains(&entity))
        })
    });
    for child in &mut node.children {
        prune_parked_children(world, child);
    }
}

fn prune_parked_layout_nodes(
    world: &World,
    root: Entity,
    active_entities: &[Entity],
    parked: &mut Vec<LayoutNode>,
    reachable: &mut Vec<Entity>,
) {
    if parked.is_empty() {
        reachable.clear();
        if reachable.capacity() > active_entities.len().max(1).saturating_mul(4) {
            *reachable = Vec::new();
        }
        return;
    }
    reachable.clear();
    let indexed = collect_layout_reachability(world, root, reachable);
    if indexed {
        reachable.sort_unstable_by_key(|entity| (entity.id, entity.generation));
    }
    parked.retain(|node| {
        node.entity.is_some_and(|entity| {
            world.is_alive(entity)
                && if indexed {
                    reachable
                        .binary_search_by_key(&(entity.id, entity.generation), |e| {
                            (e.id, e.generation)
                        })
                        .is_ok()
                } else {
                    layout_entity_reachable(world, root, entity)
                }
                && !active_entities.contains(&entity)
        })
    });
    for node in parked.iter_mut() {
        prune_parked_children(world, node);
    }
    if parked.is_empty() || reachable.capacity() > reachable.len().max(1).saturating_mul(4) {
        *reachable = Vec::new();
    }
}

#[derive(Default)]
struct LayoutTreeChanges {
    parked: bool,
    structure: bool,
    solve: super::layout::solve::SolveScope,
}

fn refresh_layout_tree(
    world: &mut World,
    entity: Entity,
    node: &mut LayoutNode,
    parked: &mut Vec<LayoutNode>,
    changes: &mut LayoutTreeChanges,
) -> Result<bool, TextLayoutFailure> {
    if node.entity != Some(entity)
        || world.get::<Widget>(entity).is_none()
        || super::branch::is_authored_hidden(world, entity)
    {
        return Ok(false);
    }
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(false);
    };
    refresh_layout_values(world, entity, node, style, changes)?;

    refresh_layout_children(world, entity, node, parked, changes)?;
    Ok(true)
}

fn refresh_layout_values(
    world: &World,
    entity: Entity,
    node: &mut LayoutNode,
    style: &Style,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let next_style = layout_style(world, entity, style.layout);
    let observe_solve =
        !changes.structure && changes.solve != super::layout::solve::SolveScope::Full;
    let was_absolute = node.style.position == super::layout::Position::Absolute;
    let style_changed = observe_solve && node.style != next_style;
    let old_intrinsic = (node.intrinsic_width, node.intrinsic_height);
    node.style = next_style;
    node.intrinsic_width = None;
    node.intrinsic_height = None;
    apply_text_intrinsic(world, entity, node)?;
    apply_static_glyph_intrinsic(world, entity, style, node);
    if observe_solve
        && (style_changed || old_intrinsic != (node.intrinsic_width, node.intrinsic_height))
    {
        changes.solve.observe(node, was_absolute);
    }
    Ok(())
}

// The snapshot's maintenance revisions certify membership and child order.
// Values and intrinsic measurements still use the common preparation path.
fn refresh_stable_layout_tree(
    world: &World,
    node: &mut LayoutNode,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let entity = node.entity.expect("retained layout entity");
    let style = world.get::<Style>(entity).expect("retained layout style");
    refresh_layout_values(world, entity, node, style, changes)?;
    for child in &mut node.children {
        refresh_stable_layout_tree(world, child, changes)?;
    }
    Ok(())
}

fn refresh_layout_children(
    world: &mut World,
    entity: Entity,
    node: &mut LayoutNode,
    parked: &mut Vec<LayoutNode>,
    changes: &mut LayoutTreeChanges,
) -> Result<(), TextLayoutFailure> {
    let mut used = 0;
    if let Some(count) = world
        .get::<Children>(entity)
        .map(|children| children.0.len())
    {
        for index in 0..count {
            let child = world.get::<Children>(entity).unwrap().0[index];
            if world.get::<Widget>(child).is_none()
                || world.get::<Style>(child).is_none()
                || super::branch::is_authored_hidden(world, child)
            {
                continue;
            }
            if let Some(index) = node.children[used..]
                .iter()
                .position(|candidate| candidate.entity == Some(child))
            {
                changes.structure |= index != 0;
                node.children.swap(used, used + index);
            } else {
                let replacement = match take_parked_layout_node(parked, child, &mut changes.parked)
                {
                    Some(node) => Some(node),
                    None => {
                        build_retained_layout_tree(world, child, true, parked, &mut changes.parked)?
                    }
                };
                let Some(replacement) = replacement else {
                    continue;
                };
                changes.structure = true;
                if used < node.children.len() {
                    let displaced = core::mem::replace(&mut node.children[used], replacement);
                    park_layout_node(world, parked, displaced, &mut changes.parked);
                } else {
                    node.children.push(replacement);
                }
            }
            if refresh_layout_tree(world, child, &mut node.children[used], parked, changes)? {
                used += 1;
            }
        }
    }
    while node.children.len() > used {
        changes.structure = true;
        park_layout_node(
            world,
            parked,
            node.children.pop().expect("trailing layout child"),
            &mut changes.parked,
        );
    }
    Ok(())
}

fn layout_style(world: &World, entity: Entity, mut layout: LayoutStyle) -> LayoutStyle {
    super::widgets::follow_bounds::layout_style(world, entity, &mut layout);
    if world.get::<super::RespectSafeArea>(entity).is_none() {
        return layout;
    }
    let Some(insets) = world.resource::<crate::surface::SafeAreaInsets>().copied() else {
        return layout;
    };
    layout.padding.top = safe_area_padding(layout.padding.top, insets.top);
    layout.padding.right = safe_area_padding(layout.padding.right, insets.right);
    layout.padding.bottom = safe_area_padding(layout.padding.bottom, insets.bottom);
    layout.padding.left = safe_area_padding(layout.padding.left, insets.left);
    layout
}

fn safe_area_padding(authored: Dimension, inset: Fixed) -> Dimension {
    match authored {
        Dimension::Px(value) => Dimension::Px(value.max(inset)),
        Dimension::Auto | Dimension::Content => Dimension::Px(inset),
        Dimension::Percent(_) => authored,
    }
}

struct LaidOutText {
    handle: crate::text::TextLayoutHandle,
    measure: crate::text::TextMeasure,
}

fn layout_text(
    world: &World,
    entity: Entity,
    width: Fixed,
) -> Result<Option<LaidOutText>, TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::ui::widgets::text::Text;

    let Some(text) = world.get::<Text>(entity) else {
        return Ok(None);
    };
    let Some(style) = world.get::<Style>(entity) else {
        return Ok(None);
    };
    let Some(resource) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(None);
    };
    let face_limit = resource.storage().layouts.limits().fallback_faces;
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(entity, TextLayoutStage::Layout, error.required, error.limit)
    })?
    else {
        return Ok(None);
    };
    let font = fonts.primary();
    let content = text.resolve(world.resource());
    let metrics = font.metrics(font.size);
    let text_path = world.get::<crate::text::TextPath>(entity).copied();
    let content_width = crate::ui::widgets::text::linear_text_content_rect(
        style,
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: width,
            h: Fixed::ZERO,
        },
    )
    .w;
    let request = text.paragraph().layout_request(
        &content,
        metrics,
        text_path.is_none().then_some(content_width),
    );
    let language = text
        .paragraph()
        .language
        .as_ref()
        .map(crate::ui::widgets::LanguageTag::as_str);
    let font_fingerprint = fonts.layout_fingerprint(language, text.paragraph().shaping);
    let handle = match text_path {
        Some(path) => {
            let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
                return Ok(None);
            };
            let mut storage = resource.storage_mut();
            let crate::text::geometry::TextGeometryStorage {
                layouts,
                paths: baselines,
            } = &mut *storage;
            let line_limit = request.max_lines.min(layouts.limits().lines);
            baselines
                .with_lines(
                    paths,
                    path,
                    crate::text::baseline::DEFAULT_TOLERANCE,
                    line_limit,
                    |widths| {
                        let mut request = request;
                        request.max_lines = request.max_lines.min(widths.line_count());
                        request.line_widths = Some(widths);
                        fonts.with_typefaces(language, text.paragraph().shaping, |typefaces| {
                            layouts.layout_cached(
                                text_layout_owner(entity),
                                font_fingerprint,
                                request,
                                typefaces,
                            )
                        })
                    },
                )
                .map_err(|error| {
                    TextLayoutFailure::baseline(entity, TextLayoutStage::Layout, error)
                })?
                .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))?
        }
        None => fonts
            .with_typefaces(language, text.paragraph().shaping, |typefaces| {
                resource.storage_mut().layouts.layout_cached(
                    text_layout_owner(entity),
                    font_fingerprint,
                    request,
                    typefaces,
                )
            })
            .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Layout, error))?,
    };
    let measure = resource
        .storage()
        .layouts
        .get(handle)
        .map(|layout| layout.measure())
        .ok_or(TextLayoutFailure {
            entity,
            stage: TextLayoutStage::Layout,
            kind: TextLayoutFailureKind::MissingLayout,
        })?;
    Ok(Some(LaidOutText { handle, measure }))
}

fn prepare_input_layout(
    world: &mut World,
    entity: Entity,
    rect: Rect,
) -> Result<bool, TextLayoutFailure> {
    use super::widgets::text_editing::geometry::{TextEditingLayoutHandle, prepare_layout};

    match prepare_layout(world, entity, rect)? {
        Some(handle) => {
            if world
                .get::<TextEditingLayoutHandle>(entity)
                .map(|value| value.0)
                != Some(handle)
            {
                world.insert(entity, TextEditingLayoutHandle(handle));
                return Ok(true);
            }
        }
        None => {
            return Ok(world.remove::<TextEditingLayoutHandle>(entity).is_some());
        }
    }
    Ok(false)
}

fn prepare_text_geometry(
    world: &mut World,
    entities: &[Entity],
) -> Result<bool, TextLayoutFailure> {
    let mut changed = false;
    if world.has_any_by_id(core::any::TypeId::of::<super::widgets::TextEditing>()) {
        for &entity in entities {
            if !world.has::<super::widgets::TextEditing>(entity)
                || crate::ui::branch::is_hidden_in_tree(world, entity)
            {
                continue;
            }
            let Some(rect) = world.get::<super::ComputedRect>(entity).map(|rect| rect.0) else {
                continue;
            };
            changed |= prepare_input_layout(world, entity, rect)?;
        }
    }
    prepare_path_text_placements(world, entities)?;
    Ok(changed)
}

fn publish_text_geometry(world: &World, entities: &[Entity]) -> Result<(), TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
    use crate::text::geometry::TextGeometryRecord;
    use crate::text::{TextLayoutHandle, TextPath};
    use crate::ui::widgets::text::{Text, linear_text_content_rect, vertical_offset};

    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    if !outputs.has_requests() {
        return Ok(());
    }
    for &entity in entities {
        if !outputs.is_requested(entity) {
            continue;
        }
        let record = (|| {
            if crate::ui::branch::is_hidden_in_tree(world, entity) {
                return None;
            }
            let text = world.get::<Text>(entity)?;
            let style = world.get::<Style>(entity)?;
            let handle = world.get::<TextLayoutHandle>(entity).copied()?;
            let (rect, affine, projective) = widget_render_geometry(world, entity)?;
            let storage = outputs.storage();
            let layout = storage.layouts.get(handle)?;
            let fonts = ResolvedFontStack::resolve(
                world.resource::<crate::render::font::FontManager>(),
                &style.font_stack,
                style.font_size,
                storage.layouts.limits().fallback_faces,
            )
            .ok()??;
            let metrics = fonts.primary().metrics(fonts.primary().size);
            let text_path = world.get::<TextPath>(entity).copied();
            let placement = match text_path {
                Some(path) => Some(PlacementKey::new(
                    entity,
                    handle,
                    path,
                    world
                        .resource::<crate::render::path::PathStore>()?
                        .revision(path.path())
                        .ok()?,
                    DEFAULT_TOLERANCE,
                )),
                None => None,
            };
            let local = Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: rect.w,
                h: rect.h,
            };
            let mut content_rect = if text_path.is_some() {
                local
            } else {
                linear_text_content_rect(style, local)
            };
            if text_path.is_none() {
                content_rect.y += vertical_offset(
                    text.paragraph().vertical_align,
                    content_rect.h,
                    crate::types::fixed::from_textflow(layout.measure().height),
                );
            }
            let transform = if projective.is_identity() {
                Transform3D::from(affine)
            } else {
                projective
            }
            .compose(&Transform3D::from(Transform::translate(rect.x, rect.y)));
            let caret_ready =
                placement.is_some_and(|key| storage.paths.caret_frames(key, |_| ()).is_some());
            Some(TextGeometryRecord {
                layout: handle,
                placement,
                metrics,
                content_rect,
                transform,
                text_path,
                caret_ready,
            })
        })();
        if let Some(record) = record {
            outputs
                .publish_text(entity, record)
                .map_err(|_| TextLayoutFailure {
                    entity,
                    stage: TextLayoutStage::Layout,
                    kind: TextLayoutFailureKind::OutputPublication,
                })?;
        } else {
            outputs.reset(entity);
        }
    }
    Ok(())
}

fn prepare_path_text_placements(
    world: &World,
    entities: &[Entity],
) -> Result<(), TextLayoutFailure> {
    use crate::text::baseline::{DEFAULT_TOLERANCE, PathBaselineError, PlacementKey};
    use crate::text::{TextLayoutHandle, TextPath};

    let Some(outputs) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    let mut storage = outputs.storage_mut();
    let crate::text::geometry::TextGeometryStorage {
        layouts,
        paths: baselines,
    } = &mut *storage;
    baselines.begin_placements();
    if !world.has_any_by_id(core::any::TypeId::of::<TextPath>()) {
        return Ok(());
    }
    let (Some(_fonts), Some(paths)) = (
        world.resource::<crate::render::font::FontManager>(),
        world.resource::<crate::render::path::PathStore>(),
    ) else {
        return Ok(());
    };
    let key_for = |entity| -> Result<Option<(PlacementKey, TextLayoutHandle)>, TextLayoutFailure> {
        if !world.has::<crate::ui::widgets::Text>(entity)
            || crate::ui::branch::is_hidden_in_tree(world, entity)
        {
            return Ok(None);
        }
        let (Some(path), Some(handle)) = (
            world.get::<TextPath>(entity).copied(),
            world.get::<TextLayoutHandle>(entity).copied(),
        ) else {
            return Ok(None);
        };
        let revision = paths.revision(path.path()).map_err(|error| {
            TextLayoutFailure::baseline(
                entity,
                TextLayoutStage::Layout,
                PathBaselineError::Store(error),
            )
        })?;
        Ok(Some((
            PlacementKey::new(entity, handle, path, revision, DEFAULT_TOLERANCE),
            handle,
        )))
    };
    for &entity in entities {
        if let Some((key, _)) = key_for(entity)? {
            baselines.pin_placement(key);
        }
    }
    for &entity in entities {
        let Some((key, handle)) = key_for(entity)? else {
            continue;
        };
        let layout = layouts.get(handle).ok_or(TextLayoutFailure {
            entity,
            stage: TextLayoutStage::Layout,
            kind: TextLayoutFailureKind::MissingLayout,
        })?;
        baselines
            .prepare_glyph_frames(paths, key, &layout)
            .map_err(|error| TextLayoutFailure::baseline(entity, TextLayoutStage::Layout, error))?;
        // Caret storage is optional; exhausted interaction geometry does not discard glyphs.
        let _ = baselines.prepare_caret_frames(paths, key, &layout);
    }
    Ok(())
}

fn layout_text_tree(
    node: &mut LayoutNode,
    world: &mut World,
    entities: &[Entity],
    index: &mut usize,
) -> Result<bool, TextLayoutFailure> {
    use crate::types::fixed::from_textflow;

    if *index >= entities.len() {
        return Ok(false);
    }
    let entity = entities[*index];
    *index += 1;
    let mut intrinsic_changed = false;
    if let Some(layout) = layout_text(world, entity, node.rect.w)? {
        let measured_width = from_textflow(layout.measure.width);
        let measured_height = from_textflow(layout.measure.height);
        let (width, height) = if world.get::<crate::text::TextPath>(entity).is_none() {
            world
                .get::<Style>(entity)
                .map_or((measured_width, measured_height), |style| {
                    crate::ui::widgets::text::padded_text_intrinsic_size(
                        style,
                        measured_width,
                        measured_height,
                    )
                })
        } else {
            (measured_width, measured_height)
        };
        intrinsic_changed =
            node.intrinsic_width != Some(width) || node.intrinsic_height != Some(height);
        node.set_intrinsic_size(width, height);
        if world.get::<crate::text::TextLayoutHandle>(entity) != Some(&layout.handle) {
            world.insert(entity, layout.handle);
        }
    }
    prepare_input_layout(world, entity, node.rect)?;
    for child in &mut node.children {
        intrinsic_changed |= layout_text_tree(child, world, entities, index)?;
    }
    Ok(intrinsic_changed)
}

fn compute_layout_snapshot(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
) -> Result<Option<Box<LayoutSnapshot>>, TextLayoutFailure> {
    world.watch_component_revision::<crate::input::event::scroll::ScrollConfig>();
    LayoutSnapshot::watch_maintenance(world);
    output_plan::invalidate_geometry(world);
    let mut previous = world.take_resource_box::<LayoutSnapshot>();
    let local_solve = previous.as_ref().is_some_and(|snapshot| {
        snapshot.matches(root, logical_w, logical_h) && !snapshot.bindings_pending
    });
    if super::branch::is_authored_hidden(world, root) {
        output_plan::invalidate_structure(world);
        if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
            cache
                .storage_mut()
                .layouts
                .begin_frame_with_owner_filter(|_| false);
        }
        return Ok(None);
    }
    let initial_demand = (!previous
        .as_ref()
        .is_some_and(|snapshot| snapshot.root == root))
    .then(|| {
        let mut demand = LayoutStorageDemand::default();
        count_layout_storage(world, root, &mut demand);
        demand
    });
    if let Some(demand) = &initial_demand {
        if demand.parked_roots > 0 {
            world.reserve_dirty_traversal_stack();
        }
        if demand.missing_visual_dirty > 0 {
            world.reserve_component_storage::<super::dirty::VisualDirty>(
                demand.max_entity_id,
                demand.missing_visual_dirty,
            );
        }
        if demand.missing_scroll_bounds > 0 {
            world.reserve_component_storage::<crate::input::event::scroll::ScrollBounds>(
                demand.max_entity_id,
                demand.missing_scroll_bounds,
            );
        }
        if demand.missing_computed > 0 {
            world.reserve_component_storage::<super::ComputedRect>(
                demand.max_entity_id,
                demand.missing_computed,
            );
        }
        if demand.missing_prev > 0 {
            world.reserve_component_storage::<super::dirty::PaintedCoverage>(
                demand.max_entity_id,
                demand.missing_prev,
            );
        }
        if demand.missing_text_handles > 0 {
            world.reserve_component_storage::<crate::text::TextLayoutHandle>(
                demand.max_entity_id,
                demand.missing_text_handles,
            );
        }
        if demand.missing_input_handles > 0 {
            world.reserve_component_storage::<super::widgets::text_editing::geometry::TextEditingLayoutHandle>(
                demand.max_entity_id,
                demand.missing_input_handles,
            );
        }
    }
    let mut entities = previous
        .as_mut()
        .map(|snapshot| core::mem::take(&mut snapshot.entities))
        .unwrap_or_default();
    entities.clear();
    if let Some(demand) = &initial_demand {
        entities.reserve(demand.nodes);
    }
    collect_entities_preorder(world, root, &mut entities);
    entities.sort_unstable_by_key(|entity| (entity.id, entity.generation));
    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache
            .storage_mut()
            .layouts
            .begin_frame_with_owner_filter(|owner| visible_text_owner(world, &entities, owner));
    }
    entities.clear();
    let mut changes = LayoutTreeChanges {
        solve: if local_solve {
            super::layout::solve::SolveScope::Unchanged
        } else {
            super::layout::solve::SolveScope::Full
        },
        ..Default::default()
    };
    let mut snapshot = crate::trace_span!("layout.build_tree", {
        match previous {
            Some(mut snapshot) if snapshot.root == root => {
                let refreshed = if snapshot.maintenance_current(world) {
                    refresh_stable_layout_tree(world, &mut snapshot.layout_tree, &mut changes)
                        .map(|()| true)
                } else {
                    refresh_layout_tree(
                        world,
                        root,
                        &mut snapshot.layout_tree,
                        &mut snapshot.parked,
                        &mut changes,
                    )
                };
                if changes.structure || !matches!(refreshed, Ok(true)) {
                    snapshot.invalidate_geometry_plan();
                    output_plan::invalidate_structure(world);
                }
                match refreshed {
                    Ok(true) => {}
                    Ok(false) => return Ok(None),
                    Err(failure) => {
                        snapshot.valid = false;
                        snapshot.entities = entities;
                        world.put_resource_box(snapshot);
                        return Err(failure);
                    }
                }
                snapshot.entities = entities;
                snapshot
            }
            _ => {
                output_plan::invalidate_structure(world);
                let node_count = initial_demand.as_ref().map_or(0, |demand| demand.nodes);
                let parked_capacity = initial_demand
                    .as_ref()
                    .map_or(0, |demand| demand.parked_roots);
                let mut parked = Vec::with_capacity(parked_capacity);
                let Some(layout_tree) = build_retained_layout_tree(
                    world,
                    root,
                    true,
                    &mut parked,
                    &mut changes.parked,
                )?
                else {
                    return Ok(None);
                };
                Box::new(LayoutSnapshot {
                    root,
                    logical_w,
                    logical_h,
                    valid: false,
                    bindings_pending: false,
                    scroll_config_revision: 0,
                    maintenance_revisions: [0; 6],
                    layout_tree,
                    parked,
                    reachable: Vec::new(),
                    entities,
                    painted_damage: Vec::with_capacity(node_count),
                    prepared_paint: PreparedPaint::with_capacity(node_count),
                    paths: super::layout::paths::LayoutPaths::default(),
                    follow_plan: None,
                })
            }
        }
    });
    crate::trace_span!("layout.initial_compute", {
        let solved_locally =
            local_solve && !changes.structure && changes.solve.apply(&mut snapshot.layout_tree);
        #[cfg(test)]
        local_layout_tests::record_solve(solved_locally);
        if !solved_locally {
            compute_layout(
                &mut snapshot.layout_tree,
                Fixed::ZERO,
                Fixed::ZERO,
                logical_w.into(),
                logical_h.into(),
            );
        }
    });
    crate::trace_span!("layout.collect_entities", {
        snapshot.entities.clear();
        collect_entities_preorder(world, root, &mut snapshot.entities);
    });
    snapshot.prepared_paint.resize(
        snapshot.entities.len(),
        world
            .resource::<ViewRegistry>()
            .map_or(0, |registry| registry.paint_iter().len()),
    );
    if !snapshot.maintenance_current(world) || changes.parked {
        crate::trace_span!("layout.prune_parked", {
            prune_parked_layout_nodes(
                world,
                root,
                &snapshot.entities,
                &mut snapshot.parked,
                &mut snapshot.reachable,
            );
        });
    }
    let mut text_index = 0;
    let intrinsic_changed = crate::trace_span!("layout.text", {
        layout_text_tree(
            &mut snapshot.layout_tree,
            world,
            &snapshot.entities,
            &mut text_index,
        )
    });
    let intrinsic_changed = match intrinsic_changed {
        Ok(changed) => changed,
        Err(failure) => {
            snapshot.valid = false;
            world.put_resource_box(snapshot);
            return Err(failure);
        }
    };
    if intrinsic_changed {
        crate::trace_span!("layout.final_compute", {
            #[cfg(test)]
            local_layout_tests::record_solve(false);
            compute_layout(
                &mut snapshot.layout_tree,
                Fixed::ZERO,
                Fixed::ZERO,
                logical_w.into(),
                logical_h.into(),
            )
        });
    }
    snapshot.logical_w = logical_w;
    snapshot.logical_h = logical_h;
    snapshot.maintenance_revisions = LayoutSnapshot::maintenance_revisions(world);
    snapshot.valid = true;
    Ok(Some(snapshot))
}

pub(crate) fn apply_text_intrinsic(
    world: &World,
    entity: Entity,
    node: &mut LayoutNode,
) -> Result<(), TextLayoutFailure> {
    use crate::render::font::ResolvedFontStack;
    use crate::types::fixed::from_textflow;
    use crate::ui::widgets::text::Text;

    let Some(text) = world.get::<Text>(entity) else {
        return Ok(());
    };
    let style = world.get::<Style>(entity).expect("text widget style");
    let Some(cache) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    let face_limit = cache.storage().layouts.limits().fallback_faces;
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        &style.font_stack,
        style.font_size,
        face_limit,
    )
    .map_err(|error| {
        TextLayoutFailure::font_stack(
            entity,
            TextLayoutStage::Measure,
            error.required,
            error.limit,
        )
    })?
    else {
        return Ok(());
    };
    let font = fonts.primary();
    let content = text.resolve(world.resource());
    let metrics = font.metrics(font.size);
    let text_path = world.get::<crate::text::TextPath>(entity).copied();
    let request = text.paragraph().layout_request(&content, metrics, None);
    let language = text
        .paragraph()
        .language
        .as_ref()
        .map(crate::ui::widgets::LanguageTag::as_str);
    let font_fingerprint = fonts.layout_fingerprint(language, text.paragraph().shaping);
    let measure = match text_path {
        Some(path) => {
            let Some(paths) = world.resource::<crate::render::path::PathStore>() else {
                return Ok(());
            };
            let mut storage = cache.storage_mut();
            let crate::text::geometry::TextGeometryStorage {
                layouts,
                paths: baselines,
            } = &mut *storage;
            let line_limit = request.max_lines.min(layouts.limits().lines);
            baselines
                .with_lines(
                    paths,
                    path,
                    crate::text::baseline::DEFAULT_TOLERANCE,
                    line_limit,
                    |widths| {
                        let mut request = request;
                        request.max_lines = request.max_lines.min(widths.line_count());
                        request.line_widths = Some(widths);
                        fonts.with_typefaces(language, text.paragraph().shaping, |faces| {
                            layouts.measure_cached(
                                text_layout_owner(entity),
                                font_fingerprint,
                                request,
                                faces,
                            )
                        })
                    },
                )
                .map_err(|error| {
                    TextLayoutFailure::baseline(entity, TextLayoutStage::Measure, error)
                })?
                .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Measure, error))?
        }
        None => fonts
            .with_typefaces(language, text.paragraph().shaping, |faces| {
                cache.storage_mut().layouts.measure_cached(
                    text_layout_owner(entity),
                    font_fingerprint,
                    request,
                    faces,
                )
            })
            .map_err(|error| TextLayoutFailure::text(entity, TextLayoutStage::Measure, error))?,
    };
    let measured_width = from_textflow(measure.width);
    let measured_height = from_textflow(measure.height);
    let (width, height) = if text_path.is_none() {
        crate::ui::widgets::text::padded_text_intrinsic_size(style, measured_width, measured_height)
    } else {
        (measured_width, measured_height)
    };
    node.set_intrinsic_size(width, height);
    Ok(())
}

fn text_layout_owner(entity: Entity) -> u64 {
    u64::from(entity.id) | (u64::from(entity.generation) << 32)
}

fn visible_text_owner(world: &World, visible_entities: &[Entity], owner: u64) -> bool {
    let entity = Entity {
        id: owner as u32,
        generation: (owner >> 32) as u32,
    };
    visible_entities
        .binary_search_by_key(&(entity.id, entity.generation), |candidate| {
            (candidate.id, candidate.generation)
        })
        .is_ok()
        && world.get::<Widget>(entity).is_some()
        && world.get::<Style>(entity).is_some()
}

fn apply_static_glyph_intrinsic(
    world: &World,
    entity: Entity,
    style: &Style,
    node: &mut LayoutNode,
) {
    let Some(run) = world.get::<crate::ui::widgets::StaticGlyphRun>(entity) else {
        return;
    };
    let Some(font) = crate::render::font::resolve_or_default(world, style.font_stack.primary())
    else {
        return;
    };
    let ppem = style.font_size.unwrap_or(font.size).max(1);
    let metrics = font.metrics(ppem);
    let (width, height) = run.measure(metrics.line_height);
    node.set_intrinsic_size(width, height);
}

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

pub(crate) fn count_nodes(node: &LayoutNode) -> usize {
    1 + node.children.iter().map(count_nodes).sum::<usize>()
}

/// Recursively emit draw commands from the computed layout tree
#[allow(clippy::too_many_arguments)]
fn intersect_with_self(clip: &Rect, rect: &Rect) -> Rect {
    let cx = clip.x.max(rect.x);
    let cy = clip.y.max(rect.y);
    let cx2 = (clip.x + clip.w).min(rect.x + rect.w);
    let cy2 = (clip.y + clip.h).min(rect.y + rect.h);
    Rect {
        x: cx,
        y: cy,
        w: if cx2 > cx { cx2 - cx } else { Fixed::ZERO },
        h: if cy2 > cy { cy2 - cy } else { Fixed::ZERO },
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tree_offset(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    offset_x: Fixed,
    offset_y: Fixed,
    parent_transform: &Transform,
    parent_transform_3d: &Transform3D,
    output_plan: Option<&output_plan::TexturePlan>,
    requests: &PaintRequestScope,
    preparation: Option<&PreparedPaint>,
) -> Result<(), RenderError> {
    if let Some(&entity) = entities.get(*idx)
        && super::branch::is_hidden_in_tree(world, entity)
    {
        *idx += count_nodes(node);
        return Ok(());
    }
    draw_visible_tree_offset(
        node,
        world,
        entities,
        idx,
        renderer,
        clip,
        offset_x,
        offset_y,
        parent_transform,
        parent_transform_3d,
        output_plan,
        requests,
        preparation,
    )
}

#[allow(clippy::too_many_arguments)]
#[mirui::trace_fn("draw.entity")]
fn draw_visible_tree_offset(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    offset_x: Fixed,
    offset_y: Fixed,
    parent_transform: &Transform,
    parent_transform_3d: &Transform3D,
    output_plan: Option<&output_plan::TexturePlan>,
    requests: &PaintRequestScope,
    preparation: Option<&PreparedPaint>,
) -> Result<(), RenderError> {
    let Some(&entity) = entities.get(*idx) else {
        return Ok(());
    };
    if node.children.is_empty()
        && output_plan.is_none()
        && let Some(coverage) = preparation.and_then(|prepared| prepared.get(*idx))
        && !rects_intersect(&coverage.outer, clip)
    {
        *idx += 1;
        return Ok(());
    }
    let visibility = prepared_paint::visibility(world, entity, *idx, preparation);
    if visibility == prepared_paint::PaintVisibility::Hidden {
        *idx += count_nodes(node);
        return Ok(());
    }
    if visibility == prepared_paint::PaintVisibility::Unclipped {
        return draw_tree_content(
            node,
            world,
            entities,
            idx,
            renderer,
            clip,
            offset_x,
            offset_y,
            parent_transform,
            parent_transform_3d,
            output_plan,
            requests,
            preparation,
        );
    }
    let rect = Rect {
        x: node.rect.x - offset_x,
        y: node.rect.y - offset_y,
        ..node.rect
    };
    let (affine, projective) = prepared_paint::transforms(
        *parent_transform,
        *parent_transform_3d,
        world,
        entity,
        rect,
        *idx,
        preparation,
    );
    let before = *idx;
    let result = super::widgets::follow_bounds::clip::draw(
        world,
        entity,
        rect,
        affine,
        projective,
        renderer,
        *clip,
        |renderer, clip| {
            draw_tree_content(
                node,
                world,
                entities,
                idx,
                renderer,
                &clip,
                offset_x,
                offset_y,
                parent_transform,
                parent_transform_3d,
                output_plan,
                requests,
                preparation,
            )
        },
    );
    if *idx == before {
        *idx += count_nodes(node);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn draw_tree_content(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    offset_x: Fixed,
    offset_y: Fixed,
    parent_transform: &Transform,
    parent_transform_3d: &Transform3D,
    output_plan: Option<&output_plan::TexturePlan>,
    requests: &PaintRequestScope,
    preparation: Option<&PreparedPaint>,
) -> Result<(), RenderError> {
    let shifted_rect = Rect {
        x: node.rect.x - offset_x,
        y: node.rect.y - offset_y,
        w: node.rect.w,
        h: node.rect.h,
    };

    let entity = if *idx < entities.len() {
        entities[*idx]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };
    let (tf, tf_3d) = prepared_paint::transforms(
        *parent_transform,
        *parent_transform_3d,
        world,
        entity,
        shifted_rect,
        *idx,
        preparation,
    );
    let quad = if tf_3d.is_identity()
        && !matches!(
            tf.classify(),
            crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
        ) {
        Some(tf.apply_rect(shifted_rect))
    } else {
        None
    };

    let cull_rect = planned_visual_bounds(
        world,
        entity,
        shifted_rect,
        tf,
        tf_3d,
        renderer.output_scale(),
        *idx,
        output_plan,
        preparation,
    );
    let visible = rects_intersect(&cull_rect, clip);
    let style = world.get::<Style>(entity);
    let scroll = if node.children.is_empty() {
        None
    } else {
        world.get::<crate::input::event::scroll::ScrollOffset>(entity)
    };
    let clips_children = !node.children.is_empty()
        && (scroll.is_some() || style.is_some_and(|style| style.clip_children));
    let child_clip = if clips_children {
        transformed_clip_bounds(shifted_rect, tf, tf_3d)
            .map(|bounds| intersect_with_self(clip, &bounds))
            .unwrap_or(Rect::ZERO)
    } else {
        *clip
    };
    if !visible
        && (node.children.is_empty() || child_clip.w <= Fixed::ZERO || child_clip.h <= Fixed::ZERO)
    {
        *idx += count_nodes(node);
        return Ok(());
    }

    if output_plan.is_some_and(|plan| plan.blocked(entity)) {
        *idx += count_nodes(node);
        return Ok(());
    }

    let offscreen = prepared_paint::offscreen(world, entity, *idx, preparation).map(|mut off| {
        if let Some(fallback) = world.get::<crate::ui::offscreen::CompositeFallback>(entity)
            && !output_plan.is_some_and(|plan| plan.ready(entity) && plan.ready(fallback.required))
        {
            off.opacity = fallback.opacity;
        }
        off
    });
    if offscreen.is_some_and(|off| off.opacity == 0) {
        *idx += count_nodes(node);
        return Ok(());
    }
    if output_plan.is_some_and(|plan| plan.ready(entity))
        && let Some(off) = offscreen
    {
        let handled = if !tf_3d.is_identity() {
            let mut scoped = ScopedRenderer {
                physical_clip: None,
                inner: renderer,
                transform: tf_3d,
            };
            draw_prepared_output(
                node,
                world,
                idx,
                &mut scoped,
                clip,
                &shifted_rect,
                off,
                entity,
                &tf,
            )?
        } else {
            draw_prepared_output(
                node,
                world,
                idx,
                renderer,
                clip,
                &shifted_rect,
                off,
                entity,
                &tf,
            )?
        };
        if handled {
            return Ok(());
        }
    }

    if visible && *idx < entities.len() {
        if let Some(style) = style {
            let state = prepared_paint::state(world, entity, *idx, preparation);
            let clip_x = !matches!(
                style.layout.width,
                crate::types::Dimension::Auto | crate::types::Dimension::Content
            );
            let clip_y = !matches!(
                style.layout.height,
                crate::types::Dimension::Auto | crate::types::Dimension::Content
            );
            let view_clip = if (clip_x || clip_y)
                && world.get::<crate::ui::widgets::Text>(entity).is_some()
                && world.get::<crate::text::TextPath>(entity).is_none()
            {
                let mut bounds = transformed_clip_bounds(shifted_rect, tf, tf_3d)
                    .map(|bounds| intersect_with_self(clip, &bounds))
                    .unwrap_or(Rect {
                        x: clip.x,
                        y: clip.y,
                        w: Fixed::ZERO,
                        h: Fixed::ZERO,
                    });
                if !clip_x {
                    bounds.x = clip.x;
                    bounds.w = clip.w;
                }
                if !clip_y {
                    bounds.y = clip.y;
                    bounds.h = clip.h;
                }
                bounds
            } else {
                *clip
            };
            if !tf_3d.is_identity() {
                let mut scoped = ScopedRenderer {
                    physical_clip: None,
                    inner: renderer,
                    transform: tf_3d,
                };
                let result = {
                    let mut ctx = PaintCtx::new(
                        &mut scoped,
                        style,
                        &shifted_rect,
                        &view_clip,
                        tf,
                        quad,
                        state,
                    );
                    if let Some(plan) = output_plan {
                        ctx.set_workspace(&plan.workspace);
                    }
                    ctx.set_layout_offset(Point::new(offset_x, offset_y));
                    render_views(world, entity, &mut ctx, requests, preparation, *idx)
                };
                result?;
            } else {
                let mut ctx =
                    PaintCtx::new(renderer, style, &shifted_rect, &view_clip, tf, quad, state);
                if let Some(plan) = output_plan {
                    ctx.set_workspace(&plan.workspace);
                }
                ctx.set_layout_offset(Point::new(offset_x, offset_y));
                render_views(world, entity, &mut ctx, requests, preparation, *idx)?;
            }
        }
    }
    *idx += 1;

    let (sx, sy) = scroll.map_or((offset_x, offset_y), |scroll| {
        (offset_x + scroll.x, offset_y + scroll.y)
    });

    for child in &node.children {
        draw_visible_tree_offset(
            child,
            world,
            entities,
            idx,
            renderer,
            &child_clip,
            sx,
            sy,
            &tf,
            &tf_3d,
            output_plan,
            requests,
            preparation,
        )?;
    }
    Ok(())
}

fn render_views<'frame>(
    world: &World,
    entity: Entity,
    ctx: &mut PaintCtx<'frame>,
    requests: &'frame PaintRequestScope,
    preparation: Option<&PreparedPaint>,
    position: usize,
) -> Result<(), RenderError> {
    ctx.set_waker(requests.waker());
    let Some(registry) = world.resource::<ViewRegistry>() else {
        return Ok(());
    };
    crate::trace_span!("draw.view_dispatch");
    for view in prepared_paint::paint_views(world, registry, entity, position, preparation) {
        let reusable = preparation.is_some() && view.measurement_reusable();
        view.paint(world, entity, ctx);
        if reusable
            && !view.measurement_reusable()
            && let Some(preparation) = preparation
        {
            preparation.clear_coverage();
        }
        if ctx.resource_pending() && !requests.wait_for_resource(entity) {
            return Err(RenderError::MissingWorkspace);
        }
        if let Some(error) = ctx.error() {
            return Err(error);
        }
    }
    let recorded = match ctx.frame_request() {
        FrameRequest::None => true,
        FrameRequest::Next => requests.request(entity),
        FrameRequest::IfOutputChanged => requests.request_on_change(entity),
    };
    if !recorded {
        return Err(RenderError::MissingWorkspace);
    }
    Ok(())
}

#[cfg(test)]
mod projective_transform_tests {
    use super::*;

    #[derive(Default)]
    struct AffineOnlyRenderer {
        draws: usize,
    }

    #[derive(Default)]
    struct ProjectiveLeafCapture {
        quad_was_none: Option<bool>,
        transform: Option<Transform3D>,
    }

    impl Renderer for AffineOnlyRenderer {
        type TargetState = ();

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            self.draws += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    impl Renderer for ProjectiveLeafCapture {
        type TargetState = ();

        fn route(
            &self,
            _: &DrawRequest<'_, '_>,
        ) -> Result<crate::render::RenderRoute, RenderError> {
            Ok(crate::render::RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            if let DrawCommand::Fill { quad, .. } = request.command {
                self.quad_was_none = Some(quad.is_none());
                self.transform = Some(request.projective);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    #[test]
    fn first_projective_boundary_folds_affine_ancestors_once() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        let affine = Transform::translate(Fixed::from_int(3), Fixed::from_int(2));
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(18), Fixed::from_int(400));
        world.insert(entity, WidgetTransform(affine));
        world.insert(entity, WidgetTransform3D(projective));
        let parent = Transform::scale(Fixed::from_int(2), Fixed::from_int(2));
        let rect = Rect::new(10, 10, 20, 10);

        let (command, scope) =
            render_transforms(parent, Transform3D::IDENTITY, &world, entity, rect);
        let local_affine = effective_transform(&Transform::IDENTITY, &world, entity, rect);
        let local_projective = effective_transform_3d(&Transform3D::IDENTITY, &world, entity, rect);
        let expected = Transform3D::from_affine(parent)
            .compose(&local_projective)
            .compose(&Transform3D::from_affine(local_affine));

        assert_eq!(command, Transform::IDENTITY);
        assert_eq!(scope, expected);
    }

    #[test]
    fn affine_descendant_composes_inside_existing_projective_scope() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        world.insert(
            entity,
            WidgetTransform(Transform::translate(
                Fixed::from_int(4),
                Fixed::from_int(-2),
            )),
        );
        let parent = Transform3D::rotate_x_perspective(Fixed::from_int(12), Fixed::from_int(360));
        let rect = Rect::new(8, 6, 24, 12);

        let (command, scope) = render_transforms(Transform::IDENTITY, parent, &world, entity, rect);
        let local = effective_transform(&Transform::IDENTITY, &world, entity, rect);

        assert_eq!(command, Transform::IDENTITY);
        assert_eq!(scope, parent.compose(&Transform3D::from_affine(local)));
    }

    #[test]
    fn affine_only_renderer_does_not_receive_projective_approximation() {
        let mut renderer = AffineOnlyRenderer::default();
        let mut scoped = ScopedRenderer {
            physical_clip: None,
            inner: &mut renderer,
            transform: Transform3D::translate(Fixed::from_int(4), Fixed::ZERO),
        };
        let command = DrawCommand::Fill {
            area: Rect::new(0, 0, 8, 8),
            transform: Transform::IDENTITY,
            quad: None,
            color: crate::types::Color::rgb(255, 255, 255),
            radius: Fixed::ZERO,
            opa: u8::MAX,
        };
        assert_eq!(
            scoped.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 16, 16))),
            Err(RenderError::Unsupported(
                crate::render::RenderFeature::ProjectiveGeometry
            ))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn projective_scope_forwards_text_run_identity() {
        #[derive(Default)]
        struct Recorder {
            seen: Option<(crate::render::TextRunIdentity, Transform3D)>,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn submit_text_run(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
                identity: crate::render::TextRunIdentity,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.seen = Some((identity, request.projective));
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let identity = crate::render::TextRunIdentity {
            entity: Entity {
                id: 3,
                generation: 2,
            },
            run_index: 1,
        };
        let projection = Transform3D::translate(Fixed::from_int(2), Fixed::from_int(3));
        let clip = Rect::new(0, 0, 10, 10);
        let command = DrawCommand::ApplyBlur {
            alpha: Fixed::ONE,
            region: clip,
        };
        let mut recorder = Recorder::default();
        ScopedRenderer {
            physical_clip: None,
            inner: &mut recorder,
            transform: projection,
        }
        .draw_text_run(&DrawRequest::new(&command, clip), identity)
        .unwrap();
        assert_eq!(recorder.seen, Some((identity, projection)));
    }

    #[test]
    fn projective_widget_keeps_the_leaf_unprojected() {
        let mut app = crate::app::App::headless(32, 24);
        app.with_default_widgets();
        let root = app.world.spawn_empty();
        app.world.insert(root, Widget);
        app.world.insert(
            root,
            Style {
                bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
                layout: crate::ui::layout::LayoutStyle {
                    width: crate::types::Dimension::px(32),
                    height: crate::types::Dimension::px(24),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
        app.world.insert(root, WidgetTransform3D(projective));

        let mut capture = ProjectiveLeafCapture::default();
        render(
            &app.world,
            root,
            &Viewport::new(32, 24, Fixed::ONE),
            &mut capture,
        )
        .unwrap();

        assert_eq!(capture.quad_was_none, Some(true));
        assert!(capture.transform.is_some_and(|value| !value.is_identity()));
    }

    #[test]
    fn projective_widget_failure_reaches_render_caller() {
        let mut app = crate::app::App::headless(32, 24);
        app.with_default_widgets();
        let root = app.world.spawn_empty();
        app.world.insert(root, Widget);
        app.world.insert(
            root,
            Style {
                bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
                layout: crate::ui::layout::LayoutStyle {
                    width: crate::types::Dimension::px(32),
                    height: crate::types::Dimension::px(24),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        app.world.insert(
            root,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::from_int(12),
                Fixed::from_int(320),
            )),
        );

        let mut renderer = AffineOnlyRenderer::default();
        assert_eq!(
            render(
                &app.world,
                root,
                &Viewport::new(32, 24, Fixed::ONE),
                &mut renderer
            ),
            Err(RenderError::Unsupported(
                crate::render::RenderFeature::ProjectiveGeometry
            ))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn affine_widget_failure_reaches_render_caller() {
        let mut app = crate::app::App::headless(32, 24);
        app.with_default_widgets();
        let root = app.world.spawn_empty();
        app.world.insert(root, Widget);
        app.world.insert(
            root,
            Style {
                bg_color: Some(crate::types::Color::rgb(20, 40, 60).into()),
                layout: crate::ui::layout::LayoutStyle {
                    width: crate::types::Dimension::px(32),
                    height: crate::types::Dimension::px(24),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut renderer = AffineOnlyRenderer::default();
        assert_eq!(
            render(
                &app.world,
                root,
                &Viewport::new(32, 24, Fixed::ONE),
                &mut renderer
            ),
            Err(RenderError::Unsupported(
                crate::render::RenderFeature::AffineGeometry
            ))
        );
        assert_eq!(renderer.draws, 0);
    }

    #[test]
    fn child_clip_uses_the_effective_transform() {
        let rect = Rect::new(8, 6, 20, 12);
        let affine = Transform::translate(Fixed::from_int(4), Fixed::from_int(-2));
        assert_eq!(
            transformed_clip_bounds(rect, affine, Transform3D::IDENTITY),
            Some(affine.apply_rect_bbox(rect))
        );

        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
        let expected = projective.apply_rect(rect).map(quad_bbox);
        assert_eq!(
            transformed_clip_bounds(rect, Transform::IDENTITY, projective),
            expected
        );

        let mut behind = Transform3D::IDENTITY;
        behind.m22 = -crate::types::Fixed64::ONE;
        assert_eq!(
            transformed_clip_bounds(rect, Transform::IDENTITY, behind),
            None
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_offscreen(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    shifted_rect: &Rect,
    key: super::offscreen::BufferKey,
    entity: Entity,
    output_plan: Option<&output_plan::TexturePlan>,
    scratch: &mut crate::render::backends::sw::SwScratch,
) -> Result<bool, RenderError> {
    use crate::render::canvas::Canvas;
    use crate::render::sw::SwRenderer;

    if renderer.offscreen_format().is_none() || super::branch::is_hidden_in_tree(world, entity) {
        return Ok(false);
    }

    // Preparation is independent of the parent clip: a visible consumer can
    // need a source outside the current damage region.

    let scale = key.scale;
    let (buf_w, buf_h) = (key.w, key.h);

    let outputs = match world.resource::<Outputs<WidgetTexture>>() {
        Some(p) => p,
        None => return Ok(false),
    };

    crate::trace_span!("draw.offscreen");

    if outputs.texture_matches(key)? {
        *idx += count_nodes(node);
        return Ok(true);
    }

    // Keep unfinished pixels outside the readable cache. View callbacks can
    // borrow completed sources without overlapping a mutable cache borrow.
    let Some(mut write) = outputs.prepare_texture(key) else {
        return Ok(false);
    };
    let requests = PaintRequestScope::new(world);
    let texture = write.texture_mut();

    if texture.is_native() {
        let workspace = &output_plan
            .ok_or(RenderError::MissingWorkspace)?
            .target_workspace;
        workspace.with_bytes(texture.target_workspace_bytes(), |bytes| {
            texture.render_native(
                renderer,
                Viewport::new(buf_w, buf_h, scale),
                bytes,
                &mut |inner| {
                    draw_output_subtree(
                        node,
                        world,
                        entities,
                        idx,
                        inner,
                        shifted_rect,
                        key,
                        output_plan,
                        &requests,
                    )
                },
            )
        })?;
    } else {
        let mut pixels = texture.pixels_mut()?;
        let stride = pixels.stride();
        let inner_tex =
            Texture::new(pixels.bytes_mut(), buf_w, buf_h, key.format)?.with_stride(stride)?;
        let mut inner = SwRenderer::with_scratch(inner_tex, scratch)
            .with_alpha_mode(crate::render::sw::AlphaMode::Blend);
        inner.viewport = Viewport::new(buf_w, buf_h, scale);

        draw_output_subtree(
            node,
            world,
            entities,
            idx,
            &mut inner,
            shifted_rect,
            key,
            output_plan,
            &requests,
        )?;
        Canvas::flush(&mut inner)?;
    }

    let changed = requests.has_change_requests() && write.differs_from_previous()?;
    let published = write.publish();
    if published {
        requests.commit_output(changed);
    }
    Ok(published)
}

#[allow(clippy::too_many_arguments)]
fn draw_output_subtree(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    shifted_rect: &Rect,
    key: super::offscreen::BufferKey,
    output_plan: Option<&output_plan::TexturePlan>,
    requests: &PaintRequestScope,
) -> Result<(), RenderError> {
    // Render the capture root directly; its marker must not recursively capture itself.
    let entity = key.entity;
    let offset_x = shifted_rect.x + key.local_bounds.x;
    let offset_y = shifted_rect.y + key.local_bounds.y;
    let entity_rect = Rect {
        x: -key.local_bounds.x,
        y: -key.local_bounds.y,
        w: shifted_rect.w,
        h: shifted_rect.h,
    };
    let clip = Rect::new(0, 0, key.local_bounds.w, key.local_bounds.h);

    super::widgets::follow_bounds::clip::draw(
        world,
        entity,
        entity_rect,
        Transform::IDENTITY,
        Transform3D::IDENTITY,
        renderer,
        clip,
        |renderer, clip| {
            if let Some(style) = world.get::<Style>(entity) {
                let state = resolve_widget_state(world, entity);
                let mut ctx = PaintCtx::new(
                    renderer,
                    style,
                    &entity_rect,
                    &clip,
                    Transform::IDENTITY,
                    None,
                    state,
                );
                if let Some(plan) = output_plan {
                    ctx.set_workspace(&plan.workspace);
                }
                ctx.set_layout_offset(Point::new(offset_x, offset_y));
                render_views(world, entity, &mut ctx, requests, None, *idx)?;
            }
            *idx += 1;

            let scroll = world.get::<crate::input::event::scroll::ScrollOffset>(entity);
            let (sx, sy) = scroll.map_or((offset_x, offset_y), |scroll| {
                (offset_x + scroll.x, offset_y + scroll.y)
            });
            let child_clip = if scroll.is_some()
                || world.get::<Style>(entity).is_some_and(|s| s.clip_children)
            {
                clip.intersect(&entity_rect).unwrap_or(Rect::ZERO)
            } else {
                clip
            };

            for child in &node.children {
                draw_tree_offset(
                    child,
                    world,
                    entities,
                    idx,
                    renderer,
                    &child_clip,
                    sx,
                    sy,
                    &Transform::IDENTITY,
                    &Transform3D::IDENTITY,
                    output_plan,
                    requests,
                    None,
                )?;
            }
            Ok(())
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_prepared_output(
    node: &LayoutNode,
    world: &World,
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    shifted_rect: &Rect,
    off: super::OffscreenRender,
    entity: Entity,
    outer_tf: &Transform,
) -> Result<bool, RenderError> {
    // Blit buffer back to the outer renderer through DrawCommand::Blit.
    // This is the only Canvas-style operation we need from outer; going
    // through the DrawCommand path keeps Renderer trait the single
    // entry point and lets every backend handle the blit identically.

    {
        let Some(texture) = world
            .resource::<Outputs<WidgetTexture>>()
            .and_then(|outputs| outputs.get(entity))
        else {
            return Ok(false);
        };
        let local = texture.local_bounds();
        let footprint = Rect::new(
            shifted_rect.x + local.x,
            shifted_rect.y + local.y,
            local.w,
            local.h,
        );
        let corners = outer_tf.apply_rect(footprint);
        let integral = corners.iter().all(|point| {
            let scale = crate::types::Fixed64::from_fixed(renderer.output_scale());
            let x = crate::types::Fixed64::from_fixed(point.x).mul_wide(scale);
            let y = crate::types::Fixed64::from_fixed(point.y).mul_wide(scale);
            x == crate::types::Fixed64::from_int(x.to_int())
                && y == crate::types::Fixed64::from_int(y.to_int())
        });
        let quad = if integral
            && matches!(
                outer_tf.classify(),
                crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
            ) {
            None
        } else {
            Some(corners)
        };
        let blit_cmd = DrawCommand::Blit {
            pos: Point::new(footprint.x, footprint.y),
            size: Point::new(footprint.w, footprint.h),
            transform: *outer_tf,
            quad,
            texture: texture.texture(),
            opa: off.opacity,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        renderer.draw_request(&DrawRequest::new(&blit_cmd, *clip))?;
    }
    *idx += count_nodes(node);
    Ok(true)
}

fn collect_entities_preorder(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if super::branch::is_authored_hidden(world, entity) {
        return;
    }
    out.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            collect_entities_preorder(world, child, out);
        }
    }
}

/// Run the render system: build layout → compute → emit logical-coord
/// DrawCommands. Backends convert to physical at draw time.
pub fn render(
    world: &World,
    root: Entity,
    transform: &Viewport,
    renderer: &mut dyn Renderer,
) -> Result<(), RenderError> {
    reset_paint_preparation(world);
    super::offscreen::reconcile_texture_outputs(world);
    let _outputs_batch = world
        .resource::<Outputs<WidgetTexture>>()
        .map(Outputs::begin_batch);
    let (logical_w, logical_h) = transform.logical_size();
    if let Some(snapshot) = world
        .resource::<LayoutSnapshot>()
        .filter(|snapshot| snapshot.matches(root, logical_w, logical_h))
    {
        return render_full_with(
            world,
            &snapshot.layout_tree,
            &snapshot.entities,
            logical_w,
            logical_h,
            renderer,
        );
    }
    output_plan::invalidate_structure(world);
    let Some(mut layout_tree) =
        build_layout_tree(world, root).map_err(|_| RenderError::TextLayout)?
    else {
        return Ok(());
    };
    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);
    render_full_with(
        world,
        &layout_tree,
        &entities,
        logical_w,
        logical_h,
        renderer,
    )
}

fn render_full_with(
    world: &World,
    layout_tree: &LayoutNode,
    entities: &[Entity],
    logical_w: u16,
    logical_h: u16,
    renderer: &mut dyn Renderer,
) -> Result<(), RenderError> {
    let requests = prepare_paint_requests(world)?;
    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: logical_w.into(),
        h: logical_h.into(),
    };
    let prepared = output_plan::prepare(world, layout_tree, entities, renderer)?;

    let mut idx = 0;
    draw_tree_offset(
        layout_tree,
        world,
        entities,
        &mut idx,
        renderer,
        &clip,
        Fixed::ZERO,
        Fixed::ZERO,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
        prepared.as_ref().map(|prepared| &prepared.plan),
        &requests,
        None,
    )?;
    output_plan::acknowledge(world, entities);
    requests.commit();
    Ok(())
}

pub(crate) fn render_regions_cached(
    world: &World,
    snapshot: &LayoutSnapshot,
    regions: &[crate::types::PhysicalRect],
    viewport: &Viewport,
    renderer: &mut (impl Renderer + crate::render::Canvas),
    requests: &PaintRequestScope,
) -> Result<(), RenderError> {
    let _outputs_batch = world
        .resource::<Outputs<WidgetTexture>>()
        .map(Outputs::begin_batch);
    let (width, height) = viewport.logical_size();
    let bounds = Rect::new(0, 0, width, height);
    let mut prepared = None;
    for &area in regions {
        renderer.clear(&area, &crate::types::Color::rgba(0, 0, 0, 0));
        let Some(rect) = viewport.logical_cover(area).intersect(&bounds) else {
            continue;
        };
        if prepared.is_none() {
            prepared = Some(output_plan::prepare(
                world,
                &snapshot.layout_tree,
                &snapshot.entities,
                renderer,
            )?);
        }
        draw_prepared_region(
            world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &rect,
            renderer,
            requests,
            Some(area),
            Some(&snapshot.prepared_paint),
            prepared
                .as_ref()
                .and_then(Option::as_ref)
                .map(|prepared| &prepared.plan),
        )?;
    }
    Ok(())
}

pub(crate) fn prepare_paint_requests(world: &World) -> Result<PaintRequestScope, RenderError> {
    if !crate::core::reactive::reserve_paint_requests(world.live_entity_count()) {
        return Err(RenderError::MissingWorkspace);
    }
    Ok(PaintRequestScope::new(world))
}

fn reset_paint_preparation(world: &World) {
    if let Some(snapshot) = world.resource::<LayoutSnapshot>() {
        snapshot.prepared_paint.clear();
    }
}

fn reconcile_layout_snapshot(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    reuse_visual_layout: bool,
) -> Result<Option<Box<LayoutSnapshot>>, TextLayoutFailure> {
    const MAX_LAYOUT_BINDING_UPDATES: usize = 32;
    let binding_count = world
        .storage::<super::layout_binding::LayoutBinding>()
        .map_or(0, |storage| storage.len())
        + crate::input::event::runtime::size_observer_count(world)
        + usize::from(super::widgets::follow_bounds::active(world));
    let update_budget = if world
        .query::<super::widgets::LazyList>()
        .iter()
        .next()
        .is_some()
    {
        MAX_LAYOUT_BINDING_UPDATES
    } else {
        binding_count
            .saturating_add(1)
            .min(MAX_LAYOUT_BINDING_UPDATES)
    };
    let pass_count = update_budget.saturating_add(1);

    for pass in 0..pass_count {
        let reuse = reuse_visual_layout
            && pass == 0
            && !super::branch::is_authored_hidden(world, root)
            && !crate::input::event::hit_test::geometry_changes(world).0
            && world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
                snapshot.matches(root, logical_w, logical_h)
                    && !snapshot.bindings_pending
                    && snapshot.scroll_config_current(world)
            });
        let snapshot = if reuse {
            world.take_resource_box::<LayoutSnapshot>()
        } else {
            compute_layout_snapshot(world, root, logical_w, logical_h)?
        };
        let Some(mut snapshot) = snapshot else {
            return Ok(None);
        };

        let mut idx = 0;
        write_computed_rects(&snapshot.layout_tree, world, &snapshot.entities, &mut idx);
        snapshot.prepare_geometry_plan(world);
        let followed = if let Some(plan) = &snapshot.follow_plan {
            super::widgets::follow_bounds::reconcile(
                world,
                &mut snapshot.layout_tree,
                &snapshot.entities,
                &snapshot.paths,
                plan,
                Rect::new(0, 0, logical_w, logical_h),
            )
        } else {
            false
        };
        let invoke = pass < update_budget;
        let sizes =
            match crate::input::event::runtime::capture_sizes(world, &snapshot.entities, invoke) {
                Ok(sizes) => sizes,
                Err(error) => {
                    crate::warn!(target: "mirui::layout", "size observation skipped: {:?}", error);
                    None
                }
            };
        world.put_resource_box(snapshot);
        let mut changed = followed | super::layout_binding::apply_layout_bindings(world, invoke);
        if invoke {
            changed |= super::widgets::lazy_list::reconcile(world);
        }
        if let Some(sizes) = sizes {
            changed |= sizes.changed;
            if invoke {
                sizes.dispatch(world, super::lifecycle::prepare);
            }
        }
        if !changed || !invoke {
            if changed {
                crate::warn!(target: "mirui::layout", "layout bindings did not settle after {} updates", update_budget);
            }
            let mut snapshot = world.take_resource_box::<LayoutSnapshot>();
            if let Some(snapshot) = &mut snapshot {
                snapshot.bindings_pending = changed;
            }
            return Ok(snapshot);
        }
        crate::core::reactive::flush_signal_dirty(world);
    }
    Ok(None)
}

fn record_text_layout_failure(world: &mut World, root: Entity, failure: TextLayoutFailure) {
    if let Some(outputs) = world.resource::<Outputs<super::LayoutGeometry>>() {
        outputs.reset_all();
    }
    if let Some(snapshot) = world.resource_mut::<LayoutSnapshot>() {
        snapshot.valid = false;
        snapshot.invalidate_geometry_plan();
    }
    crate::input::event::hit_test::invalidate_hit_test_geometry(world);
    world.mark_subtree_dirty(root);
    world.insert_resource(failure);
}

/// Compute layout, reporting text capacity and shaping failures before drawing.
/// The last failure is also available as a [`TextLayoutFailure`] world resource.
pub fn try_update_layout(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
) -> Result<(), TextLayoutFailure> {
    update_layout(world, root, transform, false)
}

fn update_layout(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
    reuse_visual_layout: bool,
) -> Result<(), TextLayoutFailure> {
    super::view::ViewRegistry::invalidate_queries(world);
    let (logical_w, logical_h) = transform.logical_size();

    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache.storage_mut().paths.begin_layout();
    }

    let snapshot =
        match reconcile_layout_snapshot(world, root, logical_w, logical_h, reuse_visual_layout) {
            Ok(snapshot) => snapshot,
            Err(failure) => {
                record_text_layout_failure(world, root, failure);
                return Err(failure);
            }
        };
    let Some(mut snapshot) = snapshot else {
        if let Some(outputs) = world.resource::<Outputs<super::LayoutGeometry>>() {
            outputs.reset_all();
        }
        crate::input::event::scroll::bounds::publish(world, &[]);
        world.remove_resource::<TextLayoutFailure>();
        super::view::ViewRegistry::invalidate_queries(world);
        return Ok(());
    };
    if let Err(failure) = prepare_text_geometry(world, &snapshot.entities) {
        world.put_resource_box(snapshot);
        record_text_layout_failure(world, root, failure);
        return Err(failure);
    }
    crate::input::event::hit_test::update_hit_test_geometry(
        world,
        root,
        logical_w,
        logical_h,
        &snapshot.layout_tree,
        &snapshot.entities,
    );
    super::view::ViewRegistry::invalidate_queries(world);
    if let Err(failure) = publish_text_geometry(world, &snapshot.entities) {
        world.put_resource_box(snapshot);
        record_text_layout_failure(world, root, failure);
        return Err(failure);
    }
    snapshot.prepare_geometry_plan(world);
    super::layout_geometry::publish(
        world,
        &snapshot.layout_tree,
        &snapshot.entities,
        &snapshot.paths,
        Rect::new(0, 0, logical_w, logical_h),
    );
    super::view::ViewRegistry::propagate_output_publications(world);
    snapshot.publish_scroll_bounds(world);
    world.put_resource_box(snapshot);
    world.remove_resource::<TextLayoutFailure>();
    Ok(())
}

pub(crate) fn prepare_input_geometry(
    world: &mut World,
    root: Entity,
    viewport: &Viewport,
) -> Result<(), TextLayoutFailure> {
    use crate::input::event::hit_test;
    hit_test::prepare_geometry_tracking(world);
    if super::branch::is_authored_hidden(world, root) {
        if let Some(outputs) = world.resource::<Outputs<super::LayoutGeometry>>() {
            outputs.reset_all();
        }
        hit_test::invalidate_hit_test_geometry(world);
        return Ok(());
    }
    super::view::ViewRegistry::invalidate_queries(world);
    let (width, height) = viewport.logical_size();
    let (_, visual_changed) = hit_test::geometry_changes(world);
    if visual_changed && super::widgets::lazy_list::reconcile(world) {
        crate::core::reactive::flush_signal_dirty(world);
        super::view::ViewRegistry::invalidate_queries(world);
    }
    let (layout_changed, visual_changed) = hit_test::geometry_changes(world);
    let layout_current = world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
        snapshot.matches(root, width, height)
            && !snapshot.bindings_pending
            && snapshot.scroll_config_current(world)
    });
    if layout_changed
        || !layout_current
        || (visual_changed && super::widgets::follow_bounds::active(world))
    {
        return update_layout(world, root, viewport, true);
    }
    if visual_changed || !hit_test::geometry_matches(world, root, width, height) {
        world.with_resource_box::<LayoutSnapshot, _>(|snapshot, world| {
            hit_test::update_hit_test_geometry(
                world,
                root,
                width,
                height,
                &snapshot.layout_tree,
                &snapshot.entities,
            );
            snapshot.prepare_geometry_plan(world);
            super::layout_geometry::publish(
                world,
                &snapshot.layout_tree,
                &snapshot.entities,
                &snapshot.paths,
                Rect::new(0, 0, width, height),
            );
        });
        super::view::ViewRegistry::propagate_output_publications(world);
    }
    Ok(())
}

pub(crate) fn write_computed_rects(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
) {
    if *idx < entities.len() {
        let entity = entities[*idx];
        if world
            .get::<super::ComputedRect>(entity)
            .is_none_or(|rect| rect.0 != node.rect)
        {
            world.insert(entity, super::ComputedRect(node.rect));
            world.invalidate_visual(entity);
        }
    }
    *idx += 1;
    for child in &node.children {
        write_computed_rects(child, world, entities, idx);
    }
}

/// Render only the region that intersects `dirty_rect`. Widgets outside are skipped.
pub fn render_region(
    world: &World,
    root: Entity,
    transform: &Viewport,
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
) -> Result<(), RenderError> {
    let requests = prepare_paint_requests(world)?;
    render_region_in_batch(
        world, root, transform, dirty_rect, renderer, &requests, None,
    )?;
    requests.commit();
    Ok(())
}

pub(crate) fn render_region_in_batch(
    world: &World,
    root: Entity,
    transform: &Viewport,
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
    requests: &PaintRequestScope,
    physical_clip: Option<crate::types::PhysicalRect>,
) -> Result<(), RenderError> {
    reset_paint_preparation(world);
    super::offscreen::reconcile_texture_outputs(world);
    let _outputs_batch = world
        .resource::<Outputs<WidgetTexture>>()
        .map(Outputs::begin_batch);
    let (logical_w, logical_h) = transform.logical_size();
    if let Some(snapshot) = world
        .resource::<LayoutSnapshot>()
        .filter(|snapshot| snapshot.matches(root, logical_w, logical_h))
    {
        return render_region_with(
            world,
            &snapshot.layout_tree,
            &snapshot.entities,
            dirty_rect,
            renderer,
            requests,
            physical_clip,
            None,
        );
    }
    output_plan::invalidate_structure(world);
    let Some(mut layout_tree) =
        build_layout_tree(world, root).map_err(|_| RenderError::TextLayout)?
    else {
        return Ok(());
    };
    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);
    render_region_with(
        world,
        &layout_tree,
        &entities,
        dirty_rect,
        renderer,
        requests,
        physical_clip,
        None,
    )
}

pub(crate) fn acknowledge_rendered_outputs(world: &World) {
    if let Some(snapshot) = world.resource::<LayoutSnapshot>() {
        output_plan::acknowledge(world, &snapshot.entities);
    }
}

/// Internal cached variant: caller supplies a layout tree + entity
/// preorder produced earlier in the same frame (typically by the
/// dirty walker). Skips the build / compute / collect phases.
///
/// Caller must guarantee the snapshot was produced in the same frame
/// against the same `transform` and `root`. Public `render_region` is
/// the safe entry point — this one is `pub(crate)` and only invoked
/// from App frame rendering.
pub(crate) fn render_region_cached(
    world: &World,
    snapshot: &LayoutSnapshot,
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
    requests: &PaintRequestScope,
    physical_clip: Option<crate::types::PhysicalRect>,
) -> Result<(), RenderError> {
    let _outputs_batch = world
        .resource::<Outputs<WidgetTexture>>()
        .map(Outputs::begin_batch);
    render_region_with(
        world,
        &snapshot.layout_tree,
        &snapshot.entities,
        dirty_rect,
        renderer,
        requests,
        physical_clip,
        Some(&snapshot.prepared_paint),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_region_with(
    world: &World,
    layout_tree: &LayoutNode,
    entities: &[Entity],
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
    requests: &PaintRequestScope,
    physical_clip: Option<crate::types::PhysicalRect>,
    preparation: Option<&PreparedPaint>,
) -> Result<(), RenderError> {
    let prepared = output_plan::prepare(world, layout_tree, entities, renderer)?;
    draw_prepared_region(
        world,
        layout_tree,
        entities,
        dirty_rect,
        renderer,
        requests,
        physical_clip,
        preparation,
        prepared.as_ref().map(|prepared| &prepared.plan),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_prepared_region(
    world: &World,
    layout_tree: &LayoutNode,
    entities: &[Entity],
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
    requests: &PaintRequestScope,
    physical_clip: Option<crate::types::PhysicalRect>,
    preparation: Option<&PreparedPaint>,
    output_plan: Option<&output_plan::TexturePlan>,
) -> Result<(), RenderError> {
    {
        crate::trace_span!("render.draw_tree");
        let mut idx = 0;
        let mut renderer = ScopedRenderer::new(renderer, Transform3D::IDENTITY, physical_clip);
        draw_tree_offset(
            layout_tree,
            world,
            entities,
            &mut idx,
            &mut renderer,
            dirty_rect,
            Fixed::ZERO,
            Fixed::ZERO,
            &Transform::IDENTITY,
            &Transform3D::IDENTITY,
            output_plan,
            requests,
            preparation,
        )?;
    }
    Ok(())
}

pub(crate) const DIRTY_REGION_CAPACITY: usize = 8;
pub(crate) const DIRTY_PLAN_RECT_CAPACITY: usize = DIRTY_REGION_CAPACITY * 2;

struct DirtyBounds {
    regions: super::dirty::RegionSet<DIRTY_REGION_CAPACITY>,
}

impl DirtyBounds {
    fn include(&mut self, rect: Rect) {
        if rect.w <= Fixed::ZERO || rect.h <= Fixed::ZERO {
            return;
        }
        let (x0, y0, x1, y1) = rect.pixel_bounds();
        let rect = Rect::new(x0, y0, x1 - x0, y1 - y0);
        self.regions.insert(rect);
    }
}

/// Resource exposing the last frame's plan. Read-only for probes /
/// debug overlays; production code should not depend on it.
#[derive(Clone, Debug, Default)]
pub struct LastDirtyRegions(pub DirtyRegions);

pub(crate) struct LayoutSnapshot {
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    valid: bool,
    bindings_pending: bool,
    scroll_config_revision: u64,
    maintenance_revisions: [u64; 6],
    pub(crate) layout_tree: LayoutNode,
    parked: Vec<LayoutNode>,
    reachable: Vec<Entity>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) paths: super::layout::paths::LayoutPaths,
    pub(crate) follow_plan: Option<Box<super::widgets::follow_bounds::FollowPlan>>,
    painted_damage: Vec<Entity>,
    prepared_paint: PreparedPaint,
}

impl LayoutSnapshot {
    pub(crate) fn trim_memory(&mut self) {
        self.reachable = Vec::new();
        self.paths.release();
        self.follow_plan = None;
    }

    fn invalidate_geometry_plan(&mut self) {
        self.paths.invalidate();
        if let Some(plan) = &mut self.follow_plan {
            plan.invalidate();
        }
    }

    pub(crate) fn prepare_geometry_plan(&mut self, world: &mut World) {
        if super::widgets::follow_bounds::active(world) {
            let plan = self.follow_plan.get_or_insert_with(Default::default);
            plan.prepare(world, &self.entities);
        } else {
            self.follow_plan = None;
        }
        let outputs = world.resource::<Outputs<super::LayoutGeometry>>();
        if !outputs.is_some_and(Outputs::has_requests)
            && self
                .follow_plan
                .as_ref()
                .is_none_or(|plan| plan.targets().is_empty())
        {
            self.paths.release();
            return;
        }
        let relation_revision = world
            .resource::<super::outputs::relations::OutputRelations>()
            .map_or(
                0,
                super::outputs::relations::OutputRelations::structure_revision,
            );
        let revision = self
            .follow_plan
            .as_ref()
            .map_or([0; 2], |plan| plan.revision());
        self.paths.prepare(
            &self.layout_tree,
            self.entities.len(),
            [relation_revision, revision[0], revision[1]],
            |entity| {
                outputs.is_some_and(|outputs| outputs.is_requested(entity))
                    || self
                        .follow_plan
                        .as_ref()
                        .is_some_and(|plan| plan.targets().contains(&entity))
            },
        );
    }

    fn watch_maintenance(world: &mut World) {
        world.watch_component_revision::<Children>();
        world.watch_component_revision::<Parent>();
        world.watch_component_revision::<super::Hidden>();
        world.watch_component_revision::<super::branch::CachedBranchVisibility>();
        world.watch_component_revision::<Widget>();
        world.watch_component_revision::<Style>();
    }

    fn maintenance_revisions(world: &World) -> [u64; 6] {
        [
            world.component_revision::<Children>(),
            world.component_revision::<Parent>(),
            world.component_revision::<super::Hidden>(),
            world.component_revision::<super::branch::CachedBranchVisibility>(),
            world.component_membership_revision::<Widget>(),
            world.component_membership_revision::<Style>(),
        ]
    }

    fn maintenance_current(&self, world: &World) -> bool {
        self.valid && self.maintenance_revisions == Self::maintenance_revisions(world)
    }

    fn scroll_config_current(&self, world: &World) -> bool {
        self.scroll_config_revision
            == world.component_revision::<crate::input::event::scroll::ScrollConfig>()
    }

    fn publish_scroll_bounds(&mut self, world: &mut World) {
        crate::input::event::scroll::bounds::publish(world, &self.entities);
        self.scroll_config_revision =
            world.component_revision::<crate::input::event::scroll::ScrollConfig>();
    }

    fn matches(&self, root: Entity, logical_w: u16, logical_h: u16) -> bool {
        self.valid
            && self.root == root
            && self.logical_w == logical_w
            && self.logical_h == logical_h
    }

    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
        self.invalidate_geometry_plan();
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_dirty_walk(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    parent_transform: &Transform,
    parent_3d: &Transform3D,
    output_scale: Fixed,
    scroll_offset: (Fixed, Fixed),
    scroll_clip: Option<Rect>,
    parent_scroll: Option<Entity>,
    carrier: Option<Point>,
    ancestor_moved: bool,
    ancestors_visible: bool,
    bounds: &mut DirtyBounds,
    plan: &mut DirtyRegions,
    painted_damage: &mut alloc::vec::Vec<Entity>,
    output_plan: Option<&output_plan::TexturePlan>,
    preparation: &PreparedPaint,
) {
    use super::dirty::VisualDirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let visible = ancestors_visible && !super::branch::is_effectively_hidden(world, entity);
    let shifted_rect = Rect {
        x: node.rect.x - scroll_offset.0,
        y: node.rect.y - scroll_offset.1,
        ..node.rect
    };
    let (tf, tf_3d) = prepared_paint::transforms(
        *parent_transform,
        *parent_3d,
        world,
        entity,
        shifted_rect,
        *idx,
        Some(preparation),
    );

    let pending = VisualDirty::prepare(world, entity);
    let moved = ancestor_moved || pending.is_some_and(VisualDirty::transforms_subtree);
    let is_offscreen = prepared_paint::offscreen(world, entity, *idx, Some(preparation)).is_some();
    if is_offscreen {
        let mut sub_idx = *idx + 1;
        let mut pending = pending.map(VisualDirty::content_pending);
        for child in &node.children {
            pending = merge_pending(
                pending,
                scan_subtree_dirty(child, world, entities, &mut sub_idx),
            );
        }
        if pending.is_some() || moved {
            push_entity_dirty(
                world,
                entity,
                shifted_rect,
                &tf,
                &tf_3d,
                output_scale,
                visible,
                scroll_clip,
                bounds,
                painted_damage,
                *idx,
                output_plan,
                preparation,
            );
        }

        if pending == Some(VisualDirty::New) {
            let next_gen = world
                .get::<super::OffscreenGeneration>(entity)
                .map(|g| g.0.wrapping_add(1))
                .unwrap_or(1);
            world.insert(entity, super::OffscreenGeneration(next_gen));
        }
        *idx = sub_idx;
        return;
    }

    let current_scroll = scroll::PaintedScroll::capture(
        world,
        entity,
        shifted_rect,
        scroll_clip,
        tf,
        tf_3d,
        parent_scroll,
    );
    let painted_scroll = world.get::<scroll::PaintedScroll>(entity).copied();
    let my_scroll_op = current_scroll
        .zip(painted_scroll)
        .and_then(|(current, painted)| current.shift_from(painted, carrier, output_scale));
    let scroll_moved = my_scroll_op.is_none()
        && current_scroll
            .map(|scroll| scroll.offset)
            .unwrap_or_default()
            != painted_scroll
                .map(|scroll| scroll.offset)
                .unwrap_or_default();
    let child_carrier = match (current_scroll, painted_scroll, my_scroll_op.as_ref()) {
        (_, _, Some(shift)) => carrier.map(|point| Point {
            x: point.x + shift.dx,
            y: point.y + shift.dy,
        }),
        (Some(current), Some(painted), None) if current.offset == painted.offset => carrier,
        (Some(_), _, None) => None,
        (None, _, _) => carrier,
    };

    if (pending.is_some() || moved) && my_scroll_op.is_none() {
        push_entity_dirty(
            world,
            entity,
            shifted_rect,
            &tf,
            &tf_3d,
            output_scale,
            visible,
            scroll_clip,
            bounds,
            painted_damage,
            *idx,
            output_plan,
            preparation,
        );
    }

    *idx += 1;
    if !node.children.is_empty() {
        let child_scroll = current_scroll.map_or(scroll_offset, |scroll| {
            (
                scroll_offset.0 + scroll.offset.x,
                scroll_offset.1 + scroll.offset.y,
            )
        });

        let clips_children = current_scroll.is_some()
            || world
                .get::<Style>(entity)
                .is_some_and(|style| style.clip_children);
        let child_scroll_clip = if clips_children {
            let area = transformed_clip_bounds(shifted_rect, tf, tf_3d).unwrap_or(Rect::ZERO);
            Some(scroll_clip.map_or(area, |clip| area.intersect(&clip).unwrap_or(Rect::ZERO)))
        } else {
            scroll_clip
        };

        let child_carrier =
            child_carrier.filter(|_| scroll::children_are_transferable(world, entity));
        for child in &node.children {
            collect_dirty_walk(
                child,
                world,
                entities,
                idx,
                &tf,
                &tf_3d,
                output_scale,
                child_scroll,
                child_scroll_clip,
                if current_scroll.is_some() {
                    Some(entity)
                } else {
                    parent_scroll
                },
                child_carrier,
                moved || scroll_moved,
                visible,
                bounds,
                plan,
                painted_damage,
                output_plan,
                preparation,
            );
        }
    }

    // DFS post-order so a nested inner shift executes before its outer
    // carrier; the outer then moves the already-shifted inner pixels.
    if let Some(sop) = my_scroll_op {
        let carrier = carrier.expect("a scroll transfer requires a valid ancestor mapping");
        let final_shift = RegionShift {
            area: Rect {
                x: sop.area.x + carrier.x,
                y: sop.area.y + carrier.y,
                ..sop.area
            },
            ..sop
        };
        push_scroll_strips(&final_shift, plan);
        plan.shifts.push(sop);
    }
}

/// Append the strip(s) of `area` left without source pixels after the
/// shift. dy < 0 exposes a bottom strip, dy > 0 exposes a top strip;
/// dx mirrors that on the horizontal axis. Both axes can apply.
fn push_scroll_strips(sop: &RegionShift, plan: &mut DirtyRegions) {
    let area = sop.area;
    if sop.dy < Fixed::ZERO {
        let h = -sop.dy;
        plan.rects.push(Rect {
            x: area.x,
            y: area.y + area.h - h,
            w: area.w,
            h,
        });
    } else if sop.dy > Fixed::ZERO {
        plan.rects.push(Rect {
            x: area.x,
            y: area.y,
            w: area.w,
            h: sop.dy,
        });
    }
    if sop.dx < Fixed::ZERO {
        let w = -sop.dx;
        plan.rects.push(Rect {
            x: area.x + area.w - w,
            y: area.y,
            w,
            h: area.h,
        });
    } else if sop.dx > Fixed::ZERO {
        plan.rects.push(Rect {
            x: area.x,
            y: area.y,
            w: sop.dx,
            h: area.h,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn push_entity_dirty(
    world: &World,
    entity: Entity,
    rect: Rect,
    tf: &Transform,
    tf_3d: &Transform3D,
    output_scale: Fixed,
    visible: bool,
    scroll_clip: Option<Rect>,
    bounds: &mut DirtyBounds,
    painted_damage: &mut alloc::vec::Vec<Entity>,
    position: usize,
    output_plan: Option<&output_plan::TexturePlan>,
    preparation: &PreparedPaint,
) {
    let mut curr = if visible {
        visible_paint_coverage(
            world,
            entity,
            rect,
            *tf,
            *tf_3d,
            output_scale,
            position,
            output_plan,
            Some(preparation),
        )
    } else {
        PaintCoverage::EMPTY
    };
    curr = inflated_coverage(world, entity, rect, curr);
    if world.has::<super::dirty::PaintedCoverage>(entity) {
        painted_damage.push(entity);
    }
    if let Some(clip) = scroll_clip {
        curr = curr.clipped(clip);
    }
    for rect in curr.pixel_aligned().regions() {
        bounds.include(rect);
    }
}

fn inflated_coverage(
    world: &World,
    entity: Entity,
    rect: Rect,
    coverage: PaintCoverage,
) -> PaintCoverage {
    let Some(inflate) = world.get::<super::dirty::PaintInflate>(entity) else {
        return coverage;
    };
    if inflate.left == Fixed::ZERO
        && inflate.right == Fixed::ZERO
        && inflate.top == Fixed::ZERO
        && inflate.bottom == Fixed::ZERO
    {
        return coverage;
    }
    let mut outer = if coverage.is_empty() {
        rect
    } else {
        coverage.outer
    };
    outer.x -= inflate.left;
    outer.y -= inflate.top;
    outer.w += inflate.left + inflate.right;
    outer.h += inflate.top + inflate.bottom;
    PaintCoverage::solid(outer)
}

fn merge_pending(
    left: Option<super::dirty::VisualDirty>,
    right: Option<super::dirty::VisualDirty>,
) -> Option<super::dirty::VisualDirty> {
    use super::dirty::VisualDirty;
    match (left, right) {
        (Some(VisualDirty::New), _) | (_, Some(VisualDirty::New)) => Some(VisualDirty::New),
        _ => left.or(right),
    }
}

fn scan_subtree_dirty(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
) -> Option<super::dirty::VisualDirty> {
    use super::dirty::VisualDirty;
    if *idx >= entities.len() {
        return None;
    }
    let entity = entities[*idx];
    let own_pending = VisualDirty::prepare(world, entity);
    let mut pending = own_pending.map(VisualDirty::content_pending);
    *idx += 1;
    for child in &node.children {
        pending = merge_pending(pending, scan_subtree_dirty(child, world, entities, idx));
    }
    if pending == Some(VisualDirty::New) && crate::ui::offscreen::config(world, entity).is_some() {
        let generation = world
            .get::<super::OffscreenGeneration>(entity)
            .map_or(1, |g| g.0.wrapping_add(1));
        world.insert(entity, super::OffscreenGeneration(generation));
        if let Some(outputs) = world.resource::<Outputs<WidgetTexture>>() {
            outputs.invalidate(entity);
        }
    }
    merge_pending(pending, own_pending.map(VisualDirty::parent_pending))
}

/// Prepare the bounding rect of pending damage in logical pixels. Damage remains pending until `seed_prev_rects` confirms successful drawing. Returns `Ok(None)` when there is no damage, or a layout failure before drawing.
pub fn collect_dirty_region(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
) -> Result<Option<Rect>, TextLayoutFailure> {
    Ok(collect_dirty_regions(world, root, transform)?.bounding_rect())
}

/// Prepare redraw rects and framebuffer scroll operations without acknowledging drawing. Layout failures are returned before any blit or draw.
pub fn collect_dirty_regions(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
) -> Result<DirtyRegions, TextLayoutFailure> {
    let mut plan = DirtyRegions::default();
    try_collect_dirty_regions_into(world, root, transform, &mut plan)?;
    Ok(plan)
}

/// Build a dirty plan, reporting text layout failures before any blit or draw.
pub(crate) fn try_collect_dirty_regions_into(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
    plan: &mut DirtyRegions,
) -> Result<(), TextLayoutFailure> {
    reset_paint_preparation(world);
    super::view::ViewRegistry::invalidate_queries(world);
    let (logical_w, logical_h) = transform.logical_size();
    plan.clear();

    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache.storage_mut().paths.begin_layout();
    }

    if super::branch::is_authored_hidden(world, root) {
        let had_snapshot = world.resource::<LayoutSnapshot>().is_some();
        if let Err(failure) = compute_layout_snapshot(world, root, logical_w, logical_h) {
            record_text_layout_failure(world, root, failure);
            return Err(failure);
        }
        crate::input::event::scroll::bounds::publish(world, &[]);
        super::view::ViewRegistry::invalidate_queries(world);
        if had_snapshot {
            world.invalidate_rect(Rect::new(0, 0, logical_w, logical_h));
        }
        prepare_dirty_rects(world, plan);
        if had_snapshot || !plan.rects.is_empty() {
            plan.rects.clear();
            plan.rects.push(Rect::new(0, 0, logical_w, logical_h));
            crate::input::event::hit_test::invalidate_hit_test_geometry(world);
        }
        world.remove_resource::<TextLayoutFailure>();
        return Ok(());
    }

    // Idle skip: with no visible Dirty markers the 5-step walk would just
    // re-derive last frame's outputs. Systems that mutate visible
    // state without inserting Dirty (animation helpers, mainly) own
    // the responsibility to mark themselves.
    use super::dirty::Dirty;
    let mut has_layout_dirty = super::dirty::has_visible_dirty::<Dirty>(world, root);
    let has_visual_dirty =
        super::dirty::has_visible_dirty::<super::dirty::VisualDirty>(world, root);
    if !has_layout_dirty && has_visual_dirty && super::widgets::lazy_list::reconcile(world) {
        crate::core::reactive::flush_signal_dirty(world);
        super::view::ViewRegistry::invalidate_queries(world);
        has_layout_dirty = true;
    }
    let has_exact_dirty = world
        .resource::<super::dirty::ExactDirtyRegions>()
        .is_some_and(|regions| !regions.is_empty());
    let hit_geometry_valid =
        crate::input::event::hit_test::geometry_matches(world, root, logical_w, logical_h);
    let snapshot_stale = world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
        !snapshot.matches(root, logical_w, logical_h)
            || snapshot.bindings_pending
            || !snapshot.scroll_config_current(world)
    });
    if !has_layout_dirty
        && !has_visual_dirty
        && !has_exact_dirty
        && hit_geometry_valid
        && !snapshot_stale
    {
        world.remove_resource::<TextLayoutFailure>();
        return Ok(());
    }

    if !has_layout_dirty
        && !has_visual_dirty
        && !snapshot_stale
        && world
            .resource::<LayoutSnapshot>()
            .is_some_and(|snapshot| snapshot.matches(root, logical_w, logical_h))
        && hit_geometry_valid
    {
        prepare_dirty_rects(world, plan);
        world.remove_resource::<TextLayoutFailure>();
        return Ok(());
    }

    let (_, visual_changed) = crate::input::event::hit_test::geometry_changes(world);
    let visual_only = !has_layout_dirty
        && !snapshot_stale
        && !(visual_changed && super::widgets::follow_bounds::active(world));
    let existing = match world.take_resource_box::<LayoutSnapshot>() {
        Some(snapshot) if snapshot.matches(root, logical_w, logical_h) => Some(snapshot),
        Some(snapshot) => {
            world.put_resource_box(snapshot);
            None
        }
        None => None,
    };
    let layout_recomputed = !visual_only || existing.is_none();
    let result = if visual_only {
        match existing {
            Some(snapshot) => Ok(Some(snapshot)),
            None => crate::trace_span!("dirty.layout", {
                reconcile_layout_snapshot(world, root, logical_w, logical_h, true)
            }),
        }
    } else {
        if let Some(snapshot) = existing {
            world.put_resource_box(snapshot);
        }
        crate::trace_span!("dirty.layout", {
            reconcile_layout_snapshot(world, root, logical_w, logical_h, true)
        })
    };
    let snapshot = match result {
        Ok(snapshot) => snapshot,
        Err(failure) => {
            record_text_layout_failure(world, root, failure);
            plan.clear();
            return Err(failure);
        }
    };
    let Some(mut snapshot) = snapshot else {
        if let Some(outputs) = world.resource::<Outputs<super::LayoutGeometry>>() {
            outputs.reset_all();
        }
        world.remove_resource::<TextLayoutFailure>();
        super::view::ViewRegistry::invalidate_queries(world);
        return Ok(());
    };

    let text_changed = match prepare_text_geometry(world, &snapshot.entities) {
        Ok(changed) => changed,
        Err(failure) => {
            world.put_resource_box(snapshot);
            record_text_layout_failure(world, root, failure);
            plan.clear();
            return Err(failure);
        }
    };
    // A reused snapshot and unchanged text handles produced no new component inputs.
    if layout_recomputed || text_changed {
        super::view::ViewRegistry::invalidate_queries(world);
    }
    if let Err(failure) = publish_text_geometry(world, &snapshot.entities) {
        world.put_resource_box(snapshot);
        record_text_layout_failure(world, root, failure);
        plan.clear();
        return Err(failure);
    }
    if layout_recomputed {
        snapshot.publish_scroll_bounds(world);
    }
    snapshot.prepare_geometry_plan(world);
    super::layout_geometry::publish(
        world,
        &snapshot.layout_tree,
        &snapshot.entities,
        &snapshot.paths,
        Rect::new(0, 0, logical_w, logical_h),
    );
    super::view::ViewRegistry::propagate_output_publications(world);
    let output_plan =
        output_plan::measure_for_dirty(world, &snapshot.layout_tree, &snapshot.entities);

    let mut bounds = DirtyBounds {
        regions: super::dirty::RegionSet::new(),
    };

    snapshot.painted_damage.clear();
    {
        crate::trace_span!("dirty.walk");
        let mut idx = 0;
        collect_dirty_walk(
            &snapshot.layout_tree,
            world,
            &snapshot.entities,
            &mut idx,
            &Transform::IDENTITY,
            &Transform3D::IDENTITY,
            transform.scale(),
            (Fixed::ZERO, Fixed::ZERO),
            Some(Rect::new(0, 0, logical_w, logical_h)),
            None,
            Some(Point::new(0, 0)),
            false,
            !super::branch::is_hidden_in_tree(world, root),
            &mut bounds,
            plan,
            &mut snapshot.painted_damage,
            output_plan.as_ref(),
            &snapshot.prepared_paint,
        );
    }
    output_plan::return_plan(world, output_plan);

    for entity in &snapshot.painted_damage {
        if let Some(painted) = world.get::<super::dirty::PaintedCoverage>(*entity) {
            for rect in painted.0.regions() {
                for &mapped in scroll::map_painted_damage(rect, &plan.shifts).as_slice() {
                    bounds.include(mapped);
                }
            }
        }
    }
    plan.rects.extend_from_slice(bounds.regions.as_slice());

    if !plan.shifts.is_empty() {
        visit_overlay_rects(world, |overlay| {
            if plan
                .shifts
                .iter()
                .any(|shift| overlay.intersect(&shift.area).is_some())
            {
                plan.rects.push(overlay);
                plan.rects.extend_from_slice(
                    scroll::map_painted_damage(overlay, &plan.shifts).as_slice(),
                );
            }
        });
    }

    prepare_dirty_rects(world, plan);

    crate::input::event::hit_test::update_hit_test_geometry(
        world,
        root,
        logical_w,
        logical_h,
        &snapshot.layout_tree,
        &snapshot.entities,
    );

    world.put_resource_box(snapshot);
    world.remove_resource::<TextLayoutFailure>();
    Ok(())
}

fn prepare_dirty_rects(world: &mut World, plan: &mut DirtyRegions) {
    if let Some(regions) = world.resource_mut::<super::dirty::ExactDirtyRegions>() {
        let start = plan.rects.len();
        regions.prepare_into(&mut plan.rects);
        let mut exact = super::dirty::RegionSet::<DIRTY_REGION_CAPACITY>::new();
        for &rect in &plan.rects[start..] {
            exact.insert(rect);
            for &mapped in scroll::map_painted_damage(rect, &plan.shifts).as_slice() {
                exact.insert(mapped);
            }
        }
        plan.rects.truncate(start);
        plan.rects.extend_from_slice(exact.as_slice());
    }
    plan.coalesce_overlaps();
}

fn visit_overlay_rects(world: &World, mut visit: impl FnMut(Rect)) {
    use crate::input::feedback::{OverlayCursor, OverlayRotary};
    use crate::ui::ComputedRect;
    if let Some(storage) = world.storage::<OverlayCursor>() {
        for (e, _) in storage.iter() {
            if let Some(r) = world.get::<ComputedRect>(e).map(|r| r.0) {
                if r.w > Fixed::ZERO && r.h > Fixed::ZERO {
                    visit(r);
                }
            }
        }
    }
    if let Some(storage) = world.storage::<OverlayRotary>() {
        for (e, _) in storage.iter() {
            if let Some(r) = world.get::<ComputedRect>(e).map(|r| r.0) {
                if r.w > Fixed::ZERO && r.h > Fixed::ZERO {
                    visit(r);
                }
            }
        }
    }
}

#[cfg(test)]
mod local_layout_tests;

#[cfg(test)]
mod layout_snapshot_reuse_check {
    use super::*;
    use crate::types::Dimension;
    use crate::ui::dirty::Dirty;
    use crate::ui::layout::LayoutStyle;
    use crate::ui::property::{apply_to_world, prop};
    use crate::ui::widgets::Text;
    use crate::ui::{Hidden, branch::CachedBranchVisibility};

    fn widget(world: &mut World, width: i32) -> Entity {
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        world.insert(
            entity,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(width),
                    height: Dimension::px(10),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        entity
    }

    fn child(world: &mut World, parent: Entity, text: Option<&'static str>) -> Entity {
        let entity = widget(world, 20);
        world.insert(entity, Parent(parent));
        if let Some(children) = world.get_mut::<Children>(parent) {
            children.0.push(entity);
        } else {
            world.insert(parent, Children(vec![entity]));
        }
        if let Some(text) = text {
            world.insert(entity, Text::from(text));
        }
        entity
    }

    fn text_handle(world: &World, entity: Entity) -> crate::text::TextLayoutHandle {
        *world.get::<crate::text::TextLayoutHandle>(entity).unwrap()
    }

    #[test]
    fn stable_value_refresh_matches_structural_refresh_without_allocating() {
        fn same_tree(left: &LayoutNode, right: &LayoutNode) {
            assert_eq!(left.entity, right.entity);
            assert_eq!(left.style, right.style);
            assert_eq!(left.rect, right.rect);
            assert_eq!(left.intrinsic_width, right.intrinsic_width);
            assert_eq!(left.intrinsic_height, right.intrinsic_height);
            assert_eq!(left.children.len(), right.children.len());
            for (left, right) in left.children.iter().zip(&right.children) {
                same_tree(left, right);
            }
        }

        let mut world = crate::app::App::headless(128, 128).world;
        let root = widget(&mut world, 128);
        let branch = child(&mut world, root, None);
        let label = child(&mut world, branch, Some("old"));
        let leaf = child(&mut world, root, None);
        let mut stable = compute_layout_snapshot(&mut world, root, 128, 128)
            .unwrap()
            .unwrap();
        let mut general = compute_layout_snapshot(&mut world, root, 128, 128)
            .unwrap()
            .unwrap();

        for width in [24, 28, 40, 16] {
            world.get_mut::<Style>(leaf).unwrap().layout.width = Dimension::px(width);
            world.get_mut::<Style>(label).unwrap().font_size = Some(width as u16);
            world
                .get_mut::<Text>(label)
                .unwrap()
                .set_content("new text");
            assert!(stable.maintenance_current(&world));
            world.put_resource_box(stable);
            stable = compute_layout_snapshot(&mut world, root, 128, 128)
                .unwrap()
                .unwrap();
            general.valid = false;
            world.put_resource_box(general);
            general = compute_layout_snapshot(&mut world, root, 128, 128)
                .unwrap()
                .unwrap();
            same_tree(&stable.layout_tree, &general.layout_tree);
        }

        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..16 {
                    refresh_stable_layout_tree(
                        &world,
                        &mut stable.layout_tree,
                        &mut LayoutTreeChanges::default(),
                    )
                    .unwrap();
                }
            }),
            0
        );
    }

    #[test]
    fn hidden_branches_keep_nested_storage_and_children_only_order() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let deep = widget(&mut world, 20);
        let middle = widget(&mut world, 20);
        let leaf = widget(&mut world, 20);
        let shallow = widget(&mut world, 24);
        let shallow_leaf = widget(&mut world, 12);
        let tail = widget(&mut world, 8);
        world.insert(root, Children(vec![deep, shallow, tail]));
        world.insert(deep, Children(vec![middle]));
        world.insert(middle, Children(vec![leaf]));
        world.insert(shallow, Children(vec![shallow_leaf]));
        world.insert(shallow, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert!(snapshot.parked.capacity() >= 1);
        assert!(snapshot.entities.capacity() >= 7);
        assert!(snapshot.painted_damage.capacity() >= 7);
        let root_children_ptr = snapshot.layout_tree.children.as_ptr();
        let nested_ptr = snapshot.layout_tree.children[0].children.as_ptr();
        let nested_leaf_ptr = snapshot.layout_tree.children[0].children[0]
            .children
            .as_ptr();
        assert_eq!(snapshot.entities, [root, deep, middle, leaf, tail]);
        assert_eq!(snapshot.layout_tree.children.len(), 2);
        assert_eq!(snapshot.layout_tree.children[0].entity, Some(deep));
        assert_eq!(snapshot.layout_tree.children[1].entity, Some(tail));
        assert!(
            snapshot
                .parked
                .iter()
                .any(|node| node.entity == Some(shallow))
        );

        world.insert(deep, Hidden);
        world.remove::<Hidden>(shallow);
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.entities, [root, shallow, shallow_leaf, tail]);
        assert_eq!(snapshot.layout_tree.children.as_ptr(), root_children_ptr);
        assert_eq!(snapshot.layout_tree.children[0].entity, Some(shallow));
        assert_eq!(snapshot.layout_tree.children[1].entity, Some(tail));

        world.remove::<Hidden>(deep);
        world.insert(shallow, Hidden);
        world.get_mut::<Children>(root).unwrap().0 = vec![tail, deep, shallow];
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.entities, [root, tail, deep, middle, leaf]);
        assert_eq!(snapshot.layout_tree.children.as_ptr(), root_children_ptr);
        assert_eq!(snapshot.layout_tree.children[0].entity, Some(tail));
        assert_eq!(snapshot.layout_tree.children[1].entity, Some(deep));
        assert_eq!(
            snapshot.layout_tree.children[1].children.as_ptr(),
            nested_ptr
        );
        assert_eq!(
            snapshot.layout_tree.children[1].children[0]
                .children
                .as_ptr(),
            nested_leaf_ptr
        );
    }

    #[test]
    fn parked_layout_nodes_do_not_alias_recycled_entity_generations() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let old = widget(&mut world, 16);
        world.insert(root, Children(vec![old]));
        world.insert(old, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.despawn(old));

        let replacement = widget(&mut world, 40);
        assert_eq!(replacement.id, old.id);
        assert_ne!(replacement.generation, old.generation);
        world.get_mut::<Children>(root).unwrap().0[0] = replacement;
        try_update_layout(&mut world, root, &viewport).unwrap();

        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.entities, [root, replacement]);
        assert_eq!(snapshot.layout_tree.children[0].entity, Some(replacement));
        assert_eq!(
            snapshot.layout_tree.children[0].style.width,
            Dimension::px(40)
        );
        assert!(!snapshot.parked.iter().any(|node| node.entity == Some(old)));
    }

    #[test]
    fn static_layout_does_not_reserve_parked_nodes() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let first = widget(&mut world, 16);
        let second = widget(&mut world, 16);
        world.insert(root, Children(vec![first, second]));
        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.parked.capacity(), 0);
        assert_eq!(snapshot.layout_tree.children.len(), 2);
    }

    #[test]
    fn hidden_root_discards_its_previous_layout_snapshot() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.resource::<LayoutSnapshot>().is_some());

        world.insert(root, Hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.resource::<LayoutSnapshot>().is_none());

        world.remove::<Hidden>(root);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.resource::<LayoutSnapshot>().is_some());
    }

    #[test]
    fn detached_alive_branch_does_not_remain_parked() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let hidden = widget(&mut world, 16);
        let child = widget(&mut world, 8);
        world.insert(root, Children(vec![hidden]));
        world.insert(hidden, Children(vec![child]));
        world.insert(hidden, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.resource::<LayoutSnapshot>().unwrap().parked.len() == 1);

        world.get_mut::<Children>(root).unwrap().0.clear();
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert!(world.is_alive(hidden));
        assert!(snapshot.parked.is_empty());
        assert!(snapshot.layout_tree.children.is_empty());
        assert_eq!(snapshot.entities, [root]);
    }

    #[test]
    fn parked_maintenance_tracks_structure_without_warm_allocations() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let hidden = child(&mut world, root, None);
        child(&mut world, hidden, None);
        world.insert(hidden, Hidden);
        world.insert(hidden, CachedBranchVisibility { selected: false });
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..4 {
                    assert!(
                        world
                            .resource::<LayoutSnapshot>()
                            .unwrap()
                            .maintenance_current(&world)
                    );
                    try_update_layout(&mut world, root, &viewport).unwrap();
                }
            }),
            0
        );
        for kind in 0..6 {
            match kind {
                0 => {
                    world.get_mut::<Children>(root).unwrap();
                }
                1 => {
                    world.get_mut::<Parent>(hidden).unwrap();
                }
                2 => {
                    world.get_mut::<Hidden>(hidden).unwrap();
                }
                3 => {
                    world.get_mut::<CachedBranchVisibility>(hidden).unwrap();
                }
                4 => {
                    world.get_mut::<Widget>(hidden).unwrap();
                }
                _ => {
                    world.get_mut::<Style>(hidden).unwrap();
                }
            }
            assert_eq!(
                world
                    .resource::<LayoutSnapshot>()
                    .unwrap()
                    .maintenance_current(&world),
                kind >= 4
            );
            try_update_layout(&mut world, root, &viewport).unwrap();
            let snapshot = world.resource::<LayoutSnapshot>().unwrap();
            assert!(snapshot.maintenance_current(&world));
            assert_eq!(snapshot.parked.len(), 1);
        }
        world.remove::<Style>(hidden);
        assert!(
            !world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .maintenance_current(&world)
        );
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .parked
                .is_empty()
        );
    }

    #[test]
    fn parked_refresh_reports_swaps_without_a_pool_length_change() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let first = child(&mut world, root, None);
        let second = child(&mut world, root, None);
        world.insert(second, Hidden);
        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();
        world.insert(first, Hidden);
        world.remove::<Hidden>(second);
        let mut snapshot = world.take_resource_box::<LayoutSnapshot>().unwrap();
        let mut changes = LayoutTreeChanges::default();
        let count = snapshot.parked.len();
        assert!(
            refresh_layout_tree(
                &mut world,
                root,
                &mut snapshot.layout_tree,
                &mut snapshot.parked,
                &mut changes,
            )
            .unwrap()
        );
        assert!(changes.parked);
        assert_eq!(snapshot.parked.len(), count);
        assert_eq!(snapshot.parked[0].entity, Some(first));
        assert_eq!(snapshot.layout_tree.children[0].entity, Some(second));
    }

    #[test]
    fn parked_reachability_reuses_storage_and_releases_under_memory_pressure() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let hidden = child(&mut world, root, None);
        world.insert(hidden, Hidden);
        for _ in 0..16 {
            child(&mut world, hidden, None);
        }
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap().reachable.len(),
            18
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..8 {
                    world.get_mut::<Children>(root).unwrap();
                    try_update_layout(&mut world, root, &viewport).unwrap();
                }
            }),
            0
        );
        world
            .resource_mut::<LayoutSnapshot>()
            .unwrap()
            .trim_memory();
        assert_eq!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .reachable
                .capacity(),
            0
        );
        world.get_mut::<Children>(root).unwrap().0.clear();
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert!(snapshot.parked.is_empty());
        assert_eq!(snapshot.reachable.capacity(), 0);
    }

    #[test]
    fn parked_nodes_release_removed_layout_components_and_restore_current_structure() {
        for remove_widget in [false, true] {
            for remove_root in [false, true] {
                let mut world = World::new();
                let root = widget(&mut world, 64);
                let hidden = child(&mut world, root, None);
                let removed = child(&mut world, hidden, None);
                let retained = child(&mut world, hidden, None);
                world.insert(hidden, Hidden);
                let viewport = Viewport::new(64, 64, Fixed::ONE);
                try_update_layout(&mut world, root, &viewport).unwrap();
                let target = if remove_root { hidden } else { removed };
                let style = if remove_widget {
                    world.remove::<Widget>(target);
                    None
                } else {
                    world.remove::<Style>(target)
                };
                try_update_layout(&mut world, root, &viewport).unwrap();
                let snapshot = world.resource::<LayoutSnapshot>().unwrap();
                assert_eq!(snapshot.entities, [root]);
                if remove_root {
                    assert!(snapshot.parked.is_empty());
                } else {
                    assert_eq!(snapshot.parked.len(), 1);
                    assert_eq!(snapshot.parked[0].children.len(), 1);
                    assert_eq!(snapshot.parked[0].children[0].entity, Some(retained));
                }

                if let Some(style) = style {
                    world.insert(target, style);
                } else {
                    world.insert(target, Widget);
                }
                world.get_mut::<Children>(hidden).unwrap().0.swap(0, 1);
                world.remove::<Hidden>(hidden);
                try_update_layout(&mut world, root, &viewport).unwrap();
                let snapshot = world.resource::<LayoutSnapshot>().unwrap();
                assert!(snapshot.maintenance_current(&world));
                assert!(snapshot.parked.is_empty());
                assert_eq!(snapshot.entities, [root, hidden, retained, removed]);
            }
        }
    }

    #[test]
    fn parked_children_follow_reparenting_without_duplicate_storage() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let hidden = child(&mut world, root, None);
        let visible = child(&mut world, root, None);
        let moved = child(&mut world, hidden, None);
        let leaf = child(&mut world, moved, None);
        world.insert(hidden, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        world.get_mut::<Children>(hidden).unwrap().0.clear();
        world.insert(visible, Children(vec![moved]));
        world.insert(moved, Parent(visible));
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert!(snapshot.maintenance_current(&world));
        assert_eq!(snapshot.entities, [root, visible, moved, leaf]);
        assert_eq!(snapshot.parked.len(), 1);
        assert_eq!(snapshot.parked[0].entity, Some(hidden));
        assert!(snapshot.parked[0].children.is_empty());

        world.remove::<Hidden>(hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.entities, [root, hidden, visible, moved, leaf]);
        assert!(snapshot.parked.is_empty());
    }

    #[test]
    fn failed_text_preparation_does_not_confirm_parked_maintenance() {
        use crate::ui::widgets::{Placeholder, TextInput};

        let mut app = crate::app::App::headless(96, 48);
        app.with_default_widgets();
        app.with_text_layout_limits(crate::text::TextLayoutLimits {
            text_bytes: 2,
            ..crate::text::TextLayoutLimits::default()
        });
        let root = app.spawn_root().id();
        let input = app.compose(root, |cx| cx.spawn(TextInput::build()));
        app.world.insert(input, Placeholder("A"));
        let hidden = child(&mut app.world, root, None);
        app.world.insert(hidden, Hidden);
        app.render().unwrap();
        let revisions = app
            .world
            .resource::<LayoutSnapshot>()
            .unwrap()
            .maintenance_revisions;

        app.world.get_mut::<Style>(input).unwrap();
        app.world.get_mut::<Children>(root).unwrap();
        app.world.insert(input, Placeholder("Too long"));
        app.world.invalidate(input);
        for _ in 0..2 {
            assert!(matches!(app.render_dirty(), Err(RenderError::TextLayout)));
            let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
            assert!(!snapshot.maintenance_current(&app.world));
            assert_eq!(snapshot.maintenance_revisions, revisions);
        }
        app.world.insert(input, Placeholder("A"));
        app.render_dirty().unwrap();
        let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
        assert!(snapshot.maintenance_current(&app.world));
        assert_ne!(snapshot.maintenance_revisions, revisions);
        assert_eq!(snapshot.parked.len(), 1);
    }

    #[test]
    fn initially_hidden_text_keeps_only_layout_node_storage() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let hidden = child(&mut world, root, Some("deferred"));
        world.insert(hidden, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.get::<crate::text::TextLayoutHandle>(hidden).is_none());
        assert!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .parked
                .iter()
                .any(|node| node.entity == Some(hidden))
        );

        world.remove::<Hidden>(hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.get::<crate::text::TextLayoutHandle>(hidden).is_some());
    }

    #[test]
    fn children_only_hidden_branches_restore_painted_pixels() {
        use crate::surface::FramebufferAccess;
        use crate::types::Color;

        fn pixel<F: FnMut(&[u8], crate::types::PhysicalRect)>(
            app: &mut crate::app::App<crate::surface::framebuf::FramebufSurface<F>>,
        ) -> [u8; 3] {
            let framebuffer = app.backend.framebuffer();
            let offset = (2 * 64 + 2) * 4;
            let pixels = framebuffer.read_pixels().unwrap();
            let bytes = pixels.bytes();
            [bytes[offset], bytes[offset + 1], bytes[offset + 2]]
        }

        let mut app = crate::app::App::headless(64, 32);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let red = widget(&mut app.world, 64);
        let red_child = widget(&mut app.world, 16);
        let blue = widget(&mut app.world, 64);
        app.world
            .get_mut::<Style>(red)
            .unwrap()
            .set_bg_color(Color::rgb(240, 20, 30));
        app.world
            .get_mut::<Style>(blue)
            .unwrap()
            .set_bg_color(Color::rgb(20, 70, 240));
        app.world.insert(root, Children(vec![red, blue]));
        app.world.insert(red, Children(vec![red_child]));
        app.world.insert(blue, Hidden);

        app.render().unwrap();
        assert_eq!(pixel(&mut app), [240, 20, 30]);
        app.world.insert(red, Hidden);
        app.world.remove::<Hidden>(blue);
        app.world.mark_subtree_dirty(root);
        app.render_dirty().unwrap();
        assert_eq!(pixel(&mut app), [20, 70, 240]);

        app.world.remove::<Hidden>(red);
        app.world.insert(blue, Hidden);
        app.world.mark_subtree_dirty(root);
        app.render_dirty().unwrap();
        assert_eq!(pixel(&mut app), [240, 20, 30]);
    }

    #[test]
    fn visible_property_keeps_snapshot_storage_and_never_renders_stale_text() {
        #[derive(Default)]
        struct GlyphRecorder(usize);

        impl Renderer for GlyphRecorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                if matches!(request.command, DrawCommand::GlyphRun { .. }) {
                    self.0 += 1;
                }
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = widget(&mut world, 64);
        let label = child(&mut world, root, Some("VISIBLE"));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        let snapshot_ptr = snapshot as *const LayoutSnapshot;
        let entities_ptr = snapshot.entities.as_ptr();
        let entities_capacity = snapshot.entities.capacity();
        let mut drawn = GlyphRecorder::default();
        render(&world, root, &viewport, &mut drawn).unwrap();
        assert!(drawn.0 > 0);

        apply_to_world::<prop::Visible>(&mut world, label, false);
        assert!(world.resource::<LayoutSnapshot>().is_some());
        let mut drawn = GlyphRecorder::default();
        render(&world, root, &viewport, &mut drawn).unwrap();
        assert_eq!(drawn.0, 0);
        try_update_layout(&mut world, root, &viewport).unwrap();
        let hidden = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(hidden as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(hidden.entities.as_ptr(), entities_ptr);
        assert_eq!(hidden.entities.capacity(), entities_capacity);
        assert_eq!(hidden.entities, [root]);

        apply_to_world::<prop::Visible>(&mut world, label, true);
        assert!(
            !world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .matches(root, 64, 64)
        );
        try_update_layout(&mut world, root, &viewport).unwrap();
        let mut drawn = GlyphRecorder::default();
        render(&world, root, &viewport, &mut drawn).unwrap();
        assert!(drawn.0 > 0);
        let visible = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(visible as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(visible.entities.as_ptr(), entities_ptr);
        assert_eq!(visible.entities.capacity(), entities_capacity);
        assert_eq!(visible.entities, [root, label]);
    }

    #[test]
    fn dirty_layout_reuses_snapshot_storage_when_viewport_changes() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        world.get_mut::<Style>(root).unwrap().layout.width = Dimension::Auto;
        let label = child(&mut world, root, None);
        let wide = Viewport::new(64, 64, Fixed::ONE);
        let narrow = Viewport::new(32, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &wide).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        let snapshot_ptr = snapshot as *const LayoutSnapshot;
        let entities_ptr = snapshot.entities.as_ptr();
        assert_eq!(snapshot.entities, [root, label]);

        let mut plan = DirtyRegions::default();
        try_collect_dirty_regions_into(&mut world, root, &narrow, &mut plan).unwrap();

        let resized = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(resized as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(resized.entities.as_ptr(), entities_ptr);
        assert!(resized.matches(root, 32, 64));
        assert_eq!(
            world.get::<super::super::ComputedRect>(root).unwrap().0.w,
            Fixed::from_int(32)
        );
        assert_eq!(resized.entities, [root, label]);
    }

    #[test]
    fn dirty_layout_reuses_invalidated_snapshot_after_visible_changes() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let label = child(&mut world, root, None);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        let snapshot_ptr = snapshot as *const LayoutSnapshot;
        let entities_ptr = snapshot.entities.as_ptr();
        let mut plan = DirtyRegions::default();

        apply_to_world::<prop::Visible>(&mut world, label, false);
        assert!(
            !world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .matches(root, 64, 64)
        );
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        let hidden = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(hidden as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(hidden.entities.as_ptr(), entities_ptr);
        assert_eq!(hidden.entities, [root]);

        apply_to_world::<prop::Visible>(&mut world, label, true);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        let visible = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(visible as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(visible.entities.as_ptr(), entities_ptr);
        assert_eq!(visible.entities, [root, label]);
    }

    fn layout_is_live(world: &World, handle: crate::text::TextLayoutHandle) -> bool {
        world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .get(handle)
            .is_some()
    }

    #[test]
    fn warm_layout_reuses_tree_and_entity_storage() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let first = widget(&mut world, 20);
        let second = widget(&mut world, 20);
        world.insert(root, Children(vec![first, second]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        let snapshot_ptr = snapshot as *const LayoutSnapshot;
        let tree_ptr = snapshot.layout_tree.children.as_ptr();
        let entities_ptr = snapshot.entities.as_ptr();

        try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot as *const LayoutSnapshot, snapshot_ptr);
        assert_eq!(snapshot.layout_tree.children.as_ptr(), tree_ptr);
        assert_eq!(snapshot.entities.as_ptr(), entities_ptr);
        assert_eq!(snapshot.entities, [root, first, second]);
    }

    #[test]
    fn hidden_ancestor_retires_only_its_text_layouts_before_tree_build() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let visible = child(&mut world, root, Some("visible"));
        let panel = child(&mut world, root, None);
        let hidden = child(&mut world, panel, Some("hidden"));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let visible_handle = text_handle(&world, visible);
        let hidden_handle = text_handle(&world, hidden);
        world.insert(panel, Hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();

        assert_eq!(text_handle(&world, visible), visible_handle);
        assert!(layout_is_live(&world, visible_handle));
        assert!(!layout_is_live(&world, hidden_handle));

        world.remove::<Hidden>(panel);
        try_update_layout(&mut world, root, &viewport).unwrap();
        let returned = text_handle(&world, hidden);
        assert_ne!(returned, hidden_handle);
        assert!(layout_is_live(&world, returned));
    }

    #[test]
    fn hidden_root_retires_children_only_text_layouts() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let label = widget(&mut world, 20);
        world.insert(label, Text::from("visible"));
        world.insert(root, Children(vec![label]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let handle = text_handle(&world, label);
        assert!(layout_is_live(&world, handle));

        world.insert(root, Hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(world.resource::<LayoutSnapshot>().is_none());
        assert!(!layout_is_live(&world, handle));
    }

    #[test]
    fn children_only_tree_preserves_visible_text_layout() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let label = widget(&mut world, 20);
        world.insert(label, Text::from("visible"));
        world.insert(root, Children(vec![label]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let first = text_handle(&world, label);
        try_update_layout(&mut world, root, &viewport).unwrap();

        assert_eq!(text_handle(&world, label), first);
        assert!(layout_is_live(&world, first));
    }

    #[test]
    fn visible_text_input_owner_is_retained() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let input = child(&mut world, root, None);
        world.insert(input, crate::ui::widgets::TextInput::new());
        let mut visible = Vec::new();
        collect_entities_preorder(&world, root, &mut visible);
        visible.sort_unstable_by_key(|entity| (entity.id, entity.generation));

        assert!(visible_text_owner(
            &world,
            &visible,
            text_layout_owner(input)
        ));
    }

    #[test]
    fn cached_branch_text_layout_rebuilds_when_selected_again() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let branch = child(&mut world, root, None);
        let label = child(&mut world, branch, Some("branch"));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let first = text_handle(&world, label);
        world.insert(branch, CachedBranchVisibility { selected: false });
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert!(!layout_is_live(&world, first));

        world.insert(branch, CachedBranchVisibility { selected: true });
        try_update_layout(&mut world, root, &viewport).unwrap();
        let rebuilt = text_handle(&world, label);
        assert_ne!(rebuilt, first);
        assert!(layout_is_live(&world, rebuilt));
    }

    #[test]
    fn despawned_text_owner_cannot_reuse_a_layout_handle() {
        let mut world = crate::app::App::headless(64, 64).world;
        let root = widget(&mut world, 64);
        let old = child(&mut world, root, Some("before"));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let stale = text_handle(&world, old);
        assert!(world.despawn(old));
        world.get_mut::<Children>(root).unwrap().0.clear();
        let replacement = child(&mut world, root, Some("after"));
        assert_eq!(replacement.id, old.id);
        assert_ne!(replacement.generation, old.generation);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let current = text_handle(&world, replacement);
        assert!(!layout_is_live(&world, stale));
        assert!(layout_is_live(&world, current));
        assert_ne!(current, stale);
    }

    #[test]
    fn switching_roots_retires_text_from_the_previous_tree() {
        let mut world = crate::app::App::headless(64, 64).world;
        let first_root = widget(&mut world, 64);
        let first_label = child(&mut world, first_root, Some("first"));
        let second_root = widget(&mut world, 64);
        let second_label = child(&mut world, second_root, Some("second"));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, first_root, &viewport).unwrap();
        let previous = text_handle(&world, first_label);
        try_update_layout(&mut world, second_root, &viewport).unwrap();
        assert!(!layout_is_live(&world, previous));
        assert!(layout_is_live(&world, text_handle(&world, second_label)));
    }

    #[test]
    fn detached_text_owners_are_not_visible() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let first = child(&mut world, root, Some("detached"));
        world.get_mut::<Children>(root).unwrap().0.clear();
        let mut visible = Vec::new();
        collect_entities_preorder(&world, root, &mut visible);
        visible.sort_unstable_by_key(|entity| (entity.id, entity.generation));
        assert!(!visible_text_owner(
            &world,
            &visible,
            text_layout_owner(first)
        ));
    }

    #[test]
    fn hidden_descendant_dirty_stays_idle_until_revealed() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let panel = widget(&mut world, 20);
        let leaf = widget(&mut world, 10);
        world.insert(root, Children(vec![panel]));
        world.insert(panel, Parent(root));
        world.insert(panel, Children(vec![leaf]));
        world.insert(leaf, Parent(panel));
        world.insert(panel, Hidden);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        seed_prev_rects(&mut world, root, &viewport);
        world
            .resource_mut::<LayoutSnapshot>()
            .unwrap()
            .layout_tree
            .rect
            .x = Fixed::from_int(123);
        world.insert(leaf, Dirty);
        world.insert(leaf, super::super::dirty::VisualDirty::New);

        for _ in 0..2 {
            try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
            assert!(plan.is_empty());
            assert_eq!(
                world
                    .resource::<LayoutSnapshot>()
                    .unwrap()
                    .layout_tree
                    .rect
                    .x,
                Fixed::from_int(123),
            );
        }

        world.remove::<Hidden>(panel);
        world.mark_subtree_dirty(panel);
        assert!(world.has::<Dirty>(panel));
        assert!(world.has::<Dirty>(leaf));
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(
            world
                .resource::<LayoutSnapshot>()
                .unwrap()
                .layout_tree
                .rect
                .x,
            Fixed::ZERO,
        );
        assert!(!world.has::<Dirty>(panel));
        assert!(!world.has::<Dirty>(leaf));
        assert!(world.has::<super::super::dirty::VisualDirty>(leaf));
        seed_prev_rects(&mut world, root, &viewport);
        assert!(!world.has::<super::super::dirty::VisualDirty>(leaf));
    }

    #[test]
    fn safe_area_is_minimum_root_padding() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(root, Children(vec![child]));
        world.insert(root, super::super::RespectSafeArea);
        world.insert_resource(crate::surface::SafeAreaInsets {
            top: Fixed::from_int(7),
            right: Fixed::from_int(3),
            bottom: Fixed::from_int(5),
            left: Fixed::from_int(11),
        });

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        assert_eq!(
            world.get::<super::super::ComputedRect>(child).unwrap().0,
            Rect::new(11, 7, 10, 10)
        );
        assert_eq!(
            world.get::<Style>(root).unwrap().layout.padding,
            crate::ui::layout::Padding::default(),
            "safe-area layout must not mutate authored style",
        );
    }

    #[test]
    fn authored_padding_larger_than_safe_area_is_preserved() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(root, Children(vec![child]));
        world.insert(root, super::super::RespectSafeArea);
        world.get_mut::<Style>(root).unwrap().layout.padding = crate::ui::layout::Padding::all(13);
        world.insert_resource(crate::surface::SafeAreaInsets {
            top: Fixed::from_int(7),
            right: Fixed::from_int(3),
            bottom: Fixed::from_int(5),
            left: Fixed::from_int(11),
        });

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        assert_eq!(
            world.get::<super::super::ComputedRect>(child).unwrap().0,
            Rect::new(13, 13, 10, 10)
        );
    }

    #[test]
    fn refreshed_tree_tracks_reorder_visibility_and_style() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let first = widget(&mut world, 20);
        let second = widget(&mut world, 20);
        world.insert(root, Children(vec![first, second]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        world.get_mut::<Children>(root).unwrap().0.swap(0, 1);
        world.get_mut::<Style>(second).unwrap().layout.width = Dimension::px(30);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap().entities,
            [root, second, first]
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(second).unwrap().0.x,
            Fixed::ZERO
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(first).unwrap().0.x,
            Fixed::from_int(30)
        );

        world.insert(second, Hidden);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap().entities,
            [root, first]
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(first).unwrap().0.x,
            Fixed::ZERO
        );

        world.remove::<Hidden>(second);
        try_update_layout(&mut world, root, &viewport).unwrap();
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap().entities,
            [root, second, first]
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(first).unwrap().0.x,
            Fixed::from_int(30)
        );
    }

    #[test]
    fn dirty_plan_reuses_capacity_across_active_and_idle_frames() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects.len(), 1);
        let rects_ptr = plan.rects.as_ptr();

        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects.len(), 1);
        assert_eq!(plan.rects.as_ptr(), rects_ptr);
        seed_prev_rects(&mut world, root, &viewport);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert!(plan.is_empty());
        assert_eq!(plan.rects.as_ptr(), rects_ptr);

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects.len(), 1);
        assert_eq!(plan.rects.as_ptr(), rects_ptr);
    }

    #[test]
    fn paint_acknowledgement_updates_existing_coverage_without_replacing_its_identity() {
        use super::super::dirty::PaintedCoverage;
        use core::any::TypeId;

        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(root, Children(vec![child]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();
        let kind = TypeId::of::<PaintedCoverage>();
        world.watch_component_lifetime(kind);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        seed_prev_rects(&mut world, root, &viewport);
        let generation = world.component_generation(child, kind).unwrap();
        let previous = world.get::<PaintedCoverage>(child).unwrap().0;

        crate::ui::property::apply_to_world::<crate::ui::property::prop::Transform>(
            &mut world,
            child,
            Transform::translate(Fixed::from_int(20), Fixed::ZERO),
        );
        for _ in 0..2 {
            try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
            assert_eq!(world.get::<PaintedCoverage>(child).unwrap().0, previous);
        }
        seed_prev_rects(&mut world, root, &viewport);
        assert_eq!(
            world.get::<PaintedCoverage>(child).unwrap().0.outer,
            Rect::new(20, 0, 10, 10)
        );
        assert_eq!(world.component_generation(child, kind), Some(generation));

        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..4 {
                seed_prev_rects(&mut world, root, &viewport);
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(world.component_generation(child, kind), Some(generation));
    }

    #[test]
    fn exact_dirty_rect_bypasses_layout_walk_with_a_valid_snapshot() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(root, Children(vec![child]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;
        let exact = Rect::new(7, 9, 11, 13);
        seed_prev_rects(&mut world, root, &viewport);
        world.invalidate_rect(exact);

        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

        assert_eq!(plan.rects, [exact]);
        assert!(
            world
                .resource::<super::super::dirty::ExactDirtyRegions>()
                .is_some_and(|regions| !regions.is_empty())
        );
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
            snapshot
        );
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects, [exact]);
        seed_prev_rects(&mut world, root, &viewport);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert!(plan.is_empty());
    }

    #[test]
    fn input_layout_tracks_repeated_invalidation_without_consuming_paint_damage() {
        use crate::input::event::hit_test::{geometry_changes, hit_test};
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(child, crate::ui::HitTarget);
        world.insert(root, Children(vec![child]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();
        world.invalidate(child);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        seed_prev_rects(&mut world, root, &viewport);
        let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;
        let allocations = crate::test_support::tracked_allocations(|| {
            for width in [20, 5, 20, 5] {
                world.get_mut::<Style>(child).unwrap().layout.width = Dimension::px(width);
                world.invalidate(child);
                prepare_input_geometry(&mut world, root, &viewport).unwrap();
                assert!(world.has::<Dirty>(child));
                assert_eq!(
                    world
                        .get::<super::super::dirty::PaintedCoverage>(child)
                        .unwrap()
                        .0
                        .outer,
                    Rect::new(0, 0, 10, 10)
                );
                assert_eq!(
                    hit_test(&world, root, 15.into(), 5.into(), 64, 64),
                    (width == 20).then_some(child)
                );
                assert_eq!(geometry_changes(&world), (false, false));
                prepare_input_geometry(&mut world, root, &viewport).unwrap();
                assert_eq!(
                    world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
                    snapshot
                );
            }
            for width in [20, 5] {
                try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
                let revision = world.component_revision::<Dirty>();
                world.get_mut::<Style>(child).unwrap().layout.width = Dimension::px(width);
                world.invalidate(child);
                assert_ne!(world.component_revision::<Dirty>(), revision);
                assert!(geometry_changes(&world).0);
                prepare_input_geometry(&mut world, root, &viewport).unwrap();
                assert!(world.has::<Dirty>(child));
                assert_eq!(
                    world.get::<super::super::ComputedRect>(child).unwrap().0.w,
                    Fixed::from_int(width)
                );
                try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
                assert_eq!(geometry_changes(&world), (false, false));
                assert_eq!(
                    hit_test(&world, root, 15.into(), 5.into(), 64, 64),
                    (width == 20).then_some(child)
                );
                assert_eq!(
                    world
                        .get::<super::super::dirty::PaintedCoverage>(child)
                        .unwrap()
                        .0
                        .outer,
                    Rect::new(0, 0, 10, 10)
                );
            }
        });
        assert_eq!(allocations, 0);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects, [Rect::new(0, 0, 10, 10)]);
    }

    #[test]
    fn visual_transform_reuses_layout_snapshot_and_tracks_old_and_new_bounds() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let child = widget(&mut world, 10);
        world.insert(child, crate::ui::HitTarget);
        world.insert(root, Children(vec![child]));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();

        world.insert(child, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        seed_prev_rects(&mut world, root, &viewport);
        let snapshot = world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

        crate::ui::property::apply_to_world::<crate::ui::property::prop::Transform>(
            &mut world,
            child,
            Transform::translate(Fixed::from_int(20), Fixed::ZERO),
        );
        prepare_input_geometry(&mut world, root, &viewport).unwrap();
        assert!(world.has::<super::super::dirty::VisualDirty>(child));
        assert_eq!(
            world
                .get::<super::super::dirty::PaintedCoverage>(child)
                .unwrap()
                .0
                .outer,
            Rect::new(0, 0, 10, 10)
        );
        assert_eq!(
            crate::input::event::hit_test::hit_test(&world, root, 25.into(), 5.into(), 64, 64),
            Some(child)
        );
        assert_eq!(
            crate::input::event::hit_test::hit_test(&world, root, 5.into(), 5.into(), 64, 64),
            None
        );
        assert_eq!(
            crate::input::event::hit_test::geometry_changes(&world),
            (false, false)
        );
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                prepare_input_geometry(&mut world, root, &viewport).unwrap();
            }
        });
        assert_eq!(allocations, 0);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

        assert_eq!(
            plan.rects,
            [Rect::new(0, 0, 10, 10), Rect::new(20, 0, 10, 10)]
        );
        assert_eq!(plan.bounding_rect(), Some(Rect::new(0, 0, 30, 10)));
        assert_eq!(
            world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
            snapshot
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(child).unwrap().0,
            Rect::new(0, 0, 10, 10)
        );
    }

    #[test]
    fn sparse_dirty_regions_keep_bounded_flush_rects() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let children: Vec<_> = (0..5).map(|_| widget(&mut world, 10)).collect();
        world.insert(root, Children(children.clone()));
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        seed_prev_rects(&mut world, root, &viewport);

        world.insert(children[0], Dirty);
        world.insert(children[2], Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(
            plan.rects,
            [Rect::new(0, 0, 10, 10), Rect::new(20, 0, 10, 10)]
        );
        assert_eq!(plan.bounding_rect(), Some(Rect::new(0, 0, 30, 10)));

        for child in children {
            world.insert(child, Dirty);
        }
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert_eq!(plan.rects, [Rect::new(0, 0, 50, 10)]);
    }

    #[test]
    fn previous_scroll_rect_scratch_reuses_capacity() {
        let mut world = World::new();
        let root = widget(&mut world, 64);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut plan = DirtyRegions::default();
        world.insert(
            root,
            super::super::dirty::PaintedCoverage(crate::ui::dirty::PaintCoverage::solid(
                Rect::new(0, 0, 64, 10),
            )),
        );

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.painted_damage.len(), 1);
        let scratch_ptr = snapshot.painted_damage.as_ptr();

        world.insert(root, Dirty);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        assert_eq!(snapshot.painted_damage.len(), 1);
        assert_eq!(snapshot.painted_damage.as_ptr(), scratch_ptr);
    }
}

#[cfg(all(test, feature = "std"))]
mod text_layout_check {
    use alloc::rc::Rc;

    use super::*;
    use crate::render::font::{
        Font, FontFaceId, FontManager, FontMetrics, FontProvider, FontStack, FontToken, GlyphId,
        RasterGlyph,
    };
    use crate::types::{Dimension, Viewport};
    use crate::ui::layout::{FlexDirection, LayoutStyle, Padding};
    use crate::ui::widgets::Text;

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let entity = world.spawn_empty();
        world.insert(entity, Widget);
        world.insert(entity, style);
        if let Some(parent) = parent {
            world.insert(entity, Parent(parent));
            if let Some(children) = world.get_mut::<Children>(parent) {
                children.0.push(entity);
            } else {
                world.insert(parent, Children(vec![entity]));
            }
        }
        entity
    }

    fn world() -> World {
        crate::app::App::headless(64, 64).world
    }

    #[test]
    fn content_size_uses_shaped_font_metrics() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("abc"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(24));
        assert_eq!(rect.h, Fixed::from_int(8));
    }

    #[test]
    fn text_intrinsic_size_includes_padding() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    padding: Padding {
                        top: Dimension::px(3),
                        right: Dimension::px(4),
                        bottom: Dimension::px(3),
                        left: Dimension::px(4),
                    },
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("abc"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(32));
        assert_eq!(rect.h, Fixed::from_int(14));
    }

    #[test]
    fn constrained_text_height_tracks_wrapped_lines() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(24),
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("ab cd"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(24));
        assert_eq!(rect.h, Fixed::from_int(16));
        let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        let cache = &storage.layouts;
        assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
    }

    #[test]
    fn text_padding_reduces_wrap_width_and_expands_content_height() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(32),
                    height: Dimension::Content,
                    padding: Padding {
                        top: Dimension::px(2),
                        right: Dimension::px(4),
                        bottom: Dimension::px(2),
                        left: Dimension::px(4),
                    },
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("ab cd"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(32));
        assert_eq!(rect.h, Fixed::from_int(20));
        let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        let cache = &storage.layouts;
        assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
    }

    #[test]
    fn text_paints_from_the_padded_content_origin() {
        #[derive(Default)]
        struct Recorder {
            pos: Option<Point>,
            text_run: Option<crate::render::TextRunIdentity>,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                if let DrawCommand::GlyphRun { pos, .. } = request.command {
                    self.pos = Some(*pos);
                }
                Ok(())
            }

            fn submit_text_run(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
                identity: crate::render::TextRunIdentity,
            ) -> Result<(), RenderError> {
                self.text_run = Some(identity);
                self.submit(draw)
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(32),
                    height: Dimension::px(20),
                    padding: Padding {
                        top: Dimension::px(3),
                        right: Dimension::px(4),
                        bottom: Dimension::px(3),
                        left: Dimension::px(4),
                    },
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::new("abc").with_capacity(8).unwrap());
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let mut recorder = Recorder::default();
        render(&world, root, &viewport, &mut recorder).unwrap();

        assert_eq!(recorder.pos, Some(Point::new(4, 3)));
        assert_eq!(
            recorder.text_run,
            Some(crate::render::TextRunIdentity {
                entity: label,
                run_index: 0,
            })
        );

        world.get_mut::<Text>(label).unwrap().set_content("def");
        try_update_layout(&mut world, root, &viewport).unwrap();
        let mut recorder = Recorder::default();
        render(&world, root, &viewport, &mut recorder).unwrap();
        assert_eq!(
            recorder.text_run,
            Some(crate::render::TextRunIdentity {
                entity: label,
                run_index: 0,
            })
        );

        world.insert(label, Text::from("abc"));
        try_update_layout(&mut world, root, &viewport).unwrap();
        let mut recorder = Recorder::default();
        render(&world, root, &viewport, &mut recorder).unwrap();
        assert_eq!(recorder.pos, Some(Point::new(4, 3)));
        assert_eq!(recorder.text_run, None);
    }

    #[test]
    fn clipped_text_uses_its_own_rect_as_draw_clip() {
        #[derive(Default)]
        struct Recorder {
            clip: Option<Rect>,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                if matches!(request.command, DrawCommand::GlyphRun { .. }) {
                    self.clip = Some(request.clip);
                }
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(16),
                    height: Dimension::px(8),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(
            label,
            Text::from("ABCD").with_paragraph(crate::ui::widgets::ParagraphStyle {
                wrap: crate::ui::widgets::TextWrap::NoWrap,
                ..Default::default()
            }),
        );
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let mut recorder = Recorder::default();
        render(&world, root, &viewport, &mut recorder).unwrap();
        assert_eq!(
            recorder.clip,
            Some(world.get::<super::super::ComputedRect>(label).unwrap().0)
        );
    }

    #[test]
    fn rendering_reuses_the_constrained_text_layout_snapshot() {
        #[derive(Default)]
        struct Recorder {
            fills: Vec<Rect>,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                if let DrawCommand::Fill { area, .. } = request.command {
                    self.fills.push(*area);
                }
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(24),
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("ab cd"));
        spawn(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(crate::types::Color::rgb(1, 2, 3).into()),
                layout: LayoutStyle {
                    width: Dimension::px(8),
                    height: Dimension::px(8),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();
        let mut recorder = Recorder::default();
        render(&world, root, &viewport, &mut recorder).unwrap();

        assert!(recorder.fills.iter().any(|area| {
            area.x == Fixed::ZERO
                && area.y == Fixed::from_int(16)
                && area.w == Fixed::from_int(8)
                && area.h == Fixed::from_int(8)
        }));
    }

    #[test]
    fn intrinsic_text_size_does_not_replace_flex_growth() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Row,
                    width: Dimension::px(64),
                    height: Dimension::px(16),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Auto,
                    height: Dimension::Content,
                    grow: Fixed::ONE,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("abc"));
        let trailing = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(8),
                    height: Dimension::px(8),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );

        try_update_layout(&mut world, root, &Viewport::new(64, 16, Fixed::ONE)).unwrap();

        assert_eq!(
            world.get::<super::super::ComputedRect>(label).unwrap().0,
            Rect::new(0, 0, 56, 8)
        );
        assert_eq!(
            world.get::<super::super::ComputedRect>(trailing).unwrap().0,
            Rect::new(56, 0, 8, 8)
        );
    }

    #[test]
    fn max_lines_constrains_content_height() {
        let mut world = world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(16),
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(
            label,
            Text::from("ab cd ef").with_paragraph(crate::ui::widgets::ParagraphStyle {
                max_lines: Some(2),
                ..Default::default()
            }),
        );

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.h, Fixed::from_int(16));
        let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        let cache = &storage.layouts;
        assert_eq!(cache.get(*handle).unwrap().lines().len(), 2);
    }

    struct CjkFace;

    impl FontProvider for CjkFace {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(9)
        }

        fn map_char(&self, ch: char) -> Option<GlyphId> {
            (ch == '日').then_some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(10))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            _layout_ppem: u16,
            _output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            None
        }

        fn metrics(&self, _ppem: u16) -> FontMetrics {
            FontMetrics {
                ascender: Fixed::from_int(8),
                descender: Fixed::from_int(-2),
                line_height: Fixed::from_int(10),
            }
        }
    }

    #[test]
    fn font_stack_falls_back_at_cluster_boundaries() {
        static FALLBACKS: [FontToken; 1] = [FontToken::Custom("cjk")];

        let mut world = world();
        world.resource::<FontManager>().unwrap().add_static(
            "cjk",
            Font {
                family: "cjk",
                size: 10,
                provider: Rc::new(CjkFace),
            },
        );
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                font_stack: FontStack::new(FontToken::Default).with_fallbacks(&FALLBACKS[..]),
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("A日"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(18));
        let handle = world.get::<crate::text::TextLayoutHandle>(label).unwrap();
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        let cache = &storage.layouts;
        let layout = cache.get(*handle).unwrap();
        assert_eq!(layout.runs().len(), 2);
        assert_eq!(
            layout.runs()[0].font_id(),
            world
                .resource::<FontManager>()
                .unwrap()
                .resolve(FontToken::Default.cache_key())
                .face_id()
        );
        assert_eq!(layout.runs()[1].font_id(), FontFaceId::new(9));
    }

    struct PpemFace {
        id: u64,
        character: char,
    }

    impl FontProvider for PpemFace {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(self.id)
        }

        fn map_char(&self, ch: char) -> Option<GlyphId> {
            (ch == self.character).then_some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(i32::from(ppem) / 2))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            _layout_ppem: u16,
            _output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            None
        }

        fn metrics(&self, ppem: u16) -> FontMetrics {
            FontMetrics {
                ascender: Fixed::from_int(i32::from(ppem) * 3 / 4),
                descender: Fixed::from_int(-(i32::from(ppem) / 4)),
                line_height: Fixed::from_int(i32::from(ppem)),
            }
        }
    }

    #[test]
    fn style_font_size_drives_intrinsic_measurement() {
        let mut world = world();
        world.resource::<FontManager>().unwrap().add_static(
            "scalable",
            Font {
                family: "scalable",
                size: 8,
                provider: Rc::new(PpemFace {
                    id: 10,
                    character: 'A',
                }),
            },
        );
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                font_stack: FontStack::new(FontToken::Custom("scalable")),
                font_size: Some(20),
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("AAA"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(30));
        assert_eq!(rect.h, Fixed::from_int(20));
    }

    #[test]
    fn fallback_faces_share_the_paragraph_ppem() {
        static FALLBACKS: [FontToken; 1] = [FontToken::Custom("fallback-scalable")];

        let mut world = world();
        for (token, family, size, id, character) in [
            ("primary-scalable", "primary-scalable", 8, 11, 'A'),
            ("fallback-scalable", "fallback-scalable", 30, 12, '日'),
        ] {
            world.resource::<FontManager>().unwrap().add_static(
                token,
                Font {
                    family,
                    size,
                    provider: Rc::new(PpemFace { id, character }),
                },
            );
        }
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(64),
                    height: Dimension::px(64),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                font_stack: FontStack::new(FontToken::Custom("primary-scalable"))
                    .with_fallbacks(&FALLBACKS[..]),
                font_size: Some(20),
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("A日"));

        try_update_layout(&mut world, root, &Viewport::new(64, 64, Fixed::ONE)).unwrap();

        let rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        assert_eq!(rect.w, Fixed::from_int(20));
        assert_eq!(rect.h, Fixed::from_int(20));
    }

    #[test]
    fn path_text_intrinsic_size_ignores_widget_padding() {
        let mut app = crate::app::App::headless(96, 96);
        app.with_default_widgets();
        let mut world = app.world;
        let mut baseline = crate::render::path::Path::new();
        baseline
            .move_to(Point::new(0, 24))
            .line_to(Point::new(64, 24));
        let path = world
            .resource_mut::<crate::render::path::PathStore>()
            .unwrap()
            .insert(baseline)
            .unwrap();
        let plain = spawn(&mut world, None, Style::default());
        world.insert(plain, Text::from("ABC"));
        world.widget_mut(plain).unwrap().text_path(path);
        let padded_style = Style {
            layout: LayoutStyle {
                padding: Padding::all(Dimension::px(12)),
                ..LayoutStyle::default()
            },
            ..Style::default()
        };
        let padded_layout = padded_style.layout;
        let padded = spawn(&mut world, None, padded_style);
        world.insert(padded, Text::from("ABC"));
        world.widget_mut(padded).unwrap().text_path(path);

        let mut plain_node = LayoutNode::new(LayoutStyle::default());
        apply_text_intrinsic(&world, plain, &mut plain_node).unwrap();
        let mut padded_node = LayoutNode::new(padded_layout);
        apply_text_intrinsic(&world, padded, &mut padded_node).unwrap();

        assert_eq!(padded_node.intrinsic_width, plain_node.intrinsic_width);
        assert_eq!(padded_node.intrinsic_height, plain_node.intrinsic_height);
    }

    #[test]
    fn input_layout_is_prepared_before_paint_and_tracks_its_display_content() {
        use crate::prelude::*;
        use crate::ui::ComputedRect;
        use crate::ui::widgets::Placeholder;
        use crate::ui::widgets::TextInput;
        use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

        #[crate::compose]
        fn input() -> Entity {
            crate::ui!(TextInput(width: 64, height: 20) [Placeholder("Type")] {})
        }

        fn prepared(world: &World, input: Entity) -> (crate::text::TextLayoutHandle, usize) {
            let handle = world.get::<TextEditingLayoutHandle>(input).unwrap().0;
            let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
            (handle, storage.layouts.get(handle).unwrap().glyphs().len())
        }

        let mut app = crate::app::App::headless(96, 48);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        assert!(!prepare_text_geometry(&mut app.world, &[root]).unwrap());
        let input = app.compose(root, input());
        let viewport = Viewport::new(96, 48, Fixed::ONE);
        try_update_layout(&mut app.world, root, &viewport).unwrap();
        let (placeholder, glyphs) = prepared(&app.world, input);
        assert_eq!(glyphs, 4);
        assert!(!prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
        app.world.remove::<TextEditingLayoutHandle>(input);
        assert!(prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
        assert_eq!(prepared(&app.world, input), (placeholder, 4));
        let editing = app
            .world
            .remove::<super::super::widgets::TextEditing>(input)
            .unwrap();
        let rect = app.world.get::<ComputedRect>(input).unwrap().0;
        assert!(prepare_input_layout(&mut app.world, input, rect).unwrap());
        assert!(!prepare_input_layout(&mut app.world, input, rect).unwrap());
        assert!(!prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
        app.world.insert(input, editing);
        assert!(prepare_text_geometry(&mut app.world, &[root, input]).unwrap());
        assert_eq!(prepared(&app.world, input).1, 4);
        for _ in 0..3 {
            try_update_layout(&mut app.world, root, &viewport).unwrap();
            assert_eq!(prepared(&app.world, input), (placeholder, 4));
        }
        app.render_dirty().unwrap();

        app.world.get_mut::<Placeholder>(input).unwrap().0 = "Name";
        app.render_dirty().unwrap();
        assert_ne!(prepared(&app.world, input).0, placeholder);
        assert_eq!(prepared(&app.world, input).1, 4);
        let document = app
            .add_model(crate::ui::widgets::TextDocument::new())
            .finish();
        app.world.insert(
            input,
            crate::ui::widgets::TextEditing::new(document.clone()),
        );
        document.set_text("AB").unwrap();
        app.render_dirty().unwrap();
        let (content, glyphs) = prepared(&app.world, input);
        assert_eq!(glyphs, 2);
        assert_ne!(content, placeholder);
        app.world
            .get_mut::<crate::ui::widgets::TextEditing>(input)
            .unwrap()
            .set_selection(crate::ui::widgets::TextSelection::default())
            .unwrap();
        app.render_dirty().unwrap();
        assert_eq!(prepared(&app.world, input), (content, 2));

        app.world.get_mut::<Style>(input).unwrap().layout.width = Dimension::px(32);
        app.render_dirty().unwrap();
        let (resized, _) = prepared(&app.world, input);
        assert_ne!(resized, content);
        assert_eq!(
            app.world.get::<ComputedRect>(input).unwrap().0.w,
            Fixed::from_int(32)
        );
        document.set_text("").unwrap();
        app.render_dirty().unwrap();
        assert_eq!(prepared(&app.world, input).1, 4);
        app.world.remove::<Placeholder>(input);
        app.render_dirty().unwrap();
        assert_eq!(prepared(&app.world, input).1, 0);

        app.world
            .remove_resource::<crate::render::font::FontManager>();
        app.handle_memory_warning();
        app.render().unwrap();
        assert!(app.world.get::<TextEditingLayoutHandle>(input).is_none());
    }

    #[test]
    fn input_layout_recycles_bounded_storage_across_content_updates() {
        use crate::text::{TextLayoutCapacity, WorkspaceCapacity};
        use crate::ui::widgets::TextInput;
        use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

        let mut app = crate::app::App::headless(64, 32);
        app.with_default_widgets();
        app.require_text_layout_capacity(TextLayoutCapacity {
            layout_slots: 2,
            measurements: 0,
            lines: 3,
            runs: 3,
            glyphs: 6,
            carets: 12,
            workspace: WorkspaceCapacity {
                runs: 2,
                glyphs: 2,
                scratch_glyphs: 2,
                lines: 1,
            },
        })
        .unwrap();
        let root = app.spawn_root().id();
        let input = app.compose(root, |cx| cx.spawn(TextInput::build()));
        let document = app
            .add_model(crate::ui::widgets::TextDocument::new())
            .finish();
        app.world.insert(
            input,
            crate::ui::widgets::TextEditing::new(document.clone()),
        );
        for byte in b'A'..=b'Z' {
            document
                .set_text(core::str::from_utf8(&[byte]).unwrap())
                .unwrap();
            app.render_dirty().unwrap();
            let handle = app.world.get::<TextEditingLayoutHandle>(input).unwrap().0;
            let cache = app
                .world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage();
            assert_eq!(cache.layouts.get(handle).unwrap().glyphs().len(), 1);
        }
    }

    #[test]
    fn input_layout_failure_is_reported_before_paint() {
        use crate::prelude::*;
        use crate::ui::widgets::Placeholder;
        use crate::ui::widgets::TextInput;
        use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

        #[crate::compose]
        fn input() -> Entity {
            crate::ui!(TextInput(width: 64, height: 20) [Placeholder("Type")] {})
        }
        let mut app = crate::app::App::headless(96, 48);
        app.with_default_widgets();
        app.with_text_layout_limits(crate::text::TextLayoutLimits {
            text_bytes: 2,
            ..crate::text::TextLayoutLimits::default()
        });
        let root = app.spawn_root().id();
        let input = app.compose(root, input());
        assert!(matches!(app.render(), Err(RenderError::TextLayout)));
        assert_eq!(app.last_text_layout_failure().unwrap().entity, input);
        assert!(app.world.get::<TextEditingLayoutHandle>(input).is_none());
        assert!(!app.world.resource::<LayoutSnapshot>().unwrap().valid);
        app.world.remove::<Placeholder>(input);
        app.render_dirty().unwrap();
        assert!(app.last_text_layout_failure().is_none());
        assert!(app.world.get::<TextEditingLayoutHandle>(input).is_some());
        assert!(app.world.resource::<LayoutSnapshot>().unwrap().valid);
        assert!(crate::input::event::hit_test::geometry_matches(
            &app.world, root, 96, 48
        ));
        app.world.insert(input, Placeholder("Type"));
        for _ in 0..2 {
            assert!(matches!(app.dirty_regions(), Err(RenderError::TextLayout)));
            assert_eq!(app.last_text_layout_failure().unwrap().entity, input);
            assert!(!app.world.resource::<LayoutSnapshot>().unwrap().valid);
            assert!(!crate::input::event::hit_test::geometry_matches(
                &app.world, root, 96, 48
            ));
        }
        app.world.remove::<Placeholder>(input);
        app.render_dirty().unwrap();
        assert!(app.last_text_layout_failure().is_none());
        assert!(app.world.get::<TextEditingLayoutHandle>(input).is_some());
        assert!(app.world.resource::<LayoutSnapshot>().unwrap().valid);
    }

    #[test]
    fn path_placements_are_ready_before_paint_and_survive_idle_and_visual_updates() {
        use crate::render::path::{Path, PathId, PathStore};
        use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
        use crate::text::{PathCaretGeometry, TextLayoutHandle, TextPath};

        mod demand {
            use super::*;

            #[crate::component]
            #[depends_on(TextGeometry)]
            pub(super) struct GeometryDemand;

            #[crate::view(component = GeometryDemand, name = "GeometryDemand")]
            pub(super) fn geometry_demand() {}
        }

        #[crate::compose]
        fn label(path: PathId) -> Entity {
            crate::ui!(Text("AB", width: 64, height: 16) [
                TextPath::new(path).with_range(Fixed::ZERO..Fixed::from_int(64)),
                demand::GeometryDemand,
            ] {} )
        }

        fn key(world: &World, label: Entity, path: PathId) -> PlacementKey {
            PlacementKey::new(
                label,
                *world.get::<TextLayoutHandle>(label).unwrap(),
                *world.get::<TextPath>(label).unwrap(),
                world
                    .resource::<PathStore>()
                    .unwrap()
                    .revision(path)
                    .unwrap(),
                DEFAULT_TOLERANCE,
            )
        }

        let mut paths = PathStore::new(1).unwrap();
        let mut line = Path::new();
        line.move_to(Point::new(0, 20)).line_to(Point::new(64, 20));
        let path = paths.insert(line).unwrap();
        let mut app = crate::app::App::headless(96, 96);
        app.with_default_widgets()
            .with_widget(demand::geometry_demand::view())
            .add_resource(paths);
        let root = app.spawn_root().id();
        let label = app.compose(root, label(path));
        let viewport = Viewport::new(96, 96, Fixed::ONE);
        try_update_layout(&mut app.world, root, &viewport).unwrap();
        let initial_key = key(&app.world, label, path);
        let initial_origin = app
            .world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .paths
            .glyph_frames(initial_key, |frames| frames[0].local_origin)
            .unwrap();
        let initial_handle = *app.world.get::<TextLayoutHandle>(label).unwrap();
        let mut carets = [PathCaretGeometry::default(); 8];
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let geometry = app
                    .world
                    .resource::<Outputs<TextGeometry>>()
                    .unwrap()
                    .get(label)
                    .unwrap();
                assert!(!geometry.carets_into(&mut carets).unwrap().is_empty());
                assert!(
                    crate::ui::widgets::text::path_text_ink_bounds(
                        &app.world,
                        label,
                        app.world
                            .get::<super::super::ComputedRect>(label)
                            .unwrap()
                            .0,
                        Transform::IDENTITY,
                        Fixed::ONE,
                    )
                    .is_some()
                );
            }),
            0
        );

        let mut damage = DirtyRegions::default();
        try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
        app.render().unwrap();
        for _ in 0..3 {
            try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
            assert!(damage.rects.is_empty());
            assert!(
                app.world
                    .resource::<Outputs<TextGeometry>>()
                    .unwrap()
                    .storage()
                    .paths
                    .glyph_frames(initial_key, |frames| !frames.is_empty())
                    .unwrap()
            );
        }

        app.world
            .resource_mut::<PathStore>()
            .unwrap()
            .edit(path, |line| {
                line.set_command(0, crate::render::path::PathCmd::MoveTo(Point::new(0, 28)))
                    .unwrap();
                line.set_command(1, crate::render::path::PathCmd::LineTo(Point::new(64, 28)))
                    .unwrap();
            })
            .unwrap();
        app.world.invalidate_visual(label);
        assert!(!super::super::dirty::has_visible_dirty::<
            super::super::dirty::Dirty,
        >(&app.world, root));
        try_collect_dirty_regions_into(&mut app.world, root, &viewport, &mut damage).unwrap();
        assert_eq!(
            *app.world.get::<TextLayoutHandle>(label).unwrap(),
            initial_handle
        );
        let current_key = key(&app.world, label, path);
        {
            let outputs = app.world.resource::<Outputs<TextGeometry>>().unwrap();
            let storage = outputs.storage();
            assert!(storage.paths.glyph_frames(initial_key, |_| ()).is_none());
            assert_eq!(
                storage
                    .paths
                    .glyph_frames(current_key, |frames| frames[0].local_origin.y)
                    .unwrap(),
                initial_origin.y + (8 << 8)
            );
            assert!(
                !outputs
                    .get(label)
                    .unwrap()
                    .carets_into(&mut carets)
                    .unwrap()
                    .is_empty()
            );
        }

        app.world
            .remove_resource::<crate::render::font::FontManager>();
        app.handle_memory_warning();
        app.render().unwrap();
        assert!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .storage()
                .paths
                .glyph_frames(current_key, |_| ())
                .is_none()
        );
        assert!(
            app.world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .is_none()
        );
    }

    #[test]
    fn path_text_ink_drives_culling_and_damage_outside_layout_bounds() {
        use crate::text::{PathSelectionRibbon, PathTextGeometryError};

        mod demand {
            use super::*;

            #[crate::component]
            #[depends_on(TextGeometry)]
            pub(super) struct GeometryDemand;

            #[crate::view(component = GeometryDemand, name = "GeometryDemand")]
            pub(super) fn geometry_demand() {}
        }

        #[derive(Default)]
        struct Recorder {
            posed_runs: usize,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                if matches!(request.command, DrawCommand::PosedGlyphRun { .. }) {
                    self.posed_runs += 1;
                }
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(96, 96);
        app.with_default_widgets()
            .with_widget(demand::geometry_demand::view());
        let mut world = app.world;
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Column,
                    width: Dimension::px(96),
                    height: Dimension::px(96),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Content,
                    height: Dimension::Content,
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("ABCDEFG"));
        world.insert(label, demand::GeometryDemand);
        let mut baseline = crate::render::path::Path::new();
        baseline
            .move_to(Point::new(0, 48))
            .line_to(Point::new(64, 48));
        let path = world
            .resource_mut::<crate::render::path::PathStore>()
            .unwrap()
            .insert(baseline)
            .unwrap();
        world.widget_mut(label).unwrap().text_path(path);
        let viewport = Viewport::new(96, 96, Fixed::ONE);

        try_update_layout(&mut world, root, &viewport).unwrap();

        let layout = world.get::<super::super::ComputedRect>(label).unwrap().0;
        let ink = crate::ui::widgets::text::path_text_ink_bounds(
            &world,
            label,
            layout,
            Transform::IDENTITY,
            Fixed::ONE,
        )
        .unwrap();
        assert!(ink.y >= layout.y + layout.h);

        let mut ribbons = [PathSelectionRibbon::default(); 3];
        let local_selection = {
            let geometry = world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            assert_eq!(geometry.transform(), Transform3D::IDENTITY);
            let hit = geometry
                .hit_test(Point::new(16, 48), Fixed::from_int(2))
                .unwrap()
                .unwrap();
            assert_eq!(hit.text_offset(), 2);
            assert_eq!(hit.bidi_level(), 0);
            let selection = geometry.selection_into(1..3, &mut ribbons).unwrap();
            assert_eq!(selection.len(), 2);
            assert_eq!(selection[0].text_range(), 1..2);
            assert_eq!(selection[1].text_range(), 2..3);
            assert!(
                selection
                    .iter()
                    .all(|ribbon| ribbon.quad()[0] != ribbon.quad()[1])
            );
            let sentinel = PathSelectionRibbon::default();
            let mut insufficient = [sentinel; 1];
            assert_eq!(
                geometry.selection_into(1..3, &mut insufficient),
                Err(PathTextGeometryError::InsufficientCapacity {
                    required: 2,
                    provided: 1,
                })
            );
            assert_eq!(insufficient, [sentinel; 1]);
            [selection[0], selection[1]]
        };

        let translated = Transform::translate(Fixed::from_int(10), Fixed::from_int(5));
        world.insert(label, WidgetTransform(translated));
        try_update_layout(&mut world, root, &viewport).unwrap();
        {
            let geometry = world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            let probe = geometry
                .transform()
                .apply_point(Point::new(16, 48))
                .unwrap();
            assert_eq!(probe, Point::new(26, 53));
            let hit = geometry
                .hit_test(probe, Fixed::from_int(2))
                .unwrap()
                .unwrap();
            assert_eq!(hit.text_offset(), 2);
            assert_eq!(hit.bidi_level(), 0);
            let miss = geometry
                .transform()
                .apply_point(Point::new(16, 65))
                .unwrap();
            assert!(
                geometry
                    .hit_test(miss, Fixed::from_int(2))
                    .unwrap()
                    .is_none()
            );
            assert_eq!(
                geometry.selection_into(1..3, &mut ribbons).unwrap(),
                &local_selection
            );
        }
        let projective =
            Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(320));
        world.insert(label, WidgetTransform3D(projective));
        try_update_layout(&mut world, root, &viewport).unwrap();
        {
            let geometry = world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            let probe = geometry
                .transform()
                .apply_point(Point::new(16, 48))
                .unwrap();
            let hit = geometry
                .hit_test(probe, Fixed::from_int(2))
                .unwrap()
                .unwrap();
            assert_eq!(hit.text_offset(), 2);
            assert_eq!(
                geometry.selection_into(1..3, &mut ribbons).unwrap(),
                &local_selection
            );
        }
        let mut behind = Transform3D::IDENTITY;
        behind.m22 = -crate::types::Fixed64::ONE;
        world.insert(label, WidgetTransform3D(behind));
        try_update_layout(&mut world, root, &viewport).unwrap();
        {
            let geometry = world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            assert_eq!(
                geometry.hit_test(Point::ZERO, Fixed::ONE),
                Err(PathTextGeometryError::InvalidProjection)
            );
            assert_eq!(
                geometry.selection_into(1..3, &mut ribbons).unwrap(),
                &local_selection
            );
        }
        world.remove::<WidgetTransform>(label);
        world.insert(label, WidgetTransform3D(projective));
        world.insert(
            root,
            crate::input::event::scroll::ScrollOffset {
                x: Fixed::from_int(3),
                y: Fixed::from_int(2),
            },
        );
        try_update_layout(&mut world, root, &viewport).unwrap();
        let shifted_layout = Rect {
            x: layout.x - Fixed::from_int(3),
            y: layout.y - Fixed::from_int(2),
            ..layout
        };
        let widget_projection =
            effective_transform_3d(&Transform3D::IDENTITY, &world, label, shifted_layout);
        let widget_probe = widget_projection.apply_point(Point::new(13, 46)).unwrap();
        {
            let geometry = world
                .resource::<Outputs<TextGeometry>>()
                .unwrap()
                .get(label)
                .unwrap();
            assert_eq!(
                geometry.transform().apply_point(Point::new(16, 48)),
                Some(widget_probe)
            );
            assert_eq!(
                geometry
                    .hit_test(widget_probe, Fixed::from_int(2))
                    .unwrap()
                    .unwrap()
                    .text_offset(),
                2
            );
            assert_eq!(
                geometry.selection_into(1..3, &mut ribbons).unwrap(),
                &local_selection
            );
        }
        world.remove::<WidgetTransform3D>(label);
        world.remove::<crate::input::event::scroll::ScrollOffset>(root);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let mut recorder = Recorder::default();
        render_region(&world, root, &viewport, &ink, &mut recorder).unwrap();
        assert_eq!(recorder.posed_runs, 1);

        let damage = collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .unwrap();
        assert!(damage.y + damage.h >= ink.y + ink.h);
    }

    #[test]
    fn path_text_uses_the_ancestor_clip_without_changing_placed_geometry() {
        #[derive(Default)]
        struct Recorder {
            clip: Option<Rect>,
            origin: Option<Point>,
        }

        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                if let DrawCommand::PosedGlyphRun { pos, .. } = request.command {
                    self.clip = Some(request.clip);
                    self.origin = Some(*pos);
                }
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut app = crate::app::App::headless(96, 96);
        app.with_default_widgets();
        let mut world = app.world;
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(96),
                    height: Dimension::px(96),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let clipper = spawn(
            &mut world,
            Some(root),
            Style {
                clip_children: true,
                layout: LayoutStyle {
                    width: Dimension::px(10),
                    height: Dimension::px(30),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        let label = spawn(
            &mut world,
            Some(clipper),
            Style {
                layout: LayoutStyle {
                    padding: Padding::all(Dimension::px(5)),
                    ..LayoutStyle::default()
                },
                ..Style::default()
            },
        );
        world.insert(label, Text::from("ABC"));
        let mut baseline = crate::render::path::Path::new();
        baseline
            .move_to(Point::new(0, 20))
            .quad_to(Point::new(32, 4), Point::new(64, 20));
        let path = world
            .resource_mut::<crate::render::path::PathStore>()
            .unwrap()
            .insert(baseline)
            .unwrap();
        world.widget_mut(label).unwrap().text_path(path);
        let viewport = Viewport::new(96, 96, Fixed::ONE);
        try_update_layout(&mut world, root, &viewport).unwrap();

        let label_rect = world.get::<super::super::ComputedRect>(label).unwrap().0;
        let uncut_ink = crate::ui::widgets::text::path_text_ink_bounds(
            &world,
            label,
            label_rect,
            Transform::IDENTITY,
            Fixed::ONE,
        )
        .unwrap();
        let clip = world.get::<super::super::ComputedRect>(clipper).unwrap().0;
        assert!(
            uncut_ink.x + uncut_ink.w > clip.x + clip.w,
            "ink {uncut_ink:?} must extend beyond clip {clip:?}"
        );

        let mut recorder = Recorder::default();
        render_region(
            &world,
            root,
            &viewport,
            &Rect::new(0, 0, 96, 96),
            &mut recorder,
        )
        .unwrap();
        assert_eq!(recorder.clip, Some(clip));
        assert_eq!(
            recorder.origin,
            Some(Point {
                x: label_rect.x,
                y: label_rect.y,
            })
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod clip_children_check {
    extern crate std;
    use super::*;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Color, Dimension, Viewport};
    use crate::ui::layout::{LayoutStyle, Position};
    use crate::ui::{Children, Parent, Style, Widget};

    fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(children) = world.get_mut::<Children>(p) {
                children.0.push(e);
            } else {
                world.insert(p, Children(std::vec![e]));
            }
        }
        e
    }

    fn make_world() -> World {
        {
            let mut app = crate::app::App::headless(64, 64);
            app.with_default_widgets();
            app.world
        }
    }

    fn vp() -> Viewport {
        Viewport::new(64, 64, Fixed::ONE)
    }

    #[test]
    fn child_scroll_and_clip_do_not_change_leaf_paint() {
        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(17, 31, 49).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let leaf = spawn_widget(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(201, 101, 51).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(7)),
                    top: Dimension::Px(Fixed::from_int(9)),
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let draw = |world: &World| {
            let mut bytes = std::vec![0; 64 * 64 * 4];
            let texture = Texture::new(&mut bytes, 64, 64, ColorFormat::RGBA8888).unwrap();
            let mut renderer = SwRenderer::new(texture);
            super::render(world, root, &vp(), &mut renderer).unwrap();
            drop(renderer);
            bytes
        };
        let expected = draw(&world);
        world.get_mut::<Style>(leaf).unwrap().clip_children = true;
        world.insert(
            leaf,
            crate::input::event::scroll::ScrollOffset {
                x: Fixed::from_int(100),
                y: Fixed::from_int(-100),
            },
        );
        assert_eq!(draw(&world), expected);
    }

    #[test]
    fn clip_children_clips_oversize_inner_rect() {
        // 8-px-wide mask with clip_children=true holding a 64-px-wide inner
        // child: the right 56 px must show only root bg, not the child.
        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mask = spawn_widget(
            &mut world,
            Some(root),
            Style {
                clip_children: true,
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        spawn_widget(
            &mut world,
            Some(mask),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(0)),
                    top: Dimension::Px(Fixed::from_int(0)),
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer).unwrap();

        for x in 0..8 {
            let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
            assert_eq!(
                c.r, 255,
                "x={x} should be red (inside mask), got rgb({}, {}, {})",
                c.r, c.g, c.b
            );
        }
        for x in 8..64 {
            let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
            assert_eq!(
                c.r, 0,
                "x={x} should be black (clipped), got rgb({}, {}, {})",
                c.r, c.g, c.b
            );
        }
    }

    #[test]
    fn no_clip_children_lets_inner_overflow() {
        // Default (clip_children=false): inner paints across full 64 px.
        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mask = spawn_widget(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        spawn_widget(
            &mut world,
            Some(mask),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(0)),
                    top: Dimension::Px(Fixed::from_int(0)),
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer).unwrap();

        for x in 0..64 {
            let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
            assert_eq!(c.r, 255, "x={x} should be red (no clip), got r={}", c.r);
        }
    }

    #[test]
    fn clip_children_zero_width_hides_inner() {
        // Repro for the slider-at-ratio=0 bug: a clip_children mask with
        // width=0 should hide the inner entirely, not leak any pixel.
        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mask = spawn_widget(
            &mut world,
            Some(root),
            Style {
                clip_children: true,
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::ZERO),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        spawn_widget(
            &mut world,
            Some(mask),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(0)),
                    top: Dimension::Px(Fixed::from_int(0)),
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer).unwrap();

        for x in 0..64 {
            let c = renderer.target.read_pixels().unwrap().get_pixel(x, 32);
            assert_eq!(
                c.r, 0,
                "x={x} leaked red ({}), mask width=0 must hide it",
                c.r
            );
        }
    }

    #[test]
    fn dirty_region_uses_2d_transform_bbox() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(crate::types::Color::rgb(255, 255, 255).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(10)),
                    height: Dimension::Px(Fixed::from_int(10)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let viewport = vp();
        // Settle initial Style registration before measuring transform-only damage.
        collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("initial layout damage");
        seed_prev_rects(&mut world, root, &viewport);
        assert!(!world.has::<Dirty>(root));
        world.insert(
            target,
            WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
        );
        world.insert(target, Dirty);

        let dirty = collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("dirty region");

        assert_eq!(dirty.x, Fixed::from_int(15));
        assert_eq!(dirty.y, Fixed::from_int(15));
        assert_eq!(dirty.w, Fixed::from_int(20));
        assert_eq!(dirty.h, Fixed::from_int(20));
    }

    #[test]
    fn seeded_prev_rect_uses_2d_transform_bbox() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(crate::types::Color::rgb(255, 255, 255).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(10)),
                    height: Dimension::Px(Fixed::from_int(10)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let viewport = vp();
        // Settle the initial frame before seeding a transformed presentation.
        collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("initial layout damage");
        assert!(!world.has::<Dirty>(root));
        world.insert(
            target,
            WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
        );
        seed_prev_rects(&mut world, root, &viewport);

        world.insert(target, WidgetTransform(Transform::IDENTITY));
        world.insert(target, Dirty);
        let dirty = collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("dirty region");

        assert_eq!(dirty.x, Fixed::from_int(15));
        assert_eq!(dirty.y, Fixed::from_int(15));
        assert_eq!(dirty.w, Fixed::from_int(20));
        assert_eq!(dirty.h, Fixed::from_int(20));
    }

    #[test]
    fn projective_dirty_and_previous_bounds_include_affine_ancestors() {
        use crate::types::{Transform, Transform3D};
        use crate::ui::dirty::{Dirty, PaintedCoverage};
        use crate::ui::widgets::transform::WidgetTransform;
        use crate::ui::widgets::transform_3d::WidgetTransform3D;

        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(18)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(12)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            root,
            WidgetTransform(Transform::translate(Fixed::from_int(7), Fixed::from_int(3))),
        );
        world.insert(
            target,
            WidgetTransform3D(Transform3D::rotate_y_perspective(
                Fixed::from_int(18),
                Fixed::from_int(320),
            )),
        );
        let viewport = vp();
        try_update_layout(&mut world, root, &viewport).unwrap();
        collect_dirty_region(&mut world, root, &viewport).unwrap();

        let root_rect = world.get::<super::super::ComputedRect>(root).unwrap().0;
        let target_rect = world.get::<super::super::ComputedRect>(target).unwrap().0;
        let (root_affine, root_projective) = render_transforms(
            Transform::IDENTITY,
            Transform3D::IDENTITY,
            &world,
            root,
            root_rect,
        );
        let (target_affine, target_projective) =
            render_transforms(root_affine, root_projective, &world, target, target_rect);
        let expected = visual_bounds(
            &world,
            target,
            target_rect,
            target_affine,
            target_projective,
            viewport.scale(),
        );
        let (x0, y0, x1, y1) = expected.pixel_bounds();
        let expected = Rect::new(x0, y0, x1 - x0, y1 - y0);

        seed_prev_rects(&mut world, root, &viewport);
        assert_eq!(
            world.get::<PaintedCoverage>(target).unwrap().0.outer,
            expected
        );

        world.insert(target, Dirty);
        assert_eq!(
            collect_dirty_region(&mut world, root, &viewport).unwrap(),
            Some(expected)
        );
    }

    #[test]
    fn background_blur_expands_visual_dirty_bounds() {
        use crate::ui::widgets::BackgroundBlur;

        let mut world = make_world();
        let entity = world.spawn_empty();
        world.insert(entity, BackgroundBlur::new(4).with_spread(2));
        let bounds = visual_bounds(
            &world,
            entity,
            Rect::new(20, 24, 10, 8),
            Transform::IDENTITY,
            Transform3D::IDENTITY,
            Fixed::ONE,
        );

        assert_eq!(bounds, Rect::new(14, 18, 22, 20));
    }

    #[test]
    fn style_border_expands_transformed_visual_dirty_bounds() {
        use crate::types::Color;

        let mut world = make_world();
        let entity = world.spawn_empty();
        world.insert(
            entity,
            Style {
                border_color: Some(Color::rgb(255, 255, 255).into()),
                border_width: Fixed::from_int(2),
                ..Default::default()
            },
        );
        let bounds = visual_bounds(
            &world,
            entity,
            Rect::new(20, 24, 10, 8),
            Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
            Transform3D::IDENTITY,
            Fixed::ONE,
        );

        assert_eq!(bounds, Rect::new(38, 46, 24, 20));
    }

    #[test]
    fn dirty_render_clears_pixels_from_previous_2d_transform() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(20)),
                    top: Dimension::Px(Fixed::from_int(20)),
                    width: Dimension::Px(Fixed::from_int(10)),
                    height: Dimension::Px(Fixed::from_int(10)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let viewport = vp();
        world.insert(
            target,
            WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
        );
        let mut buf = std::vec![0u8; 64 * 64 * 4];
        {
            let tex =
                Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, root, &viewport, &mut renderer).unwrap();
            assert_eq!(
                renderer.target.read_pixels().unwrap().get_pixel(16, 16).b,
                255
            );
        }
        seed_prev_rects(&mut world, root, &viewport);

        world.insert(
            target,
            WidgetTransform(Transform::scale(
                Fixed::ONE / Fixed::from_int(2),
                Fixed::ONE / Fixed::from_int(2),
            )),
        );
        world.insert(target, Dirty);
        let dirty = collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("dirty region");
        {
            let tex =
                Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, root, &viewport, &dirty, &mut renderer).unwrap();
            assert_eq!(
                renderer.target.read_pixels().unwrap().get_pixel(16, 16).b,
                0
            );
            assert_eq!(
                renderer.target.read_pixels().unwrap().get_pixel(25, 25).b,
                255
            );
        }
    }

    #[test]
    fn dirty_region_covers_rotated_2d_transform_pixels() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        let mut world = make_world();
        let root = spawn_widget(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let target = spawn_widget(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(22)),
                    top: Dimension::Px(Fixed::from_int(22)),
                    width: Dimension::Px(Fixed::from_int(20)),
                    height: Dimension::Px(Fixed::from_int(12)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(
            target,
            WidgetTransform(Transform::rotate_deg(Fixed::from_int(45))),
        );
        world.insert(target, Dirty);

        let viewport = vp();
        let mut full = std::vec![0u8; 64 * 64 * 4];
        {
            let tex = Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, root, &viewport, &mut renderer).unwrap();
        }

        let dirty = collect_dirty_region(&mut world, root, &viewport)
            .unwrap()
            .expect("dirty region");
        let mut region = std::vec![0u8; 64 * 64 * 4];
        {
            let tex = Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, root, &viewport, &dirty, &mut renderer).unwrap();
        }

        let full_tex =
            Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let region_tex =
            Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        for y in 0..64 {
            for x in 0..64 {
                let full_blue = full_tex.read_pixels().unwrap().get_pixel(x, y).b == 255;
                let region_blue = region_tex.read_pixels().unwrap().get_pixel(x, y).b == 255;
                assert_eq!(
                    region_blue, full_blue,
                    "dirty render mismatch at ({x},{y}), dirty={:?}",
                    dirty,
                );
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod hidden_check {
    extern crate std;
    use super::*;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Color, Dimension, Viewport};
    use crate::ui::layout::LayoutStyle;
    use crate::ui::{Children, Hidden, Parent, Style, Widget};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(c) = world.get_mut::<Children>(p) {
                c.0.push(e);
            } else {
                world.insert(p, Children(std::vec![e]));
            }
        }
        e
    }

    fn make_world() -> World {
        {
            let mut app = crate::app::App::headless(64, 64);
            app.with_default_widgets();
            app.world
        }
    }

    #[test]
    fn hidden_widget_does_not_paint() {
        let mut world = make_world();
        let root = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let child = spawn(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(child, Hidden);

        let mut buf = std::vec![0u8; 32 * 32 * 4];
        let tex =
            Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, root, &viewport, &mut renderer).unwrap();

        for x in 0..32 {
            let c = renderer.target.read_pixels().unwrap().get_pixel(x, 16);
            assert_eq!(
                c.r, 0,
                "x={x} should be root bg (black), Hidden child leaked red ({})",
                c.r
            );
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod disabled_state_check {
    extern crate std;
    use super::*;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Color, Dimension, Viewport};
    use crate::ui::layout::LayoutStyle;
    use crate::ui::theme::{Theme, WidgetState};
    use crate::ui::{Children, Parent, Style, UserState, Widget};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(c) = world.get_mut::<Children>(p) {
                c.0.push(e);
            } else {
                world.insert(p, Children(std::vec![e]));
            }
        }
        e
    }

    fn make_world() -> World {
        {
            let mut app = crate::app::App::headless(32, 32);
            app.with_default_widgets();
            app.world
        }
    }

    #[test]
    fn disabled_subtree_paints_blended_raw_bg() {
        let mut world = make_world();
        let theme = world
            .resource::<Theme>()
            .expect("Theme present after with_default_widgets")
            .clone();
        let red = Color::rgb(248, 81, 73);
        let root = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(red.into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(root, UserState::Disabled);

        let mut buf = std::vec![0u8; 32 * 32 * 4];
        let tex =
            Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, root, &viewport, &mut renderer).unwrap();

        let expected = theme.blend_color_in(red, WidgetState::Disabled);
        let actual = renderer.target.read_pixels().unwrap().get_pixel(16, 16);
        assert_eq!(
            actual.r, expected.r,
            "r mismatch: got {} want {}",
            actual.r, expected.r
        );
        assert_eq!(
            actual.g, expected.g,
            "g mismatch: got {} want {}",
            actual.g, expected.g
        );
        assert_eq!(
            actual.b, expected.b,
            "b mismatch: got {} want {}",
            actual.b, expected.b
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod offscreen_render_check {
    extern crate std;
    use super::*;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Color, Dimension, Viewport};
    use crate::ui::layout::LayoutStyle;
    use crate::ui::offscreen::BufferKey;
    use crate::ui::{Children, OffscreenRender, Outputs, Parent, Style, Widget, WidgetTexture};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(c) = world.get_mut::<Children>(p) {
                c.0.push(e);
            } else {
                world.insert(p, Children(std::vec![e]));
            }
        }
        e
    }

    fn make_world() -> World {
        let mut app = crate::app::App::headless(64, 64);
        // Tests in this module exercise the offscreen-render path,
        // which needs an actual cache. App ctor leaves the pool
        // disabled (budget = 0) so the production default doesn't
        // pretend to know the right size for any given target.
        app.with_default_widgets()
            .with_offscreen_pool_budget(64 * 1024);
        app.world
    }

    #[test]
    fn output_subtree_uses_supplied_renderer_without_committing_or_replaying() {
        struct Target {
            fills: alloc::vec::Vec<(Rect, Rect)>,
            fail: bool,
        }
        impl Renderer for Target {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                let DrawCommand::Fill { area, .. } = request.command else {
                    panic!("unexpected output command");
                };
                self.fills.push((*area, request.clip));
                if self.fail {
                    Err(RenderError::BackendFailure)
                } else {
                    Ok(())
                }
            }

            fn flush(&mut self) -> Result<(), RenderError> {
                panic!("the producer, not the subtree painter, commits the target");
            }
        }

        let mut world = make_world();
        let root = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(1, 2, 3).into()),
                layout: LayoutStyle {
                    width: Dimension::px(8),
                    height: Dimension::px(8),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        spawn(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(4, 5, 6).into()),
                layout: LayoutStyle {
                    width: Dimension::px(4),
                    height: Dimension::px(4),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(root, OffscreenRender::default());
        try_update_layout(&mut world, root, &Viewport::new(8, 8, Fixed::ONE)).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        let rect = Rect::new(0, 0, 8, 8);
        let key = BufferKey::for_widget(
            root,
            rect,
            Rect::new(-2, -1, 12, 10),
            OffscreenRender::default(),
            1,
        );
        for fail in [false, true] {
            let mut renderer = Target {
                fills: alloc::vec::Vec::with_capacity(2),
                fail,
            };
            let mut index = 0;
            let requests = PaintRequestScope::new(&world);
            let result = draw_output_subtree(
                &snapshot.layout_tree,
                &world,
                &snapshot.entities,
                &mut index,
                &mut renderer,
                &rect,
                key,
                None,
                &requests,
            );
            assert_eq!(
                result,
                if fail {
                    Err(RenderError::BackendFailure)
                } else {
                    Ok(())
                }
            );
            assert_eq!(index, if fail { 0 } else { 2 });
            assert_eq!(renderer.fills.len(), if fail { 1 } else { 2 });
            assert_eq!(
                renderer.fills[0],
                (Rect::new(2, 1, 8, 8), Rect::new(0, 0, 12, 10))
            );
            if !fail {
                assert_eq!(renderer.fills[1].0, Rect::new(2, 1, 4, 4));
            }
            assert!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(root)
                    .is_none()
            );
        }
    }

    #[crate::component]
    struct OffscreenReadProbe {
        #[depends_on(WidgetTexture)]
        source: Entity,
        fail: bool,
        calls: core::cell::Cell<usize>,
    }

    fn probe_offscreen_reads(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
        let probe = world.get::<OffscreenReadProbe>(entity).unwrap();
        if !probe.fail {
            assert!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(entity)
                    .is_none(),
                "a subtree under construction must not be readable"
            );
        }
        let source = world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(probe.source)
            .expect("a completed source stays readable while another subtree renders");
        assert_eq!(
            source.texture().read_pixels().unwrap().get_pixel(1, 1),
            Color::rgb(64, 128, 255)
        );
        probe.calls.set(probe.calls.get() + 1);
        if probe.fail {
            paint.fail(RenderError::BackendFailure);
        }
    }

    fn offscreen_read_probe_world(fail: bool) -> (World, Entity, Entity, Entity) {
        let mut world = make_world();
        world.resource_mut::<ViewRegistry>().unwrap().insert(
            crate::ui::View::new("OffscreenReadProbe", 100, probe_offscreen_reads)
                .with_filter::<OffscreenReadProbe>(),
        );
        let panel_style = Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(8)),
                ..Default::default()
            },
            ..Default::default()
        };
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::px(16),
                    height: Dimension::px(16),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let source = spawn(&mut world, Some(root), panel_style.clone());
        let consumer = spawn(&mut world, Some(root), panel_style);
        for entity in [source, consumer] {
            world.insert(entity, OffscreenRender::default());
        }
        world.insert(
            consumer,
            OffscreenReadProbe {
                source,
                fail,
                calls: core::cell::Cell::new(0),
            },
        );
        ViewRegistry::reconcile_observations(&mut world);
        (world, root, source, consumer)
    }

    #[test]
    fn offscreen_subtree_reads_only_completed_sources() {
        let (mut world, root, _, consumer) = offscreen_read_probe_world(false);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut pixels = [0; 64 * 64 * 4];
        let texture =
            Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(texture);
        let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
        super::render(&world, root, &viewport, &mut renderer).unwrap();
        assert_eq!(
            world
                .get::<OffscreenReadProbe>(consumer)
                .unwrap()
                .calls
                .get(),
            1
        );
        assert_eq!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(consumer)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(1, 1),
            Color::rgb(64, 128, 255)
        );
    }

    #[test]
    fn offscreen_subtree_failure_preserves_completed_sources() {
        let (mut world, root, source, consumer) = offscreen_read_probe_world(false);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut pixels = [0; 64 * 64 * 4];
        let texture =
            Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(texture);
        let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
        super::render(&world, root, &viewport, &mut renderer).unwrap();
        super::seed_prev_rects(&mut world, root, &viewport);
        let revision = world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .unwrap()
            .texture()
            .cache_revision;
        let probe = world.get_mut::<OffscreenReadProbe>(consumer).unwrap();
        probe.fail = true;
        probe.calls.set(0);
        world.invalidate_visual(consumer);
        let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert_eq!(
            super::render(&world, root, &viewport, &mut renderer),
            Err(RenderError::BackendFailure)
        );
        assert_eq!(
            world
                .get::<OffscreenReadProbe>(consumer)
                .unwrap()
                .calls
                .get(),
            1
        );
        assert!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(consumer)
                .is_none()
        );
        let source = world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .expect("completed source remains cached");
        let texture = source.texture();
        assert_eq!(texture.cache_revision, revision);
        assert_eq!(
            texture.read_pixels().unwrap().get_pixel(1, 1),
            Color::rgb(64, 128, 255)
        );
    }

    #[test]
    fn offscreen_prerender_does_not_submit_to_parent_target() {
        struct PreparationTarget;

        impl Renderer for PreparationTarget {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                panic!("source preparation must not write to the parent target");
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }

            fn offscreen_format(&self) -> Option<ColorFormat> {
                Some(ColorFormat::RGBA8888)
            }
        }

        let (mut world, root, source, _) = offscreen_read_probe_world(false);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::try_update_layout(&mut world, root, &viewport).unwrap();
        let snapshot = world.resource::<LayoutSnapshot>().unwrap();
        for _ in 0..2 {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            let _batch = outputs.begin_batch();
            let _prepared = super::output_plan::prepare(
                &world,
                &snapshot.layout_tree,
                &snapshot.entities,
                &mut PreparationTarget,
            )
            .unwrap();
            assert_eq!(
                world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(source)
                    .unwrap()
                    .texture()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(1, 1),
                Color::rgb(64, 128, 255)
            );
        }
    }

    #[test]
    fn offscreen_render_publishes_completed_subtree() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(panel).expect("completed subtree output");
        assert_eq!((output.texture().width, output.texture().height), (32, 32));
        assert_eq!(
            output.texture().read_pixels().unwrap().get_pixel(1, 1),
            Color::rgb(64, 128, 255)
        );
    }

    #[test]
    fn offscreen_production_does_not_read_parent_framebuffer() {
        struct FailingRead;

        impl Renderer for FailingRead {
            type TargetState = ();

            fn route(
                &self,
                _: &::mirui::render::DrawRequest<'_, '_>,
            ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
                Ok(::mirui::render::RenderRoute::Native)
            }

            fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
                Ok(())
            }

            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }

            fn supports_offscreen(&self) -> bool {
                true
            }

            fn offscreen_format(&self) -> Option<ColorFormat> {
                Some(ColorFormat::RGBA8888)
            }

            fn read_target_region(
                &self,
                _src: &Rect,
                _dst: &mut Texture,
            ) -> Result<(), RenderError> {
                Err(RenderError::BackendFailure)
            }
        }

        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut FailingRead).unwrap();
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(outputs.get(panel).is_some());

        let mut pixels = std::vec![0; 64 * 64 * 4];
        let texture =
            Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(texture);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
        assert!(outputs.get(panel).is_some());
    }

    #[test]
    fn offscreen_render_pixels_match_inline_render() {
        // The same panel rendered with and without OffscreenRender at
        // scale=1.0 should produce identical pixels (up to ±1 alpha
        // for blend rounding). 32×32 solid blue panel.
        let blue = Color::rgb(64, 128, 255);

        // (a) Inline: no OffscreenRender.
        let mut buf_inline = std::vec![0u8; 64 * 64 * 4];
        {
            let mut world = make_world();
            let panel = spawn(
                &mut world,
                None,
                Style {
                    bg_color: Some(blue.into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(32)),
                        height: Dimension::Px(Fixed::from_int(32)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, panel, &viewport, &mut renderer).unwrap();
        }

        // (b) Offscreen at scale=1.0.
        let mut buf_off = std::vec![0u8; 64 * 64 * 4];
        {
            let mut world = make_world();
            let panel = spawn(
                &mut world,
                None,
                Style {
                    bg_color: Some(blue.into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(32)),
                        height: Dimension::Px(Fixed::from_int(32)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(panel, OffscreenRender::default());
            let tex = Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, panel, &viewport, &mut renderer).unwrap();
        }

        // Compare a center pixel of the panel; both should be blue.
        let pi = (16 * 64 + 16) * 4;
        let po = (16 * 64 + 16) * 4;
        for c in 0..3 {
            let d = (buf_inline[pi + c] as i32 - buf_off[po + c] as i32).abs();
            assert!(
                d <= 2,
                "channel {c}: inline {} vs offscreen {} differ by {d}",
                buf_inline[pi + c],
                buf_off[po + c]
            );
        }
    }

    #[test]
    fn offscreen_render_with_scale_half_creates_smaller_buffer() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(panel).unwrap();
        assert_eq!((output.texture().width, output.texture().height), (16, 16));
    }

    /// Children inside an OffscreenRender + scale=0.5 subtree must
    /// land at upscaled coordinates after blit. A 40×20 child painted
    /// at logical (5, 5) inside a 128×114 panel should appear on the
    /// outer framebuffer at physical (5, 5) → (45, 25) — same as
    /// inline rendering. If the inner viewport.scale is not threaded
    /// through, the child paints at full physical 40×20 inside a 64×57
    /// buffer, then 2× upscale doubles its on-screen size to 80×40.
    #[test]
    fn offscreen_scale_half_child_size_matches_inline() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 255, 255).into()), // white
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(128)),
                    height: Dimension::Px(Fixed::from_int(114)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

        let _child = spawn(
            &mut world,
            Some(panel),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()), // red
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    position: crate::ui::layout::Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(5)),
                    top: Dimension::Px(Fixed::from_int(5)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mut buf = std::vec![0u8; 128 * 128 * 4];
        let tex =
            Texture::new(&mut buf, 128, 128, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let read_rgb = |x: usize, y: usize| -> (u8, u8, u8) {
            let i = (y * 128 + x) * 4;
            (buf[i], buf[i + 1], buf[i + 2])
        };
        assert_eq!(read_rgb(15, 15), (255, 0, 0), "child interior");
        assert_eq!(
            read_rgb(60, 30),
            (255, 255, 255),
            "(60, 30) should be panel white; if it's red the child \
             rendered at 2× and the inner viewport scale is broken"
        );
        assert_eq!(
            read_rgb(80, 40),
            (255, 255, 255),
            "(80, 40) should be panel white"
        );
    }

    /// Same fill invariant as the 32×32 RGBA8888 case but with a
    /// 128×114 panel + scale=0.5 + RGB565Swapped, exercising the
    /// 565 byte-swap path that the RGBA test never hits.
    #[test]
    fn offscreen_scale_half_form_sized_panel_fills_correctly() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(128)),
                    height: Dimension::Px(Fixed::from_int(114)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

        let mut buf = std::vec![0u8; 128 * 128 * 2];
        let tex = Texture::new(&mut buf, 128, 128, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        // RGB565Swapped: blue (0, 0, 255) packs to 565 = 0x001F. The
        // "swapped" variant byte-swaps to 0x1F00 and stores it
        // little-endian → bytes [0x00, 0x1F].
        let read_px = |x: usize, y: usize| -> [u8; 2] {
            let i = (y * 128 + x) * 2;
            [buf[i], buf[i + 1]]
        };
        for (x, y) in [(0, 0), (63, 56), (127, 0), (0, 113), (127, 113), (64, 60)] {
            let px = read_px(x, y);
            assert_eq!(
                px,
                [0x00, 0x1F],
                "panel pixel ({x}, {y}) is {px:02x?}; expected blue (RGB565Swapped little-endian)"
            );
        }
    }

    /// Pixel-equivalence: rendering a panel + child subtree through
    /// the OffscreenRender path must produce the same framebuffer
    /// pixels as rendering inline. Holds across cold cache (frame 1),
    /// warm cache hit (frame 2), and generation bump (frame 3 after
    /// child Dirty). Uses RGB565Swapped on a 128×128 buffer — the
    /// format pairing where integer-rounding drift between inline and
    /// offscreen raster paths is most likely to surface.
    #[test]
    fn offscreen_render_red_panel_with_child_matches_inline_across_frames() {
        use crate::ui::dirty::Dirty;
        use crate::ui::{Children, Parent, Theme};

        const FB_W: u16 = 128;
        const FB_H: u16 = 128;

        fn build_world(with_offscreen: bool) -> (World, Entity, Entity) {
            let mut world = World::default();
            // 64 KiB is plenty for a 40×20 panel + child buffer; the
            // pool default is 0 (disabled) so the test must opt in.
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
            world.insert_resource(ViewRegistry::with_builtins());
            world.insert_resource(Theme::dark());

            let panel = world.spawn_empty();
            world.insert(panel, Widget);
            world.insert(
                panel,
                Style {
                    bg_color: Some(Color::rgb(255, 0, 0).into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            if with_offscreen {
                world.insert(panel, OffscreenRender::default());
            }

            let child = world.spawn_empty();
            world.insert(child, Widget);
            world.insert(child, Parent(panel));
            world.insert(panel, Children(std::vec![child]));
            world.insert(
                child,
                Style {
                    bg_color: Some(Color::rgb(0, 0, 255).into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(8)),
                        height: Dimension::Px(Fixed::from_int(8)),
                        position: crate::ui::layout::Position::Absolute,
                        left: Dimension::Px(Fixed::from_int(4)),
                        top: Dimension::Px(Fixed::from_int(4)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            (world, panel, child)
        }

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        let render_into = |world: &World, panel: Entity| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 2];
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer).unwrap();
            fb
        };

        let (mut w_inline, p_inline, c_inline) = build_world(false);
        let (mut w_off, p_off, c_off) = build_world(true);

        let fb_inline_f1 = render_into(&w_inline, p_inline);
        let fb_off_f1 = render_into(&w_off, p_off);
        assert_eq!(fb_inline_f1, fb_off_f1, "frame 1 (cold) inline ≠ offscreen");

        // Inline re-rasters every frame; offscreen path serves the
        // cached buffer. The framebuffer must still come out the same.
        let fb_inline_f2 = render_into(&w_inline, p_inline);
        let fb_off_f2 = render_into(&w_off, p_off);
        assert_eq!(fb_inline_f2, fb_off_f2, "frame 2 (warm) inline ≠ offscreen");

        // Inline ignores Dirty (always full render); offscreen path
        // walks dirty + bumps generation. Both must converge.
        w_inline.insert(c_inline, Dirty);
        w_off.insert(c_off, Dirty);
        let fb_inline_f3 = render_into(&w_inline, p_inline);
        let fb_off_f3 = render_into(&w_off, p_off);
        assert_eq!(
            fb_inline_f3, fb_off_f3,
            "frame 3 (gen bump) inline ≠ offscreen"
        );
    }

    /// `WidgetTransform` on the OffscreenRender entity itself must
    /// reach the outer blit so the buffer ends up positioned /
    /// rotated / scaled the same way as the inline render's output.
    /// Three variants — pure translate, scale, rotate — each
    /// rendered both inline and offscreen, asserted byte-equal.
    #[test]
    fn offscreen_render_with_widget_transform_matches_inline() {
        use crate::ui::widgets::transform::WidgetTransform;
        use crate::ui::{Children, Parent, Theme};

        const FB_W: u16 = 128;
        const FB_H: u16 = 128;

        fn build_world(with_offscreen: bool, tf: Transform) -> (World, Entity) {
            let mut world = World::default();
            world.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
            world.insert_resource(ViewRegistry::with_builtins());
            world.insert_resource(Theme::dark());

            let panel = world.spawn_empty();
            world.insert(panel, Widget);
            world.insert(
                panel,
                Style {
                    bg_color: Some(Color::rgb(255, 0, 0).into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(panel, WidgetTransform(tf));
            if with_offscreen {
                world.insert(panel, OffscreenRender::default());
            }

            let child = world.spawn_empty();
            world.insert(child, Widget);
            world.insert(child, Parent(panel));
            world.insert(panel, Children(std::vec![child]));
            world.insert(
                child,
                Style {
                    bg_color: Some(Color::rgb(0, 0, 255).into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(8)),
                        height: Dimension::Px(Fixed::from_int(8)),
                        position: crate::ui::layout::Position::Absolute,
                        left: Dimension::Px(Fixed::from_int(4)),
                        top: Dimension::Px(Fixed::from_int(4)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            (world, panel)
        }

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        let render_into = |world: &World, panel: Entity| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer).unwrap();
            fb
        };

        // Translate / scale paths must be byte-identical: the inner
        // raster output then a transformed blit produces the same
        // pixels as raster-with-transform-applied directly. Rotate
        // is sampled twice — once when the inner SwRenderer rasters
        // into the buffer, once when the outer blit rotates the
        // buffer onto the framebuffer — so a few AA edge pixels
        // legitimately differ.
        let exact_cases = [
            (
                "translate(20, 30)",
                Transform::translate(Fixed::from_int(20), Fixed::from_int(30)),
            ),
            (
                "scale(2, 1.5)",
                Transform::scale(Fixed::from_int(2), Fixed::ONE + Fixed::ONE / 2),
            ),
        ];
        for (name, tf) in exact_cases {
            let (w_inline, p_inline) = build_world(false, tf);
            let (w_off, p_off) = build_world(true, tf);
            let fb_inline = render_into(&w_inline, p_inline);
            let fb_off = render_into(&w_off, p_off);
            let diff = fb_inline
                .iter()
                .zip(&fb_off)
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(
                diff, 0,
                "WidgetTransform({name}) on OffscreenRender ≠ inline ({diff} bytes diff)"
            );
        }

        let rotate_tf = Transform::rotate_deg(Fixed::from_int(15));
        let (w_inline, p_inline) = build_world(false, rotate_tf);
        let (w_off, p_off) = build_world(true, rotate_tf);
        let fb_inline = render_into(&w_inline, p_inline);
        let fb_off = render_into(&w_off, p_off);
        let diff = fb_inline
            .iter()
            .zip(&fb_off)
            .filter(|(a, b)| a != b)
            .count();
        // 40×20 panel rotated 15° has ~50 AA edge pixels × 4 bytes
        // each = ~200 bytes that legitimately resample differently.
        // 256 leaves headroom; if the fix regresses the diff jumps
        // into the thousands.
        assert!(
            diff < 256,
            "WidgetTransform(rotate(15deg)) on OffscreenRender drifts {diff} bytes from inline"
        );
    }

    /// Transparent capture and direct RGB565 drawing differ by at most one channel quantization step in this fixture.
    #[test]
    fn offscreen_render_form_page_subtree_matches_inline() {
        use crate::ui::layout::{AlignItems, FlexDirection, Padding};
        use crate::ui::theme::ColorToken;
        use crate::ui::widgets::{ProgressBar, Slider, Switch, Text};
        use crate::ui::{Children, Parent, Theme};

        const FB_W: u16 = 128;
        const FB_H: u16 = 128;

        fn add_child(world: &mut World, parent: Entity, child: Entity) {
            world.insert(child, Parent(parent));
            if let Some(c) = world.get_mut::<Children>(parent) {
                c.0.push(child);
            } else {
                world.insert(parent, Children(std::vec![child]));
            }
        }

        fn spawn_styled(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
            let e = world.spawn_empty();
            world.insert(e, Widget);
            world.insert(e, style);
            if let Some(p) = parent {
                add_child(world, p, e);
            }
            e
        }

        #[derive(Clone, Copy)]
        enum Mark {
            None,
            OnPanel,
            OnSwitch,
        }

        fn build(mark: Mark) -> (World, Entity, Entity, Entity) {
            let mut w = World::default();
            // 64 KiB covers the form_page + Switch buffer working
            // set; the pool default is 0 (disabled) so the test
            // must opt in.
            w.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
            w.insert_resource(ViewRegistry::with_builtins());
            w.insert_resource(Theme::dark());

            let fp = spawn_styled(
                &mut w,
                None,
                Style {
                    bg_color: Some(ColorToken::Surface.into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(128)),
                        height: Dimension::Px(Fixed::from_int(114)),
                        direction: FlexDirection::Column,
                        padding: Padding::all(Dimension::Px(Fixed::from_int(10))),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            if matches!(mark, Mark::OnPanel) {
                w.insert(fp, OffscreenRender::default());
            }
            let er = spawn_styled(
                &mut w,
                Some(fp),
                Style {
                    layout: LayoutStyle {
                        direction: FlexDirection::Row,
                        height: Dimension::Px(Fixed::from_int(28)),
                        align: AlignItems::Center,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let el = spawn_styled(
                &mut w,
                Some(er),
                Style {
                    text_color: ColorToken::OnSurface.into(),
                    layout: LayoutStyle {
                        grow: Fixed::ONE,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            w.insert(el, Text::from("Enable"));
            let sw = spawn_styled(
                &mut w,
                Some(er),
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            w.insert(sw, Switch::new());
            if matches!(mark, Mark::OnSwitch) {
                w.insert(sw, OffscreenRender::default());
            }
            let sr = spawn_styled(
                &mut w,
                Some(fp),
                Style {
                    layout: LayoutStyle {
                        height: Dimension::Px(Fixed::from_int(14)),
                        padding: Padding {
                            top: Dimension::Px(Fixed::from_int(6)),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let sl = spawn_styled(
                &mut w,
                Some(sr),
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(108)),
                        height: Dimension::Px(Fixed::from_int(14)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            w.insert(sl, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
            let pr = spawn_styled(
                &mut w,
                Some(fp),
                Style {
                    layout: LayoutStyle {
                        height: Dimension::Px(Fixed::from_int(10)),
                        padding: Padding {
                            top: Dimension::Px(Fixed::from_int(8)),
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let pb = spawn_styled(
                &mut w,
                Some(pr),
                Style {
                    border_radius: Fixed::from_int(4),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(108)),
                        height: Dimension::Px(Fixed::from_int(8)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            w.insert(pb, ProgressBar::new());
            (w, fp, sw, sl)
        }

        fn assert_frame(a: Vec<u8>, b: Vec<u8>, label: &str) {
            assert_eq!(a.len(), b.len());
            for (pixel, (a, b)) in a.chunks_exact(2).zip(b.chunks_exact(2)).enumerate() {
                let a = u16::from_be_bytes([a[0], a[1]]);
                let b = u16::from_be_bytes([b[0], b[1]]);
                for (shift, mask) in [(11, 31), (5, 63), (0, 31)] {
                    let error = ((a >> shift) & mask).abs_diff((b >> shift) & mask);
                    assert!(
                        error <= 1,
                        "{label}: pixel {pixel}, shift {shift}, error {error}"
                    );
                }
            }
        }

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        // Mirror render_dirty so the walker bumps OffscreenGeneration
        // when the subtree mutates between frames.
        let render_into = |world: &mut World, root: Entity| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 2];
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let dirty = super::collect_dirty_region(world, root, &viewport)
                .unwrap()
                .unwrap_or(Rect {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    w: Fixed::from_int(FB_W as i32),
                    h: Fixed::from_int(FB_H as i32),
                });
            super::render_region(world, root, &viewport, &dirty, &mut renderer).unwrap();
            fb
        };

        let (mut w_ref, fp_ref, sw_ref, sl_ref) = build(Mark::None);
        let (mut w_a, fp_a, sw_a, sl_a) = build(Mark::OnPanel);
        let (mut w_b, fp_b, sw_b, sl_b) = build(Mark::OnSwitch);

        w_ref.mark_subtree_dirty(fp_ref);
        w_a.mark_subtree_dirty(fp_a);
        w_b.mark_subtree_dirty(fp_b);

        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 1 ≠ inline",
        );
        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 1 ≠ inline",
        );

        // Mark the whole subtree so the inline path's dirty bound
        // equals the offscreen path's (which always promotes to the
        // OffscreenRender entity's full rect).
        let flip_on = |w: &mut World, fp: Entity, sw: Entity| {
            if let Some(s) = w.get_mut::<Switch>(sw) {
                s.on = true;
            }
            w.mark_subtree_dirty(fp);
        };
        flip_on(&mut w_ref, fp_ref, sw_ref);
        flip_on(&mut w_a, fp_a, sw_a);
        flip_on(&mut w_b, fp_b, sw_b);
        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 2 (switch on) ≠ inline",
        );
        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 2 (switch on) ≠ inline",
        );

        let slider_max = |w: &mut World, fp: Entity, sl: Entity| {
            if let Some(s) = w.get_mut::<Slider>(sl) {
                s.value = s.max;
            }
            w.mark_subtree_dirty(fp);
        };
        slider_max(&mut w_ref, fp_ref, sl_ref);
        slider_max(&mut w_a, fp_a, sl_a);
        slider_max(&mut w_b, fp_b, sl_b);
        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 3 (slider max) ≠ inline",
        );
        assert_frame(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 3 (slider max) ≠ inline",
        );
    }

    /// Same single-Switch reproducer but on RGB565Swapped instead of
    /// RGBA8888 — exercises the format-specific paths the RGBA test
    /// can't reach.
    #[test]
    fn offscreen_render_on_single_switch_widget_matches_inline_rgb565sw() {
        use crate::ui::widgets::Switch;

        let pix_at = |buf: &[u8], x: usize, y: usize| -> [u8; 2] {
            let i = (y * 128 + x) * 2;
            [buf[i], buf[i + 1]]
        };

        // (a) Inline Switch on RGB565Swapped 128×128 framebuffer.
        let mut buf_inline = std::vec![0u8; 128 * 128 * 2];
        {
            let mut world = make_world();
            let switch = spawn(
                &mut world,
                None,
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(switch, Switch::new());
            let tex = Texture::new(&mut buf_inline, 128, 128, ColorFormat::RGB565Swapped)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(128, 128, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer).unwrap();
        }

        let mut buf_off = std::vec![0u8; 128 * 128 * 2];
        {
            let mut world = make_world();
            let switch = spawn(
                &mut world,
                None,
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(switch, Switch::new());
            world.insert(switch, OffscreenRender::default());
            let tex = Texture::new(&mut buf_off, 128, 128, ColorFormat::RGB565Swapped)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(128, 128, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer).unwrap();
        }

        for &(x, y) in [(0, 0), (10, 5), (20, 10), (30, 15), (35, 18)].iter() {
            let i = pix_at(&buf_inline, x, y);
            let o = pix_at(&buf_off, x, y);
            assert_eq!(
                i, o,
                "pixel ({x}, {y}): inline {i:02x?} vs offscreen {o:02x?}"
            );
        }
    }

    /// Single Switch widget marked OffscreenRender::default(). Inline
    /// rendering paints the switch at the widget's logical rect;
    /// offscreen rendering must produce a pixel-identical result on
    /// the outer framebuffer.
    #[test]
    fn offscreen_render_on_single_switch_widget_matches_inline() {
        use crate::ui::widgets::Switch;

        let blue_at = |buf: &[u8], x: usize, y: usize| -> (u8, u8, u8) {
            let i = (y * 64 + x) * 4;
            (buf[i], buf[i + 1], buf[i + 2])
        };

        // (a) Inline Switch.
        let mut buf_inline = std::vec![0u8; 64 * 64 * 4];
        {
            let mut world = make_world();
            let switch = spawn(
                &mut world,
                None,
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(switch, Switch::new());
            let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer).unwrap();
        }

        // (b) Offscreen Switch.
        let mut buf_off = std::vec![0u8; 64 * 64 * 4];
        {
            let mut world = make_world();
            let switch = spawn(
                &mut world,
                None,
                Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(40)),
                        height: Dimension::Px(Fixed::from_int(20)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(switch, Switch::new());
            world.insert(switch, OffscreenRender::default());
            let tex = Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer).unwrap();
        }

        // Sample a handful of pixels across the 40×20 switch rect.
        // Both paths must produce the same colours within blending
        // rounding.
        for &(x, y) in [(0, 0), (10, 5), (20, 10), (30, 15), (35, 18)].iter() {
            let i = blue_at(&buf_inline, x, y);
            let o = blue_at(&buf_off, x, y);
            for ch in 0..3 {
                let inline_c = [i.0, i.1, i.2][ch];
                let off_c = [o.0, o.1, o.2][ch];
                let d = inline_c.abs_diff(off_c);
                assert!(
                    d <= 2,
                    "pixel ({x}, {y}) ch {ch}: inline {inline_c} vs offscreen {off_c} differ by {d}\n\
                     full pixel: inline {i:?}, offscreen {o:?}",
                );
            }
        }
    }

    /// Half-scale capture covers the full logical panel before composition.
    #[test]
    fn offscreen_scale_half_fills_buffer_and_blits_upscaled() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        // After render: outer 64×64 framebuffer's (0..32, 0..32) region
        // should be solid red. The buffer is internally 16×16 but the
        // blit upscales 2×.
        let read_px = |x: usize, y: usize| -> (u8, u8, u8, u8) {
            let i = (y * 64 + x) * 4;
            (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
        };
        for (x, y) in [(0, 0), (15, 15), (16, 0), (0, 16), (31, 31), (10, 20)] {
            let (r, g, b, _) = read_px(x, y);
            assert_eq!(
                (r, g, b),
                (255, 0, 0),
                "panel pixel ({x}, {y}) is ({r}, {g}, {b}); expected red"
            );
        }
        // Outside the panel's logical rect must still be untouched
        // (zeroed-out backing buffer).
        for (x, y) in [(32, 0), (0, 32), (50, 50), (63, 63)] {
            let (r, g, b, _) = read_px(x, y);
            assert_eq!(
                (r, g, b),
                (0, 0, 0),
                "outside-panel pixel ({x}, {y}) leaked colour ({r}, {g}, {b})"
            );
        }
    }

    // Threading test: validates off.opacity reaches the outer blit
    // cmd's opa byte. Half-red panel proves the plumb without a
    // per-backend integration harness.
    #[test]
    fn offscreen_opacity_dims_outer_blit() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::new().with_opacity(128));

        let mut buf = std::vec![0u8; 32 * 32 * 4];
        let tex =
            Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let read_r = |x: usize, y: usize| buf[(y * 32 + x) * 4];
        for (x, y) in [(0, 0), (7, 7), (15, 15), (3, 12)] {
            let r = read_r(x, y);
            assert!(
                (120..=135).contains(&r),
                "panel pixel ({x},{y}) red = {r}; expected ~128 for opacity=128",
            );
        }
        // Outside the panel must stay zero — opacity must NOT bleed
        // into untouched pixels.
        for (x, y) in [(16, 0), (0, 16), (20, 20), (31, 31)] {
            let r = read_r(x, y);
            assert_eq!(r, 0, "outside-panel ({x},{y}) leaked red={r}");
        }
    }

    #[test]
    fn offscreen_reuses_current_until_input_generation_changes() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);

        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let first_version = world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .state(panel)
            .unwrap()
            .current
            .unwrap()
            .version;

        let mut buf2 = std::vec![0u8; 64 * 64 * 4];
        let tex2 =
            Texture::new(&mut buf2, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer2 = SwRenderer::new(tex2);
        super::render(&world, panel, &viewport, &mut renderer2).unwrap();

        {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            assert_eq!(
                outputs.state(panel).unwrap().current.unwrap().version,
                first_version
            );
            assert_eq!(
                outputs
                    .get(panel)
                    .unwrap()
                    .texture()
                    .read_pixels()
                    .unwrap()
                    .get_pixel(1, 1),
                Color::rgb(0, 255, 0)
            );
        }
        world.get_mut::<Style>(panel).unwrap().bg_color = Some(Color::rgb(255, 0, 255).into());
        world.insert(panel, super::super::OffscreenGeneration(1));
        super::render(&world, panel, &viewport, &mut renderer2).unwrap();
        assert!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .state(panel)
                .unwrap()
                .current
                .unwrap()
                .version
                > first_version
        );
        assert_eq!((buf2[0], buf2[1], buf2[2]), (255, 0, 255));
    }

    /// Composition culling uses the transformed output bounds.
    #[test]
    fn offscreen_render_with_in_bounds_transform_still_renders() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        const FB_W: u16 = 64;
        const FB_H: u16 = 64;

        let mut app = crate::app::App::headless(FB_W, FB_H);
        app.with_default_widgets()
            .with_offscreen_pool_budget(64 * 1024);
        let mut world = app.world;

        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        {
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, panel, &viewport, &mut renderer).unwrap();
        }
        super::seed_prev_rects(&mut world, panel, &viewport);

        world.insert(
            panel,
            WidgetTransform(Transform::translate(
                Fixed::from_int(20),
                Fixed::from_int(10),
            )),
        );
        world.insert(panel, Dirty);

        let dirty = super::collect_dirty_region(&mut world, panel, &viewport)
            .unwrap()
            .expect("dirty region");
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        {
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, panel, &viewport, &dirty, &mut renderer).unwrap();
        }

        let i = (15 * FB_W as usize + 25) * 4;
        assert_eq!(
            (fb[i], fb[i + 1], fb[i + 2]),
            (0, 255, 0),
            "panel translated to (20,10) must show green at sample (25,15), got ({},{},{})",
            fb[i],
            fb[i + 1],
            fb[i + 2],
        );
    }

    /// A partially offscreen output still paints its visible transformed region.
    #[test]
    fn offscreen_render_with_transform_translating_partially_off_screen_still_renders() {
        use crate::types::Transform;
        use crate::ui::dirty::Dirty;
        use crate::ui::widgets::transform::WidgetTransform;

        const FB_W: u16 = 64;
        const FB_H: u16 = 64;

        let mut app = crate::app::App::headless(FB_W, FB_H);
        app.with_default_widgets()
            .with_offscreen_pool_budget(64 * 1024);
        let mut world = app.world;

        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        let render_full = |world: &World| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer).unwrap();
            fb
        };
        let render_dirty = |world: &mut World| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
            let dirty = super::collect_dirty_region(world, panel, &viewport)
                .unwrap()
                .unwrap_or(Rect {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    w: Fixed::from_int(FB_W as i32),
                    h: Fixed::from_int(FB_H as i32),
                });
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render_region(world, panel, &viewport, &dirty, &mut renderer).unwrap();
            fb
        };

        let _ = render_full(&world);
        super::seed_prev_rects(&mut world, panel, &viewport);

        world.insert(
            panel,
            WidgetTransform(Transform::translate(Fixed::from_int(-16), Fixed::ZERO)),
        );
        world.insert(panel, Dirty);
        let fb = render_dirty(&mut world);

        let i = (10 * FB_W as usize + 4) * 4;
        assert_eq!(
            (fb[i], fb[i + 1], fb[i + 2]),
            (0, 0, 255),
            "panel at translated position must show blue, got ({},{},{})",
            fb[i],
            fb[i + 1],
            fb[i + 2],
        );
    }

    /// Oversized panel + tiny pool budget rejects the buffer; caller
    /// must fall through to inline raster instead of leaving the entity
    /// unpainted. Compare byte-byte against an inline-only render.
    #[test]
    fn offscreen_render_falls_through_to_inline_when_buffer_oversized() {
        fn build(with_offscreen: bool, budget: usize) -> (World, Entity) {
            let mut app = crate::app::App::headless(64, 64);
            app.with_default_widgets()
                .with_offscreen_pool_budget(budget);
            let mut world = app.world;
            let panel = spawn(
                &mut world,
                None,
                Style {
                    bg_color: Some(Color::rgb(0, 200, 100).into()),
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(48)),
                        height: Dimension::Px(Fixed::from_int(48)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            if with_offscreen {
                world.insert(panel, OffscreenRender::default());
            }
            (world, panel)
        }

        let (w_inline, p_inline) = build(false, 64 * 1024);
        // 48×48×4 = 9216 bytes; 1 KiB budget can't host it.
        let (w_off, p_off) = build(true, 1024);

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let render = |world: &World, root: Entity| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; 64 * 64 * 4];
            let tex =
                Texture::new(&mut fb, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render(world, root, &viewport, &mut renderer).unwrap();
            fb
        };

        assert_eq!(
            render(&w_inline, p_inline),
            render(&w_off, p_off),
            "oversized rejection must fall through to inline raster"
        );
    }

    #[test]
    fn offscreen_render_reuses_buffer_across_frames_when_unchanged() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        super::render(&world, panel, &viewport, &mut renderer).unwrap();
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let version = outputs.state(panel).unwrap().current.unwrap().version;
        let pixels = outputs
            .get(panel)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .bytes()
            .as_ptr();
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
        assert_eq!(
            outputs.state(panel).unwrap().current.unwrap().version,
            version
        );
        assert_eq!(
            outputs
                .get(panel)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .bytes()
                .as_ptr(),
            pixels
        );
    }

    /// `App::snapshot_widget` returns an owned RGBA8888 texture without leaving a temporary output request behind.
    #[test]
    fn snapshot_widget_does_not_pollute_source_components() {
        use crate::app::App;
        use crate::types::{Color, Dimension};
        use crate::ui::{OffscreenRender, Widget};

        let mut app = App::headless(64, 64);
        app.with_default_widgets()
            .with_default_systems()
            .with_offscreen_pool_budget(64 * 1024);

        let panel = app.world.spawn_empty();
        app.world.insert(panel, Widget);
        app.world.insert(
            panel,
            Style {
                bg_color: Some(Color::rgb(0, 200, 100).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        app.set_root(panel);

        assert!(app.world.get::<OffscreenRender>(panel).is_none());

        let snap = app
            .snapshot_widget(panel)
            .unwrap()
            .expect("snapshot should produce an owned texture");
        assert_eq!(snap.width, 16);
        assert_eq!(snap.height, 16);

        assert!(app.world.get::<OffscreenRender>(panel).is_none());
        assert!(crate::ui::offscreen::config(&app.world, panel).is_none());
    }

    #[test]
    fn offscreen_sources_retain_transparent_outputs_across_backend_formats() {
        use super::super::offscreen::OffscreenGeneration;

        for (transparent_first, backend_format) in [
            (false, ColorFormat::RGB565),
            (true, ColorFormat::RGB565),
            (false, ColorFormat::RGB565Swapped),
            (true, ColorFormat::RGB565Swapped),
            (false, ColorFormat::RGBA8888),
            (true, ColorFormat::RGBA8888),
        ] {
            let mut world = make_world();
            let transparent = spawn(
                &mut world,
                None,
                Style {
                    bg_color: Some(Color::rgb(255, 0, 0).into()),
                    layout: LayoutStyle {
                        width: Dimension::px(8),
                        height: Dimension::px(8),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            let opaque = spawn(
                &mut world,
                None,
                Style {
                    bg_color: Some(Color::rgb(0, 255, 0).into()),
                    layout: LayoutStyle {
                        width: Dimension::px(8),
                        height: Dimension::px(8),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            );
            world.insert(transparent, OffscreenRender::default());
            world.insert(opaque, OffscreenRender::default());
            let order = if transparent_first {
                [transparent, opaque]
            } else {
                [opaque, transparent]
            };
            {
                let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
                outputs.reserve(2).unwrap();
                for entity in order {
                    outputs.set_retained(entity, true).unwrap();
                }
            }
            let viewport = Viewport::new(8, 8, Fixed::ONE);
            let mut target = [0_u8; 8 * 8 * 4];
            let texture =
                Texture::new(&mut target, 8, 8, backend_format).expect("valid pixel storage");
            let mut renderer = SwRenderer::new(texture);
            for generation in 0..2 {
                for entity in order {
                    world.insert(entity, OffscreenGeneration(generation));
                }
                let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
                let _batch = outputs.begin_batch();
                for entity in order {
                    super::render(&world, entity, &viewport, &mut renderer).unwrap();
                }
                for (entity, format) in [
                    (transparent, ColorFormat::RGBA8888),
                    (opaque, ColorFormat::RGBA8888),
                ] {
                    let current = world
                        .resource::<Outputs<WidgetTexture>>()
                        .unwrap()
                        .get(entity)
                        .expect("source-specific current buffer");
                    assert_eq!(current.texture().format, format);
                    assert_eq!(
                        current.texture().read_pixels().unwrap().get_pixel(1, 1),
                        if entity == transparent {
                            Color::rgb(255, 0, 0)
                        } else {
                            Color::rgb(0, 255, 0)
                        }
                    );
                    let previous = world
                        .resource::<Outputs<WidgetTexture>>()
                        .unwrap()
                        .previous(entity);
                    assert_eq!(previous.is_some(), generation != 0);
                    if let Some(previous) = previous {
                        assert_eq!(previous.texture().format, format);
                    }
                }
            }
            assert_eq!(
                crate::test_support::tracked_allocations(|| {
                    for entity in order {
                        assert!(
                            world
                                .resource::<Outputs<WidgetTexture>>()
                                .unwrap()
                                .get(entity)
                                .is_some()
                        );
                        assert!(
                            world
                                .resource::<Outputs<WidgetTexture>>()
                                .unwrap()
                                .previous(entity)
                                .is_some()
                        );
                    }
                }),
                0
            );
        }
    }

    /// Successful retained outputs use two slots while failed or skipped production does not advance history.
    #[test]
    fn retained_history_reads_successful_outputs_and_reuses_two_slots() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());

        {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            outputs.reserve(1).unwrap();
            outputs.set_retained(panel, true).unwrap();
        }

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        let render_frame = |world: &mut World, buf: &mut [u8]| {
            world.mark_subtree_dirty(panel);
            let dirty = super::collect_dirty_region(world, panel, &viewport)
                .unwrap()
                .unwrap_or(Rect {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    w: Fixed::from_int(64),
                    h: Fixed::from_int(64),
                });
            let tex =
                Texture::new(buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
            let mut renderer = SwRenderer::new(tex);
            super::render_region(world, panel, &viewport, &dirty, &mut renderer).unwrap();
        };

        let mut first_pixels = core::ptr::null();
        let mut prior_revision = 0;
        let colors = [
            Color::rgb(255, 0, 0),
            Color::rgb(0, 255, 0),
            Color::rgb(0, 0, 255),
        ];
        for (index, color) in colors.into_iter().enumerate() {
            world.get_mut::<Style>(panel).unwrap().bg_color = Some(color.into());
            render_frame(&mut world, &mut buf);
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            let current = outputs.get(panel).unwrap();
            assert_eq!(
                current.texture().read_pixels().unwrap().get_pixel(1, 1),
                color
            );
            assert!(current.texture().cache_revision > prior_revision);
            assert!(!current.texture().transient);
            prior_revision = current.texture().cache_revision;
            if index == 0 {
                first_pixels = current.texture().read_pixels().unwrap().bytes().as_ptr();
                assert!(outputs.previous(panel).is_none());
            } else {
                let previous = outputs.previous(panel).unwrap();
                assert_eq!(
                    previous.texture().read_pixels().unwrap().get_pixel(1, 1),
                    colors[index - 1]
                );
                if index == 1 {
                    assert_eq!(
                        previous.texture().read_pixels().unwrap().bytes().as_ptr(),
                        first_pixels
                    );
                    assert_ne!(
                        current.texture().read_pixels().unwrap().bytes().as_ptr(),
                        first_pixels
                    );
                } else {
                    assert_eq!(
                        current.texture().read_pixels().unwrap().bytes().as_ptr(),
                        first_pixels
                    );
                }
            }
        }
    }

    #[test]
    fn dirty_walker_promotes_subtree_dirty_to_offscreen_entity() {
        use super::super::dirty::Dirty;
        use super::super::offscreen::OffscreenGeneration;

        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        let child = spawn(
            &mut world,
            Some(panel),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        // Marking the child Dirty must (1) clear the child's marker,
        // (2) bump the offscreen entity's generation, (3) end up with
        // the offscreen entity's full rect in the dirty bounds — verify
        // that last bit indirectly via the buffer cache: a fresh render
        // after the bump should miss cache and re-insert.
        world.insert(child, Dirty);
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        // Run the dirty walker and grab the bounds.
        let bounds = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
        assert!(bounds.is_some(), "dirty walker should report some bounds");

        // Child's Dirty marker is cleared.
        assert!(
            world.get::<Dirty>(child).is_none(),
            "child Dirty should be cleared after offscreen subtree scan"
        );

        // Offscreen entity's generation has advanced.
        let g = world.get::<OffscreenGeneration>(panel).map(|gn| gn.0);
        assert_eq!(g, Some(1), "generation should bump from 0 to 1");
    }

    /// Self-Dirty case: the OffscreenRender entity itself goes Dirty
    /// (e.g. theme rotation calls `world.mark_subtree_dirty(root)` which
    /// stamps the entire tree). For an entity without children — a
    /// single-widget marker, like a Switch — the subtree scan finds
    /// no Dirty descendants and the old code skipped the
    /// generation bump entirely, so the cache kept handing out the
    /// stale buffer in the previous theme's colours.
    #[test]
    fn dirty_walker_bumps_generation_when_only_offscreen_entity_itself_is_dirty() {
        use super::super::dirty::Dirty;
        use super::super::offscreen::OffscreenGeneration;

        let mut world = make_world();
        let leaf = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 200, 100).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(20)),
                    height: Dimension::Px(Fixed::from_int(10)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(leaf, OffscreenRender::default());
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        // Frame 1 cold — first dirty walk (no PaintedCoverage yet).
        world.insert(leaf, Dirty);
        let _ = super::collect_dirty_region(&mut world, leaf, &viewport).unwrap();
        let g1 = world
            .get::<OffscreenGeneration>(leaf)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g1, 1, "frame 1 must bump from 0 → 1");

        // Frame 2 — only the offscreen entity itself is Dirty (no
        // children). The cache buffer is stale; generation must bump.
        world.insert(leaf, Dirty);
        let bounds = super::collect_dirty_region(&mut world, leaf, &viewport).unwrap();
        let g2 = world
            .get::<OffscreenGeneration>(leaf)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(
            g2, 2,
            "frame 2 self-Dirty must bump from 1 → 2 so the buffer cache misses"
        );
        assert!(bounds.is_some(), "frame 2 self-Dirty must report bounds");
    }

    /// Multi-frame regression: the OffscreenRender entity's bounds
    /// must end up in the dirty region every time a descendant goes
    /// Dirty, not just the first time. The previous code path made
    /// the rect-promotion conditional on `PaintedCoverage.is_none()` — true
    /// only on the very first dirty walk for a given entity — so the
    /// second time a child went Dirty the offscreen entity dropped
    /// out of the dirty bounds entirely, render_region had nothing
    /// to paint, and the offscreen subtree visibly froze on the
    /// previous frame's pixels.
    #[test]
    fn dirty_walker_promotes_subtree_dirty_on_every_frame() {
        use super::super::dirty::{Dirty, PaintedCoverage};
        use super::super::offscreen::OffscreenGeneration;

        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        let child = spawn(
            &mut world,
            Some(panel),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let viewport = Viewport::new(64, 64, Fixed::ONE);

        // Frame 1: collection promotes the panel; only paint confirmation records its bounds.
        world.insert(child, Dirty);
        let b1 = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
        assert!(b1.is_some(), "frame 1 must report bounds");
        let g1 = world
            .get::<OffscreenGeneration>(panel)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g1, 1, "frame 1 generation = 1");
        assert!(world.get::<PaintedCoverage>(panel).is_none());
        super::seed_prev_rects(&mut world, panel, &viewport);
        assert!(
            world.get::<PaintedCoverage>(panel).is_some(),
            "panel PaintedCoverage set"
        );

        // Frame 2: child goes Dirty again. Walker must bump generation
        // a second time AND include the panel rect in bounds. If it
        // doesn't, render_region won't repaint the offscreen entity
        // and the screen freezes.
        world.insert(child, Dirty);
        let b2 = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
        let g2 = world
            .get::<OffscreenGeneration>(panel)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g2, 2, "frame 2 generation = 2");
        assert!(
            b2.is_some(),
            "frame 2 must still report bounds covering the offscreen entity"
        );
        let b2 = b2.unwrap();
        // Panel logical rect is (0, 0, 32, 32) — bounds must contain it.
        assert!(
            b2.x.to_int() <= 0
                && b2.y.to_int() <= 0
                && (b2.x + b2.w).to_int() >= 32
                && (b2.y + b2.h).to_int() >= 32,
            "frame 2 bounds {b2:?} must cover the panel rect (0, 0, 32, 32)"
        );
    }

    #[test]
    fn dirty_walker_no_bump_when_subtree_clean() {
        use super::super::offscreen::OffscreenGeneration;

        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        spawn(
            &mut world,
            Some(panel),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let _ = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();

        let generation = world.get::<OffscreenGeneration>(panel).unwrap().0;
        assert!(
            super::collect_dirty_region(&mut world, panel, &viewport)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            world.get::<OffscreenGeneration>(panel).unwrap().0,
            generation
        );
        super::seed_prev_rects(&mut world, panel, &viewport);
        assert!(
            super::collect_dirty_region(&mut world, panel, &viewport)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn offscreen_render_accepts_projective_composition() {
        use crate::types::Transform3D;
        use crate::ui::widgets::transform_3d::WidgetTransform3D;
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        world.insert(panel, WidgetTransform3D(Transform3D::IDENTITY));

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
    }

    #[test]
    fn offscreen_render_composes_nested_outputs() {
        let mut world = make_world();
        let outer = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(outer, OffscreenRender::default());
        let inner = spawn(
            &mut world,
            Some(outer),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(inner, OffscreenRender::default());

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, outer, &viewport, &mut renderer).unwrap();
    }

    #[test]
    fn buffer_key_struct_layout() {
        // Sanity that BufferKey parts compose as expected for stable hashing.
        let k = BufferKey {
            entity: Entity {
                id: 1,
                generation: 0,
            },
            w: 32,
            h: 32,
            format: ColorFormat::RGBA8888,
            generation: 0,
            layout_rect: Rect::new(0, 0, 32, 32),
            local_bounds: Rect::new(0, 0, 32, 32),
            scale: Fixed::ONE,
        };
        let same = BufferKey {
            entity: Entity {
                id: 1,
                generation: 0,
            },
            w: 32,
            h: 32,
            format: ColorFormat::RGBA8888,
            generation: 0,
            layout_rect: Rect::new(0, 0, 32, 32),
            local_bounds: Rect::new(0, 0, 32, 32),
            scale: Fixed::ONE,
        };
        assert_eq!(k, same);
    }

    #[test]
    fn offscreen_render_fractional_scale_rounds_buffer_dims() {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        // Fixed::ONE * 7 / 10 = 0.7 in Q24.8.
        world.insert(panel, OffscreenRender::with_scale(Fixed::ONE * 7 / 10));

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(panel).unwrap();
        assert_eq!((output.texture().width, output.texture().height), (23, 23));
    }

    #[test]
    fn hidden_entity_skips_offscreen_path() {
        // An OffscreenRender entity that's also Hidden should not
        // create a buffer — collect_entities_preorder skips Hidden
        // entirely so it never requests a pixel slot.
        use crate::ui::Hidden;
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        world.insert(panel, Hidden);

        let mut buf = std::vec![0u8; 32 * 32 * 4];
        let tex =
            Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert!(outputs.get(panel).is_none());
        assert!(outputs.state(panel).is_none());
    }

    /// A mirror reads a later sibling's declared source during the first composition.
    #[test]
    fn mirror_of_paints_into_its_own_rect() {
        use crate::prelude::*;
        use crate::surface::FramebufferAccess;
        use crate::ui::widgets::MirrorOf;

        #[crate::compose]
        fn mirrored_panel() {
            ui! {
                View (width: 32, height: 32, bg_color: Color::rgb(0, 0, 0)) {
                    View (
                        id: "mirror",
                        position: Position::Absolute,
                        left: 0,
                        top: 16,
                        width: 16,
                        height: 16
                    ) [
                        MirrorOf::new(id("mirror_source")),
                    ]
                    View (
                        id: "mirror_source",
                        position: Position::Absolute,
                        left: 0,
                        top: 0,
                        width: 16,
                        height: 16,
                        bg_color: Color::rgb(255, 0, 0)
                    )
                }
            };
        }

        let mut app = App::headless(32, 32);
        app.with_default_widgets()
            .with_offscreen_pool_budget(64 * 1024);
        let root = app.spawn_root().id();
        app.compose(root, mirrored_panel());
        app.render().unwrap();

        let source = app.world.find_by_id("mirror_source").unwrap();
        assert!(crate::ui::offscreen::texture_requested(&app.world, source));
        assert!(app.world.get::<OffscreenRender>(source).is_none());
        assert_eq!(
            app.world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(source)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(1, 1),
            Color::rgb(255, 0, 0)
        );
        let mut backend = app.into_backend();
        let pixels = backend.framebuffer();
        assert_eq!(
            pixels.read_pixels().unwrap().get_pixel(1, 17),
            Color::rgb(255, 0, 0)
        );
    }

    #[test]
    fn offscreen_transparent_pixels_do_not_capture_parent_background() {
        let mut world = make_world();
        // Source is a 6×6 widget centered in a 12×12 buffer area
        // (it's only 6×6, so the buffer is sized 6×6 and the whole
        // buffer is "inside" the widget). To get a real "outside",
        // give the source a transparent area: just make sure the
        // alpha channel reflects what was written, not seeded.
        let panel = spawn(
            &mut world,
            None,
            Style {
                // No bg_color — source draws nothing into the buffer,
                // so every pixel should remain at the cleared default
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(6)),
                    height: Dimension::Px(Fixed::from_int(6)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, super::super::OffscreenRender::default());
        world.insert_resource(super::super::Outputs::<WidgetTexture>::with_budget(
            64 * 1024,
        ));

        let mut buf = std::vec![0xFFu8; 12 * 12 * 4];
        let tex =
            Texture::new(&mut buf, 12, 12, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(12, 12, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();

        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let output = outputs.get(panel).unwrap();
        for px in output
            .texture()
            .read_pixels()
            .unwrap()
            .bytes()
            .chunks_exact(4)
        {
            assert_eq!(px[3], 0, "alpha-clear buffer pixel must stay alpha=0");
        }
    }

    /// Theme swap while a subtree is `Hidden`, then unhide.
    /// `World::mark_subtree_dirty` skips Hidden, so the offscreen descendant
    /// inside the hidden subtree never receives Dirty. When the
    /// subtree is later unhidden, the dirty walker must still see
    /// enough Dirty markers in the freshly-revealed subtree to bump
    /// every OffscreenGeneration inside it; otherwise the cached
    /// buffer keeps painting in the previous theme's colours.
    #[test]
    fn unhide_after_global_event_invalidates_offscreen_descendants() {
        use crate::ui::Hidden;
        use crate::ui::dirty::Dirty;
        use crate::ui::offscreen::OffscreenGeneration;

        let mut world = make_world();
        let root = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let tab_content = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let offscreen_grandchild = spawn(
            &mut world,
            Some(tab_content),
            Style {
                bg_color: Some(Color::rgb(64, 128, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(offscreen_grandchild, OffscreenRender::default());

        // Frame 1 — initial render, generation gets to 1.
        world.insert(offscreen_grandchild, Dirty);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let _ = super::collect_dirty_region(&mut world, root, &viewport).unwrap();
        let g0 = world
            .get::<OffscreenGeneration>(offscreen_grandchild)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g0, 1, "first dirty walk must bump generation 0 → 1");

        // Hide the tab and clear any leftover Dirty in the subtree
        // (mirrors `tab_pages_system`'s hide branch).
        world.insert(tab_content, Hidden);
        world.clear_subtree_dirty(tab_content);

        // Global event: theme swap walks from `root`, hitting Hidden
        // along the way. The offscreen grandchild does not get Dirty.
        world.mark_subtree_dirty(root);
        assert!(
            world.get::<Dirty>(offscreen_grandchild).is_none(),
            "Hidden subtree's descendants must not be Dirty after \
             World::mark_subtree_dirty (this part is the existing optimisation)"
        );

        // Unhide: emulate the (false, true) branch of
        // `tab_pages_system::apply_visibility`. Marking the whole
        // subtree (rather than only `tab_content`) is what makes
        // descendants' caches invalidate.
        world.remove::<Hidden>(tab_content);
        world.mark_subtree_dirty(tab_content);

        // Run the walker and check the offscreen generation has
        // advanced — the cached buffer must be invalidated so the
        // next render misses cache and repaints in the new theme.
        let _ = super::collect_dirty_region(&mut world, root, &viewport).unwrap();
        let g1 = world
            .get::<OffscreenGeneration>(offscreen_grandchild)
            .map(|g| g.0)
            .unwrap_or(0);
        assert!(
            g1 > g0,
            "OffscreenGeneration must bump on unhide so the cache \
             misses; got {g1}, frame-1 was {g0}"
        );
    }
}

#[cfg(test)]
mod state_resolve_check {
    use super::*;

    #[test]
    fn enabled_when_no_state_components() {
        let mut world = World::new();
        let e = world.spawn_empty();
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Enabled);
    }

    #[test]
    fn disabled_user_state_self() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, UserState::Disabled);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
    }

    #[test]
    fn disabled_propagates_via_parent() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Disabled);
        assert_eq!(resolve_widget_state(&world, child), WidgetState::Disabled);
    }

    #[test]
    fn errored_self_only() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Errored);
        assert_eq!(resolve_widget_state(&world, child), WidgetState::Enabled);
        assert_eq!(resolve_widget_state(&world, parent), WidgetState::Error);
    }

    #[test]
    fn pressed_beats_hovered() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, InteractionState::Pressed);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Pressed);
    }

    #[test]
    fn hovered_when_only_hover() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, InteractionState::Hovered);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Hovered);
    }

    #[test]
    fn disabled_beats_pressed() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, UserState::Disabled);
        world.insert(e, InteractionState::Pressed);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
    }
}

#[cfg(all(test, feature = "std"))]
mod scroll_plan_check {
    extern crate std;
    use super::*;
    use crate::input::event::scroll::components::ScrollOffset;
    use crate::types::Dimension;
    use crate::ui::dirty::Dirty;
    use crate::ui::layout::LayoutStyle;
    use crate::ui::{Children, Parent, Style, Widget};

    fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            world.insert(e, Parent(p));
            if let Some(children) = world.get_mut::<Children>(p) {
                children.0.push(e);
            } else {
                world.insert(p, Children(std::vec![e]));
            }
        }
        e
    }

    fn px_style(w: i32, h: i32) -> Style {
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(w)),
                height: Dimension::Px(Fixed::from_int(h)),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn painted(world: &mut World, root: Entity, viewport: &Viewport) {
        collect_dirty_regions(world, root, viewport).unwrap();
        seed_prev_rects(world, root, viewport);
        world.clear_subtree_dirty(root);
    }

    fn move_from_painted(
        world: &mut World,
        root: Entity,
        viewport: &Viewport,
        list: Entity,
        old_y: i32,
    ) {
        let current = *world.get::<ScrollOffset>(list).unwrap();
        world.get_mut::<ScrollOffset>(list).unwrap().y = Fixed::from_int(old_y);
        painted(world, root, viewport);
        world.insert(list, current);
        world.invalidate_visual(list);
    }

    #[test]
    fn painted_offset_growth_emits_negative_scroll_op_and_bottom_strip() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(20),
            },
        );
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 17);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

        assert_eq!(plan.shifts.len(), 1, "should emit one RegionShift");
        let sop = &plan.shifts[0];
        assert_eq!(sop.dx, Fixed::ZERO);
        assert_eq!(
            sop.dy,
            Fixed::from_int(-3),
            "framebuffer shifts opposite to ScrollOffset growth"
        );
        assert_eq!(sop.area.h, Fixed::from_int(100));

        // Bottom strip = (0, 100-3=97, 128, 3)
        let has_strip = plan.rects.iter().any(|r| {
            r.h == Fixed::from_int(3) && r.y == Fixed::from_int(97) && r.w == Fixed::from_int(128)
        });
        assert!(
            has_strip,
            "expected bottom strip rect, got {:?}",
            plan.rects
        );
    }

    #[test]
    fn painted_offset_shrink_emits_positive_scroll_op_and_top_strip() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(20),
            },
        );
        world.insert(list, Dirty);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 23);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert_eq!(plan.shifts.len(), 1);
        assert_eq!(plan.shifts[0].dy, Fixed::from_int(3));
        let has_top_strip = plan.rects.iter().any(|r| {
            r.y == Fixed::ZERO && r.h == Fixed::from_int(3) && r.w == Fixed::from_int(128)
        });
        assert!(
            has_top_strip,
            "expected top strip rect, got {:?}",
            plan.rects
        );
    }

    #[test]
    fn displacement_larger_than_viewport_forces_full_repaint_without_shift() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(160),
            },
        );
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 40);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

        assert!(plan.shifts.is_empty());
        assert!(plan.rects.iter().any(|rect| *rect
            == Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(128),
                Fixed::from_int(100),
            )));
    }

    #[test]
    fn nested_scroll_emits_inner_first() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let outer = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            outer,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(outer, Dirty);
        let inner = spawn_widget(&mut world, Some(outer), absolute_style(0, 10, 128, 60));
        world.insert(
            inner,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(inner, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        painted(&mut world, root, &viewport);
        world.get_mut::<ScrollOffset>(outer).unwrap().y = Fixed::from_int(-2);
        world.get_mut::<ScrollOffset>(inner).unwrap().y = Fixed::from_int(-5);
        world.invalidate_visual(outer);
        world.invalidate_visual(inner);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert_eq!(plan.shifts.len(), 2, "two RegionShifts for two containers");
        assert_eq!(plan.shifts[0].dy, Fixed::from_int(5), "inner first");
        assert_eq!(plan.shifts[1].dy, Fixed::from_int(2), "outer second");
        assert_eq!(
            plan.shifts[0].area.y,
            Fixed::from_int(10),
            "source is painted position"
        );
        assert!(
            plan.rects
                .iter()
                .any(|rect| rect.y == Fixed::from_int(12) && rect.h == Fixed::from_int(5)),
            "exposure follows outer transfer"
        );
    }

    #[test]
    fn scroll_plans_use_only_the_last_painted_offset() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: 100.into(),
            },
        );
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let initial = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(
            initial.shifts.is_empty(),
            "unpainted pixels cannot be transferred"
        );
        seed_prev_rects(&mut world, root, &viewport);
        let mut plan = DirtyRegions::default();
        world.get_mut::<ScrollOffset>(list).unwrap().y = 120.into();
        world.invalidate_visual(list);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();

        let allocations = crate::test_support::tracked_allocations(|| {
            for offset in [120, 135, 110] {
                world.get_mut::<ScrollOffset>(list).unwrap().y = offset.into();
                for _ in 0..2 {
                    world.invalidate_visual(list);
                    try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
                    assert_eq!(plan.shifts.len(), 1);
                    assert_eq!(plan.shifts[0].dy, Fixed::from_int(100 - offset));
                    assert_eq!(
                        world.get::<scroll::PaintedScroll>(list).unwrap().offset.y,
                        Fixed::from_int(100)
                    );
                }
            }
            seed_prev_rects(&mut world, root, &viewport);
        });
        assert_eq!(allocations, 0);
        try_collect_dirty_regions_into(&mut world, root, &viewport, &mut plan).unwrap();
        assert!(
            plan.shifts.is_empty(),
            "successful drawing acknowledges the new offset"
        );
    }

    #[test]
    fn clipped_nested_window_is_repainted_without_discarding_outer_transfer() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        let outer = spawn_widget(&mut world, Some(root), px_style(128, 100));
        let inner = spawn_widget(&mut world, Some(outer), absolute_style(0, 0, 128, 60));
        world.insert(outer, ScrollOffset::default());
        world.insert(inner, ScrollOffset::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        painted(&mut world, root, &viewport);
        world.get_mut::<ScrollOffset>(outer).unwrap().y = 2.into();
        world.get_mut::<ScrollOffset>(inner).unwrap().y = 5.into();
        world.invalidate_visual(outer);
        world.invalidate_visual(inner);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert_eq!(plan.shifts.len(), 1);
        assert_eq!(plan.shifts[0].area, Rect::new(0, 0, 128, 100));
        assert!(plan.rects.contains(&Rect::new(0, 0, 128, 58)));
    }

    #[test]
    fn removed_scroll_offset_cannot_reuse_an_older_painted_incarnation() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(list, ScrollOffset::default());
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        painted(&mut world, root, &viewport);
        world.remove::<ScrollOffset>(list);
        world.invalidate_visual(list);
        painted(&mut world, root, &viewport);
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: 5.into(),
            },
        );
        world.invalidate_visual(list);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(plan.shifts.is_empty());
        assert!(plan.rects.contains(&Rect::new(0, 0, 128, 100)));
    }

    #[test]
    fn leaving_offscreen_or_rounded_ancestor_does_not_reuse_its_old_screen_baseline() {
        for offscreen in [false, true] {
            let mut world = World::new();
            let root = spawn_widget(&mut world, None, px_style(128, 128));
            let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
            world.insert(list, ScrollOffset::default());
            let viewport = Viewport::new(128, 128, Fixed::ONE);
            painted(&mut world, root, &viewport);
            if offscreen {
                world.insert(root, crate::ui::OffscreenRender::default());
            } else {
                let style = world.get_mut::<Style>(root).unwrap();
                style.clip_children = true;
                style.border_radius = 8.into();
            }
            world.invalidate_visual(root);
            painted(&mut world, root, &viewport);
            world.remove::<crate::ui::OffscreenRender>(root);
            world.get_mut::<Style>(root).unwrap().border_radius = Fixed::ZERO;
            world.get_mut::<ScrollOffset>(list).unwrap().y = 5.into();
            world.invalidate_visual(root);
            world.invalidate_visual(list);
            let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
            assert!(plan.shifts.is_empty());
        }
    }

    #[test]
    fn unchanged_scroll_offset_emits_no_op() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 0);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(plan.shifts.is_empty());
    }

    #[test]
    fn fractional_scroll_repaints_or_blits_whole_physical_pixels_without_residue() {
        for scale in [Fixed::ONE, Fixed::from_int(2)] {
            for delta in [
                Fixed::from_f32(-0.5),
                Fixed::from_f32(0.25),
                Fixed::from_f32(0.5),
            ] {
                let mut world = World::new();
                let root = spawn_widget(&mut world, None, px_style(128, 128));
                let list = spawn_widget(&mut world, Some(root), px_style(128, 100));
                world.insert(list, ScrollOffset::default());
                let viewport = Viewport::new(128, 128, scale);
                world.insert(root, Dirty);
                painted(&mut world, root, &viewport);
                for _ in 0..3 {
                    world.get_mut::<ScrollOffset>(list).unwrap().y += delta;
                    world.invalidate_visual(list);
                    let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
                    assert!(!plan.rects.is_empty());
                    if delta.abs() * scale == Fixed::ONE {
                        assert_eq!(plan.shifts.len(), 1);
                        assert_eq!(plan.shifts[0].dy, -delta);
                    } else {
                        assert!(plan.shifts.is_empty());
                        let height = Fixed::from_int(i32::from(viewport.logical_size().1).min(100));
                        assert!(plan.rects.iter().any(|rect| rect.h == height));
                    }
                    seed_prev_rects(&mut world, root, &viewport);
                }
            }
        }
    }

    fn absolute_style(left: i32, top: i32, w: i32, h: i32) -> Style {
        use crate::ui::layout::Position;
        Style {
            layout: LayoutStyle {
                position: Position::Absolute,
                left: Dimension::Px(Fixed::from_int(left)),
                top: Dimension::Px(Fixed::from_int(top)),
                width: Dimension::Px(Fixed::from_int(w)),
                height: Dimension::Px(Fixed::from_int(h)),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn overlay_over_scroll_covers_both_positions_without_overlapping_draws() {
        use crate::input::feedback::OverlayCursor;

        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), absolute_style(0, 0, 128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(2),
            },
        );
        world.insert(list, Dirty);

        let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 50, 40, 12));
        world.insert(overlay, OverlayCursor);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 0);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();

        assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
        let dy = plan.shifts[0].dy;
        assert_eq!(dy, Fixed::from_int(-2));

        assert_eq!(
            plan.rects,
            [Rect::new(40, 48, 40, 14), Rect::new(0, 98, 128, 2)]
        );
    }

    #[test]
    fn overlay_outside_scroll_container_keeps_scroll_op() {
        use crate::input::feedback::OverlayCursor;

        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), absolute_style(0, 30, 128, 90));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::from_int(2),
            },
        );
        world.insert(list, Dirty);

        let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 0, 40, 12));
        world.insert(overlay, OverlayCursor);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        move_from_painted(&mut world, root, &viewport, list, 0);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
    }

    #[test]
    fn dirty_walker_publishes_layout_snapshot_for_render_walker() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(64, 64));
        let child = spawn_widget(&mut world, Some(root), px_style(32, 32));
        world.insert(child, Dirty);

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(!plan.rects.is_empty(), "child Dirty should produce a rect");

        let snap = world
            .resource::<LayoutSnapshot>()
            .expect("dirty walker must publish a LayoutSnapshot");
        assert_eq!(
            snap.entities,
            std::vec![root, child],
            "snapshot must enumerate the same preorder as render walker would build",
        );
        assert_eq!(snap.layout_tree.children.len(), 1);
    }

    #[test]
    fn first_pass_publishes_input_geometry_and_preserves_initial_damage() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(64, 64));
        spawn_widget(&mut world, Some(root), px_style(32, 32));

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(!plan.rects.is_empty());
        assert!(world.resource::<LayoutSnapshot>().is_some());
        assert!(crate::input::event::hit_test::geometry_matches(
            &world, root, 64, 64,
        ));
    }

    #[test]
    fn render_region_cached_matches_fresh_output() {
        use crate::render::command::DrawCommand;
        use crate::render::renderer::Renderer;

        struct Recorder(std::vec::Vec<core::mem::Discriminant<DrawCommand<'static>>>);
        impl Renderer for Recorder {
            type TargetState = ();

            fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
                Ok(RenderRoute::Native)
            }

            fn submit(
                &mut self,
                draw: &::mirui::render::PreparedDraw<'_>,
            ) -> Result<(), RenderError> {
                let request = &draw.request();
                self.route(request)?;
                let static_cmd: &DrawCommand<'static> =
                    unsafe { core::mem::transmute(request.command) };
                self.0.push(core::mem::discriminant(static_cmd));
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let styled = |w, h| Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(w)),
                height: Dimension::Px(Fixed::from_int(h)),
                ..Default::default()
            },
            bg_color: Some(crate::ui::theme::ThemedColor::Raw(
                crate::types::Color::rgb(255, 0, 0),
            )),
            ..Default::default()
        };

        let mut world = World::new();
        world.insert_resource(crate::ui::view::ViewRegistry::with_builtins());
        world.insert_resource(crate::ui::theme::Theme::default());
        let root = spawn_widget(&mut world, None, styled(64, 64));
        let child = spawn_widget(&mut world, Some(root), styled(32, 32));
        world.insert(child, Dirty);

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let dirty_rect = Rect::new(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(64),
            Fixed::from_int(64),
        );
        let plan = collect_dirty_regions(&mut world, root, &viewport).unwrap();
        assert!(!plan.rects.is_empty());

        let snap = world
            .resource::<LayoutSnapshot>()
            .expect("snapshot present after dirty walk");
        let mut cached_out = Recorder(std::vec::Vec::new());
        let requests = prepare_paint_requests(&world).unwrap();
        render_region_cached(&world, snap, &dirty_rect, &mut cached_out, &requests, None).unwrap();
        requests.commit();

        let mut fresh_out = Recorder(std::vec::Vec::new());
        render_region(&world, root, &viewport, &dirty_rect, &mut fresh_out).unwrap();

        assert_eq!(
            cached_out.0, fresh_out.0,
            "cached and fresh paths must emit the same DrawCommand sequence",
        );
        assert!(
            !cached_out.0.is_empty(),
            "test must exercise at least one DrawCommand",
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod scroll_blit_visual_check {
    extern crate std;
    use super::*;
    use crate::render::renderer::Renderer;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};

    /// End-to-end check: scroll-blit + strip repaint matches a full
    /// repaint of the post-scroll state. Covers the `Rect` arithmetic
    /// the walker hands to the renderer; unit-level shifts are tested
    /// directly in `sw/mod.rs`.
    #[test]
    fn scroll_blit_then_strip_paint_matches_full_repaint() {
        let mut blit_buf = std::vec![0u8; 32 * 32 * 4];
        let mut full_buf = std::vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            let red = (y * 8) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                for buf in [&mut blit_buf, &mut full_buf] {
                    buf[off] = red;
                    buf[off + 1] = 0;
                    buf[off + 2] = 0;
                    buf[off + 3] = 255;
                }
            }
        }

        {
            let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut backend = SwRenderer::new(tex);
            let area = Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(32),
                Fixed::from_int(32),
            );
            backend
                .scroll_target_region(&area, Fixed::ZERO, Fixed::from_int(-4))
                .unwrap();
            for y in 28..32 {
                let post_shift_red = ((y + 4) * 8) as u8;
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    backend.target.pixels_mut().unwrap().bytes_mut()[off] = post_shift_red;
                }
            }
        }

        {
            for y in 0..32 {
                let post_shift_red = ((y + 4) * 8) as u8;
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    full_buf[off] = post_shift_red;
                }
            }
        }

        for y in 0..32 {
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                assert_eq!(
                    blit_buf[off], full_buf[off],
                    "mismatch at ({x},{y}): blit={} full={}",
                    blit_buf[off], full_buf[off]
                );
            }
        }
    }

    /// Nested scroll-blit: outer container shifts dy=-4, inner
    /// (sub-rect at rows 8..24, height 16) shifts dy=-2 in its own
    /// frame. End state must equal a full repaint where each row's
    /// red channel is shifted by the cumulative offset that applies
    /// at that y. Outer rows outside the inner sub-rect see only the
    /// outer shift; rows inside see outer-then-inner.
    #[test]
    fn nested_scroll_blit_matches_full_repaint() {
        let mut blit_buf = std::vec![0u8; 32 * 32 * 4];
        let mut full_buf = std::vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            let red = (y * 4) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                for buf in [&mut blit_buf, &mut full_buf] {
                    buf[off] = red;
                    buf[off + 1] = 0;
                    buf[off + 2] = 0;
                    buf[off + 3] = 255;
                }
            }
        }

        // Apply nested shifts: inner shift first (DFS post-order
        // — inner content moves in its own frame), then outer shift
        // pulls the already-shifted inner along with everything.
        {
            let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888)
                .expect("valid pixel storage");
            let mut backend = SwRenderer::new(tex);
            let inner_area = Rect::new(
                Fixed::ZERO,
                Fixed::from_int(8),
                Fixed::from_int(32),
                Fixed::from_int(16),
            );
            backend
                .scroll_target_region(&inner_area, Fixed::ZERO, Fixed::from_int(-2))
                .unwrap();
            // Inner strip exposed at rows 22..24 (bottom of inner).
            for y in 22..24 {
                let post_inner_red = ((y + 2) * 4) as u8;
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    backend.target.pixels_mut().unwrap().bytes_mut()[off] = post_inner_red;
                }
            }

            let outer_area = Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(32),
                Fixed::from_int(32),
            );
            backend
                .scroll_target_region(&outer_area, Fixed::ZERO, Fixed::from_int(-4))
                .unwrap();
            // Outer strip exposed at rows 28..32.
            for y in 28..32 {
                // After both shifts: row y on screen reads from
                // (y + 4) in original buffer, but if (y + 4) was in
                // the inner area (8..24) it had already been shifted
                // by 2 — so its source content was at (y + 4 + 2).
                let src_y = y + 4;
                let red = if (8..24).contains(&src_y) {
                    ((src_y + 2) * 4) as u8
                } else {
                    (src_y * 4) as u8
                };
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    backend.target.pixels_mut().unwrap().bytes_mut()[off] = red;
                }
            }
        }

        // Reference: a fresh full-repaint of the post-shift state.
        // Each row y reads from its post-shift source row.
        for y in 0..32 {
            // Outer shift -4: every row y on screen comes from y+4
            // in the pre-outer-shift buffer.
            let post_outer_y = y + 4;
            // Within the inner sub-rect (8..24), inner shift -2
            // composes: pre-inner buffer was at post_outer_y + 2.
            let src_y = if (8..24).contains(&post_outer_y) {
                post_outer_y + 2
            } else {
                post_outer_y
            };
            let red = (src_y * 4) as u8;
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                full_buf[off] = red;
            }
        }

        for y in 0..32 {
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                assert_eq!(
                    blit_buf[off], full_buf[off],
                    "row {y} mismatch: blit={} full={}",
                    blit_buf[off], full_buf[off]
                );
            }
        }
    }
}
