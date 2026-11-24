use crate::core::reactive::PaintRequestScope;
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::{DrawRequest, FallbackRegion, RenderError, RenderRoute, Renderer};
use crate::render::texture::Texture;
use crate::render::{BackendTarget, PlaneLayout, TargetPlan};
use crate::text::TextGeometry;
use crate::types::{Dimension, Fixed, Point, Rect, Transform, Transform3D, Viewport};
use crate::ui::layout::{LayoutNode, LayoutStyle, compute_layout};
use alloc::boxed::Box;
use alloc::vec::Vec;

use super::dirty::{DirtyRegions, PaintCoverage, RegionShift};
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

pub(crate) struct ScopedRenderer<'a> {
    physical_clip: Option<crate::types::PhysicalRect>,
    inner: &'a mut dyn Renderer,
    transform: Option<&'a Transform3D>,
}

impl<'a> ScopedRenderer<'a> {
    pub(crate) fn new(
        inner: &'a mut dyn Renderer,
        transform: &'a Transform3D,
        physical_clip: Option<crate::types::PhysicalRect>,
    ) -> Self {
        Self {
            inner,
            transform: (!transform.is_identity()).then_some(transform),
            physical_clip,
        }
    }

    fn apply_scope(&self, request: &mut DrawRequest<'_, '_>) {
        request.physical_clip = match (self.physical_clip, request.physical_clip) {
            (Some(left), Some(right)) => Some(
                left.intersection(right)
                    .unwrap_or(crate::types::PhysicalRect::EMPTY),
            ),
            (left, right) => left.or(right),
        };
        if let Some(transform) = self.transform {
            request.projective = transform.compose(&request.projective);
        }
    }
}

impl Renderer for ScopedRenderer<'_> {
    type TargetState = ();

    fn poll_draw_request(
        &mut self,
        mut request: DrawRequest<'_, '_>,
        text_run: Option<crate::render::TextRunIdentity>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<(), RenderError>> {
        self.apply_scope(&mut request);
        self.inner.poll_draw_request(request, text_run, cx)
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
        let mut request = *request;
        self.apply_scope(&mut request);
        self.inner.route(&request)
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
        mut request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<crate::render::PreparedDraw<'data>, RenderError>> {
        self.apply_scope(&mut request);
        self.inner.prepare(request, route, cx)
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
            let mut projected = ScopedRenderer {
                inner: local,
                transform,
                physical_clip,
            };
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
    if let Some(states) = world
        .storage::<UserState>()
        .filter(|states| !states.is_empty())
    {
        let mut cur = Some(entity);
        while let Some(e) = cur {
            if matches!(states.get(e), Some(UserState::Disabled)) {
                return WidgetState::Disabled;
            }
            cur = world.get::<Parent>(e).map(|p| p.0);
        }
        if matches!(states.get(entity), Some(UserState::Errored)) {
            return WidgetState::Error;
        }
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
    affine: &Transform,
    projective: &Transform3D,
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
    affine: &Transform,
    projective: &Transform3D,
    output_scale: Fixed,
    position: usize,
    plan: Option<&output_plan::TexturePlan>,
    preparation: Option<&PreparedPaint>,
) -> PaintCoverage {
    let coverage_preparation = preparation.filter(|_| plan.is_none());
    if let Some(coverage) = coverage_preparation.and_then(|prepared| prepared.get(position)) {
        return coverage;
    }
    let fallback = || {
        PaintCoverage::solid(visual_bounds(
            world,
            entity,
            rect,
            *affine,
            *projective,
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
            .and_then(|extra| transformed_clip_bounds(extra, *affine, *projective))
            .map_or_else(fallback, |extra| {
                fallback().union(PaintCoverage::solid(extra))
            });
    }
    let Some(registry) = world.resource::<ViewRegistry>() else {
        return fallback();
    };
    let mut coverage = PaintCoverage::EMPTY;
    let mut reusable = true;
    let matching =
        preparation.and_then(|prepared| prepared.matching_views(world, registry, entity, position));
    let mut index = 0;
    loop {
        if let Some(matching) = &matching {
            let Some(next) = matching.next_index(index) else {
                break;
            };
            index = next;
        }
        let Some(view) = registry.paint_at(index) else {
            break;
        };
        index += 1;
        if matching.is_none()
            && !view
                .component_filter()
                .is_none_or(|ty| world.has_type(entity, ty))
        {
            continue;
        }
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
    if reusable && let Some(preparation) = coverage_preparation {
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
    track_scroll: bool,
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
    let visible = ancestors_visible
        && preparation
            .and_then(|prepared| prepared.cached_visibility(*idx))
            .map_or_else(
                || !super::branch::is_effectively_hidden(world, entity),
                |visibility| visibility != prepared_paint::PaintVisibility::Hidden,
            );
    let shifted_rect = Rect {
        x: node.rect.x - scroll_offset.0,
        y: node.rect.y - scroll_offset.1,
        ..node.rect
    };
    let (tf, tf_3d) = prepared_paint::transforms(
        parent_transform,
        parent_3d,
        world,
        entity,
        shifted_rect,
        *idx,
        preparation,
    );
    let current_scroll = if track_scroll {
        scroll::PaintedScroll::capture(
            world,
            entity,
            shifted_rect,
            scroll_clip,
            tf,
            tf_3d,
            parent_scroll,
        )
    } else {
        None
    };
    if let Some(mut painted) = current_scroll {
        painted.reusable &= scroll_pixels;
        world.insert(entity, painted);
    } else if track_scroll {
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
                    &tf,
                    &tf_3d,
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
    let child_scroll_pixels =
        scroll_pixels && (!track_scroll || scroll::children_are_transferable(world, entity));
    for child in &node.children {
        seed_prev_rect_walk(
            child,
            world,
            entities,
            idx,
            &tf,
            &tf_3d,
            output_scale,
            track_scroll,
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
    crate::trace_span!("frame.seed_prev");
    {
        crate::trace_span!("paint.ack.scroll_baseline");
        scroll::PaintedScroll::invalidate_all(world);
    }
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
                crate::trace_span!("paint.ack.retired");
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
            {
                crate::trace_span!("paint.ack.walk");
                let track_scroll = scroll::needs_tracking(world);
                seed_prev_rect_walk(
                    &snapshot.layout_tree,
                    world,
                    &snapshot.entities,
                    &mut idx,
                    &Transform::IDENTITY,
                    &Transform3D::IDENTITY,
                    transform.scale(),
                    track_scroll,
                    (Fixed::ZERO, Fixed::ZERO),
                    Some(Rect::new(0, 0, logical_w, logical_h)),
                    None,
                    true,
                    !super::branch::is_hidden_in_tree(world, root),
                    output_plan.as_ref().filter(|plan| plan.matches_root(root)),
                    prepared.then_some(&snapshot.prepared_paint),
                );
            }
            output_plan::return_plan(world, output_plan);
            world.put_resource_box(snapshot);
            return;
        }
        world.put_resource_box(snapshot);
    }
    crate::trace_span!("paint.ack.layout_fallback");
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
    let track_scroll = scroll::needs_tracking(world);
    seed_prev_rect_walk(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
        transform.scale(),
        track_scroll,
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
        apply_input_intrinsic(world, entity, style, &mut node);
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
    reuse_inputs: TextInputReuse,
    solve: super::layout::solve::SolveScope,
}

#[derive(Clone, Copy, Default)]
enum TextInputReuse {
    #[default]
    Disabled,
    CleanNodes,
    UnchangedInputs,
}

impl TextInputReuse {
    fn enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
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
    if changes.reuse_inputs.enabled()
        && node.style == next_style
        && !world.has::<super::dirty::Dirty>(entity)
        && !world.has::<crate::ui::widgets::Text>(entity)
        && !world.has::<crate::ui::widgets::StaticGlyphRun>(entity)
    {
        return Ok(());
    }
    #[cfg(test)]
    local_layout_tests::record_input_visit(false);
    let observe_solve =
        !changes.structure && changes.solve != super::layout::solve::SolveScope::Full;
    let was_absolute = node.style.position == super::layout::Position::Absolute;
    let style_changed = observe_solve && node.style != next_style;
    let old_intrinsic = (node.intrinsic_width, node.intrinsic_height);
    node.style = next_style;
    node.intrinsic_width = None;
    node.intrinsic_height = None;
    let reused = changes.reuse_inputs.enabled()
        && reuse_text_inputs(world, entity, changes.reuse_inputs)
        && reuse_text_intrinsic(world, entity, style, node);
    if !reused {
        apply_text_intrinsic(world, entity, node)?;
    }
    apply_static_glyph_intrinsic(world, entity, style, node);
    apply_input_intrinsic(world, entity, style, node);
    if observe_solve
        && (style_changed || old_intrinsic != (node.intrinsic_width, node.intrinsic_height))
    {
        changes.solve.observe(node, was_absolute);
    }
    Ok(())
}

// The snapshot's maintenance revisions certify membership and child order.
// Unchanged values reuse measurements from the common text cache.
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
    reuse_inputs: TextInputReuse,
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
    if reuse_inputs.enabled()
        && reuse_text_inputs(world, entity, reuse_inputs)
        && let Some(handle) = world.get::<crate::text::TextLayoutHandle>(entity).copied()
        && let Some(measure) = resource.storage_mut().layouts.reuse_layout(
            handle,
            text_layout_owner(entity),
            crate::types::fixed::to_textflow(content_width.max(Fixed::ZERO)),
        )
    {
        return Ok(Some(LaidOutText { handle, measure }));
    }
    #[cfg(test)]
    local_layout_tests::record_text_preparation(false);
    let face_limit = resource.storage().layouts.limits().fallback_faces;
    let typography = super::Typography::inherited(world, entity, style);
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        typography.font_stack,
        typography.font_size,
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
            let typography = super::Typography::inherited(world, entity, style);
            let fonts = ResolvedFontStack::resolve(
                world.resource::<crate::render::font::FontManager>(),
                typography.font_stack,
                typography.font_size,
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
    reuse_inputs: TextInputReuse,
) -> Result<bool, TextLayoutFailure> {
    if *index >= entities.len() {
        return Ok(false);
    }
    let entity = entities[*index];
    *index += 1;
    let mut intrinsic_changed = layout_text_node(node, world, entity, reuse_inputs)?;
    for child in &mut node.children {
        intrinsic_changed |= layout_text_tree(child, world, entities, index, reuse_inputs)?;
    }
    Ok(intrinsic_changed)
}

fn layout_text_node(
    node: &mut LayoutNode,
    world: &mut World,
    entity: Entity,
    reuse_inputs: TextInputReuse,
) -> Result<bool, TextLayoutFailure> {
    #[cfg(test)]
    local_layout_tests::record_input_visit(true);
    use crate::types::fixed::from_textflow;

    let mut intrinsic_changed = false;
    if let Some(layout) = layout_text(world, entity, node.rect.w, reuse_inputs)? {
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
    Ok(intrinsic_changed)
}

// Incremental callers have delivered input invalidations before entering here.
// Explicit layout preparation continues to inspect all current inputs.
fn prepare_local_layout(
    world: &mut World,
    snapshot: &mut LayoutSnapshot,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
) -> Result<bool, TextLayoutFailure> {
    use super::layout::{Position, solve::SolveScope};

    if !snapshot.matches(root, logical_w, logical_h)
        || snapshot.bindings_pending
        || !snapshot.maintenance_current(world)
        || !snapshot.scroll_config_current(world)
    {
        return Ok(false);
    }
    // Parked branches retain their pending work. Only the active snapshot
    // participates in this solve; unrelated hidden dirt must not move its
    // parent into a full-tree preparation.
    let mut dirty = snapshot
        .entities
        .iter()
        .copied()
        .filter(|entity| world.has::<super::dirty::Dirty>(*entity));
    let Some(target) = dirty.next() else {
        return Ok(false);
    };
    if dirty.next().is_some() {
        return Ok(false);
    }
    if target == root {
        return Ok(false);
    }
    let Some(node) = snapshot.paths.node_mut(&mut snapshot.layout_tree, target) else {
        return Ok(false);
    };
    if !node.children.is_empty() || node.style.position != Position::Absolute {
        return Ok(false);
    }
    let Some(style) = world.get::<Style>(target) else {
        return Ok(false);
    };
    let mut candidate = LayoutNode::new(node.style);
    candidate.entity = Some(target);
    candidate.rect = node.rect;
    candidate.intrinsic_width = node.intrinsic_width;
    candidate.intrinsic_height = node.intrinsic_height;
    let mut changes = LayoutTreeChanges::default();
    refresh_layout_values(world, target, &mut candidate, style, &mut changes)?;
    if changes.solve == SolveScope::Full {
        return Ok(false);
    }
    node.style = candidate.style;
    node.intrinsic_width = candidate.intrinsic_width;
    node.intrinsic_height = candidate.intrinsic_height;
    if changes.solve != SolveScope::Unchanged && !changes.solve.apply(&mut snapshot.layout_tree) {
        snapshot.invalidate();
        return Ok(false);
    }
    #[cfg(test)]
    if changes.solve != SolveScope::Unchanged {
        local_layout_tests::record_solve(true);
    }
    let node = snapshot
        .paths
        .node_mut(&mut snapshot.layout_tree, target)
        .expect("prepared leaf");
    if layout_text_node(node, world, target, TextInputReuse::Disabled)? {
        snapshot.invalidate();
        return Ok(false);
    }
    snapshot.prepared_paint.resize(
        snapshot.entities.len(),
        world
            .resource::<ViewRegistry>()
            .map_or(0, |registry| registry.paint_iter().len()),
    );
    #[cfg(test)]
    local_layout_tests::record_preparation();
    Ok(true)
}

fn compute_layout_snapshot(
    world: &mut World,
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    incremental: bool,
) -> Result<Option<Box<LayoutSnapshot>>, TextLayoutFailure> {
    world.watch_component_revision::<crate::input::event::scroll::ScrollConfig>();
    LayoutSnapshot::watch_maintenance(world);
    output_plan::invalidate_geometry(world);
    let mut previous = world.take_resource_box::<LayoutSnapshot>();
    let font_inputs = FontInputs::capture(world);
    let text_inputs = TextInputRevisions::capture(world);
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
    if incremental
        && let Some(snapshot) = &mut previous
        && snapshot.font_inputs.matches(&font_inputs)
    {
        match prepare_local_layout(world, snapshot, root, logical_w, logical_h) {
            Ok(true) => return Ok(previous),
            Ok(false) => {}
            Err(error) => {
                snapshot.invalidate();
                if let Some(snapshot) = previous {
                    world.put_resource_box(snapshot);
                }
                return Err(error);
            }
        }
    }
    let local_solve = previous.as_ref().is_some_and(|snapshot| {
        snapshot.matches(root, logical_w, logical_h) && !snapshot.bindings_pending
    });
    let reuse_inputs = if incremental
        && local_solve
        && previous.as_ref().is_some_and(|snapshot| {
            snapshot.maintenance_current(world) && snapshot.font_inputs.matches(&font_inputs)
        }) {
        if text_inputs.is_some()
            && previous.as_ref().and_then(|snapshot| snapshot.text_inputs) == text_inputs
        {
            TextInputReuse::UnchangedInputs
        } else {
            TextInputReuse::CleanNodes
        }
    } else {
        TextInputReuse::Disabled
    };
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
        reuse_inputs,
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
                    font_inputs: FontInputs::default(),
                    text_inputs: None,
                    layout_tree,
                    parked,
                    reachable: Vec::new(),
                    entities,
                    painted_damage: Vec::with_capacity(node_count),
                    prepared_paint: PreparedPaint::with_capacity(node_count),
                    paths: super::layout::paths::LayoutPaths::default(),
                    layout_participants: Default::default(),
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
    snapshot
        .prepared_paint
        .prepare_subtrees(&snapshot.layout_tree, changes.structure);
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
            reuse_inputs,
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
    snapshot.font_inputs = font_inputs;
    snapshot.text_inputs = text_inputs;
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
    #[cfg(test)]
    local_layout_tests::record_text_preparation(true);
    let style = world.get::<Style>(entity).expect("text widget style");
    let Some(cache) = world.resource::<Outputs<TextGeometry>>() else {
        return Ok(());
    };
    let face_limit = cache.storage().layouts.limits().fallback_faces;
    let typography = super::Typography::inherited(world, entity, style);
    let Some(fonts) = ResolvedFontStack::resolve(
        world.resource::<crate::render::font::FontManager>(),
        typography.font_stack,
        typography.font_size,
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

fn reuse_text_inputs(world: &World, entity: Entity, reuse: TextInputReuse) -> bool {
    (matches!(reuse, TextInputReuse::UnchangedInputs) || !world.has::<super::dirty::Dirty>(entity))
        && !world.has::<crate::text::TextPath>(entity)
        && world
            .get::<crate::ui::widgets::Text>(entity)
            .is_some_and(|text| !text.is_localized())
}

fn reuse_text_intrinsic(
    world: &World,
    entity: Entity,
    style: &Style,
    node: &mut LayoutNode,
) -> bool {
    let Some(cache) = world.resource::<Outputs<TextGeometry>>() else {
        return false;
    };
    let Some(measure) = cache
        .storage_mut()
        .layouts
        .reuse_measurement(text_layout_owner(entity))
    else {
        return false;
    };
    let (width, height) = crate::ui::widgets::text::padded_text_intrinsic_size(
        style,
        crate::types::fixed::from_textflow(measure.width),
        crate::types::fixed::from_textflow(measure.height),
    );
    node.set_intrinsic_size(width, height);
    true
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

/// A single line input with no height of its own is as tall as its line and
/// the padding above and below it, three pixels more than a field's inner line
/// when that padding is left unwritten, so the line sits with room around it.
fn apply_input_intrinsic(world: &World, entity: Entity, style: &Style, node: &mut LayoutNode) {
    use super::widgets::text_editing::geometry::AUTO_HEIGHT_PADDING;

    if !world.has::<super::widgets::TextInput>(entity) {
        return;
    }
    let typography = super::Typography::inherited(world, entity, style);
    let Some(font) =
        crate::render::font::resolve_or_default(world, typography.font_stack.primary())
    else {
        return;
    };
    let ppem = font.ppem_for(typography.font_size);
    let rows = world
        .get::<super::widgets::TextInput>(entity)
        .map_or(1, |input| i32::from(input.lines.max(1)));
    let line = font.metrics(ppem).line_height * rows;
    let pad = |side: Dimension| match side {
        Dimension::Px(value) => value.max(Fixed::ZERO),
        _ => Fixed::from_int(AUTO_HEIGHT_PADDING),
    };
    node.set_intrinsic_height(
        line + pad(style.layout.padding.top) + pad(style.layout.padding.bottom),
    );
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
    let typography = super::Typography::inherited(world, entity, style);
    let Some(font) =
        crate::render::font::resolve_or_default(world, typography.font_stack.primary())
    else {
        return;
    };
    let ppem = font.ppem_for(typography.font_size);
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

fn skip_subtree(
    node: &LayoutNode,
    index: &mut usize,
    preparation: Option<&PreparedPaint>,
    plan: Option<&output_plan::TexturePlan>,
) {
    if node.children.is_empty() {
        *index += 1;
        return;
    }
    *index = preparation
        .and_then(|prepared| prepared.subtree_end(*index))
        .or_else(|| plan.and_then(|plan| plan.subtree_end(*index)))
        .unwrap_or_else(|| *index + count_nodes(node));
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
        skip_subtree(node, idx, preparation, output_plan);
        return Ok(());
    }
    let parent_typography = entities.get(*idx).and_then(|entity| {
        let parent = world.get::<super::Parent>(*entity)?.0;
        let style = world.get::<Style>(parent)?;
        Some(super::Typography::inherited(world, parent, style))
    });
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
        parent_typography.as_ref(),
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
    parent_typography: Option<&super::Typography<'_>>,
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
        skip_subtree(node, idx, preparation, output_plan);
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
            parent_typography,
        );
    }
    let rect = Rect {
        x: node.rect.x - offset_x,
        y: node.rect.y - offset_y,
        ..node.rect
    };
    let (affine, projective) = prepared_paint::transforms(
        parent_transform,
        parent_transform_3d,
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
                parent_typography,
            )
        },
    );
    if *idx == before {
        skip_subtree(node, idx, preparation, output_plan);
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
    parent_typography: Option<&super::Typography<'_>>,
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
        parent_transform,
        parent_transform_3d,
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
        &tf,
        &tf_3d,
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
        skip_subtree(node, idx, preparation, output_plan);
        return Ok(());
    }

    if output_plan.is_some_and(|plan| plan.blocked(entity)) {
        skip_subtree(node, idx, preparation, output_plan);
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
        skip_subtree(node, idx, preparation, output_plan);
        return Ok(());
    }
    if output_plan.is_some_and(|plan| plan.ready(entity))
        && let Some(off) = offscreen
    {
        let handled = if !tf_3d.is_identity() {
            let mut scoped = ScopedRenderer::new(renderer, &tf_3d, None);
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
                preparation,
                output_plan,
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
                preparation,
                output_plan,
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
                let mut scoped = ScopedRenderer::new(renderer, &tf_3d, None);
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
                    ctx.set_parent_typography(parent_typography);
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
                ctx.set_parent_typography(parent_typography);
                render_views(world, entity, &mut ctx, requests, preparation, *idx)?;
            }
        }
    }
    *idx += 1;

    if node.children.is_empty() {
        return Ok(());
    }
    let own_typography = style
        .filter(|style| super::Typography::declared_style(style))
        .map(|style| super::Typography::with_parent(style, parent_typography));
    let child_typography = own_typography.as_ref().or(parent_typography);

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
            child_typography,
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
    let matching =
        preparation.and_then(|prepared| prepared.matching_views(world, registry, entity, position));
    let mut index = 0;
    loop {
        if let Some(matching) = &matching {
            let Some(next) = matching.next_index(index) else {
                break;
            };
            index = next;
        }
        let Some(view) = registry.paint_at(index) else {
            break;
        };
        index += 1;
        if matching.is_none()
            && !view
                .component_filter()
                .is_none_or(|ty| world.has_type(entity, ty))
        {
            continue;
        }
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
        #[cfg(feature = "perf")]
        outputs.trace_texture_reuse(key);
        skip_subtree(node, idx, None, output_plan);
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
        crate::trace_span!("draw.offscreen.native");
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
        crate::trace_span!("draw.offscreen.software");
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
    preparation: Option<&PreparedPaint>,
    output_plan: Option<&output_plan::TexturePlan>,
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
    skip_subtree(node, idx, preparation, output_plan);
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
        if let Some(registry) = world.resource::<ViewRegistry>() {
            snapshot.prepared_paint.clear_for_frame(world, registry);
        } else {
            snapshot.prepared_paint.clear();
        }
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
        + super::view::ViewRegistry::prepared_layout_count(world);
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
    let mut prepared_revision = None;

    for pass in 0..pass_count {
        let layout_unchanged = if pass == 0 {
            reuse_visual_layout && !crate::input::event::hit_test::geometry_changes(world).0
        } else {
            prepared_revision.is_some()
                && prepared_revision
                    == world.tracked_component_revision::<crate::ui::dirty::Dirty>()
        };
        let reuse = layout_unchanged
            && !super::branch::is_authored_hidden(world, root)
            && world.resource::<LayoutSnapshot>().is_some_and(|snapshot| {
                snapshot.matches(root, logical_w, logical_h)
                    && !snapshot.bindings_pending
                    && snapshot.scroll_config_current(world)
            });
        let snapshot = if reuse {
            world.take_resource_box::<LayoutSnapshot>()
        } else {
            compute_layout_snapshot(
                world,
                root,
                logical_w,
                logical_h,
                reuse_visual_layout && pass == 0,
            )?
        };
        let Some(mut snapshot) = snapshot else {
            return Ok(None);
        };

        let mut idx = 0;
        write_computed_rects(&snapshot.layout_tree, world, &snapshot.entities, &mut idx);
        snapshot.prepare_geometry_plan(world);
        prepared_revision = world.tracked_component_revision::<crate::ui::dirty::Dirty>();
        let participated = snapshot.layout_participants.reconcile(
            world,
            &mut super::prepared_layout::PreparedLayout {
                tree: &mut snapshot.layout_tree,
                entities: &snapshot.entities,
                paths: &snapshot.paths,
                viewport: Rect::new(0, 0, logical_w, logical_h),
            },
        );
        if participated == super::property::PropertyChange::Layout {
            prepared_revision = None;
        }
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
        let mut changed =
            participated.changed() | super::layout_binding::apply_layout_bindings(world, invoke);
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
        super::flush_bindings(world);
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
        &|index| snapshot.prepared_paint.subtree_end(index),
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
        super::flush_bindings(world);
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
        || (visual_changed && super::view::ViewRegistry::prepared_layout_count(world) != 0)
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
                &|index| snapshot.prepared_paint.subtree_end(index),
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
        let mut renderer = ScopedRenderer::new(renderer, &Transform3D::IDENTITY, physical_clip);
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
        self.regions.insert(rect);
    }
}

/// Resource exposing the last frame's plan. Read-only for probes /
/// debug overlays; production code should not depend on it.
#[derive(Clone, Debug, Default)]
pub struct LastDirtyRegions(pub DirtyRegions);

#[derive(Default)]
struct FontInputs {
    revision: Option<alloc::rc::Rc<core::cell::Cell<u64>>>,
    observed: u64,
    providers: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct TextInputRevisions {
    content: u64,
    style_metrics: u64,
}

impl TextInputRevisions {
    fn capture(world: &mut World) -> Option<Self> {
        use crate::ecs::world::ComponentChangeKind;
        use crate::ui::widgets::Text;

        let style_metrics = match world.filtered_component_revision::<Style>() {
            Some(revision) => revision,
            None => {
                if !world.has_any_by_id(core::any::TypeId::of::<Text>()) {
                    return None;
                }
                world.watch_component_revision::<Text>();
                world.watch_filtered_component_revision::<Style>(|kind| match kind {
                    ComponentChangeKind::Component => true,
                    ComponentChangeKind::InheritedStyle(fields) => {
                        super::style::metrics_changed(fields)
                    }
                    _ => false,
                });
                world.filtered_component_revision::<Style>()?
            }
        };
        Some(Self {
            content: world.tracked_component_revision::<Text>()?,
            style_metrics,
        })
    }
}

impl FontInputs {
    fn capture(world: &World) -> Self {
        use core::hash::{Hash, Hasher};
        let fonts = world.resource::<crate::render::font::FontManager>();
        let revision = fonts.map(|fonts| fonts.revision_cell());
        let observed = revision.as_ref().map_or(0, |revision| revision.get());
        // Providers may advance independently of their manager. Inspect the
        // resident faces once per preparation, without resolving tokens or
        // changing cache recency. Order-independent accumulation survives LRU
        // reordering; manager revisions cover token-to-value replacement.
        let mut providers = 0_u64;
        if let Some(fonts) = fonts {
            fonts.visit_resident(|font| {
                let mut hash = rustc_hash::FxHasher::default();
                (
                    font.face_id().value(),
                    font.revision(),
                    font.size,
                    alloc::rc::Rc::as_ptr(&font.provider) as *const () as usize,
                )
                    .hash(&mut hash);
                providers = providers.wrapping_add(hash.finish());
            });
        }
        Self {
            revision,
            observed,
            providers,
        }
    }

    fn matches(&self, other: &Self) -> bool {
        self.observed == other.observed
            && self.providers == other.providers
            && match (&self.revision, &other.revision) {
                (None, None) => true,
                (Some(a), Some(b)) => alloc::rc::Rc::ptr_eq(a, b),
                _ => false,
            }
    }
}

pub(crate) struct LayoutSnapshot {
    root: Entity,
    logical_w: u16,
    logical_h: u16,
    valid: bool,
    bindings_pending: bool,
    scroll_config_revision: u64,
    maintenance_revisions: [u64; 6],
    font_inputs: FontInputs,
    text_inputs: Option<TextInputRevisions>,
    pub(crate) layout_tree: LayoutNode,
    parked: Vec<LayoutNode>,
    reachable: Vec<Entity>,
    pub(crate) entities: Vec<Entity>,
    pub(crate) paths: super::layout::paths::LayoutPaths,
    pub(crate) layout_participants: super::prepared_layout::LayoutParticipants,
    painted_damage: Vec<Entity>,
    prepared_paint: PreparedPaint,
}

impl LayoutSnapshot {
    pub(crate) fn trim_memory(&mut self) {
        self.reachable = Vec::new();
        self.paths.release();
        self.layout_participants.release();
    }

    fn invalidate_geometry_plan(&mut self) {
        self.paths.invalidate();
        self.layout_participants.invalidate();
    }

    pub(crate) fn prepare_geometry_plan(&mut self, world: &mut World) {
        self.layout_participants.prepare(world, &self.entities);
        let outputs = world.resource::<Outputs<super::LayoutGeometry>>();
        if !outputs.is_some_and(Outputs::has_requests) && !self.layout_participants.has_geometry() {
            self.paths.release();
            return;
        }
        let relation_revision = world
            .resource::<super::outputs::relations::OutputRelations>()
            .map_or(
                0,
                super::outputs::relations::OutputRelations::structure_revision,
            );
        self.paths.prepare(
            &self.layout_tree,
            self.entities.len(),
            [relation_revision, self.layout_participants.revision(), 0],
            |entity| {
                outputs.is_some_and(|outputs| outputs.is_requested(entity))
                    || self.layout_participants.needs_geometry(entity)
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
    track_scroll: bool,
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
    let visible =
        ancestors_visible && !prepared_paint::hidden(world, entity, *idx, Some(preparation));
    let shifted_rect = Rect {
        x: node.rect.x - scroll_offset.0,
        y: node.rect.y - scroll_offset.1,
        ..node.rect
    };
    let (tf, tf_3d) = prepared_paint::transforms(
        parent_transform,
        parent_3d,
        world,
        entity,
        shifted_rect,
        *idx,
        Some(preparation),
    );

    let pending = VisualDirty::prepare(world, entity);
    let moved = ancestor_moved || pending.is_some_and(VisualDirty::transforms_subtree);
    // Completed earlier subtrees can transfer pixels underneath a later sibling.
    // Restore both its fixed paint and any pixels displaced by those transfers.
    let overpaint_shifted = visible
        && !plan.shifts.is_empty()
        && world
            .get::<super::dirty::PaintedCoverage>(entity)
            .is_some_and(|painted| {
                painted.0.regions().any(|rect| {
                    plan.shifts
                        .iter()
                        .any(|shift| rect.intersect(&shift.area).is_some())
                })
            });
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
        if pending.is_some() || moved || overpaint_shifted {
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

    let current_scroll = if track_scroll {
        scroll::PaintedScroll::capture(
            world,
            entity,
            shifted_rect,
            scroll_clip,
            tf,
            tf_3d,
            parent_scroll,
        )
    } else {
        None
    };
    let painted_scroll = if track_scroll {
        world.get::<scroll::PaintedScroll>(entity).copied()
    } else {
        None
    };
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

    if overpaint_shifted || ((pending.is_some() || moved) && my_scroll_op.is_none()) {
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

        let child_carrier = child_carrier
            .filter(|_| !track_scroll || scroll::children_are_transferable(world, entity));
        for child in &node.children {
            collect_dirty_walk(
                child,
                world,
                entities,
                idx,
                &tf,
                &tf_3d,
                output_scale,
                track_scroll,
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
            tf,
            tf_3d,
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

fn needs_geometry_preparation(world: &World) -> bool {
    let text_output = world
        .resource::<Outputs<TextGeometry>>()
        .is_some_and(Outputs::has_requests);
    let text_editing = world.has_any_by_id(core::any::TypeId::of::<super::widgets::TextEditing>());
    let text_path = world.resource::<Outputs<TextGeometry>>().is_some()
        && world.has_any_by_id(core::any::TypeId::of::<crate::text::TextPath>());
    let layout_output = world
        .resource::<Outputs<super::LayoutGeometry>>()
        .is_some_and(Outputs::has_requests);
    let layout_participants = super::view::ViewRegistry::prepared_layout_count(world) != 0;
    let output_relations = world
        .resource::<super::outputs::relations::OutputRelations>()
        .is_some_and(|relations| relations.has_current_bindings());
    let offscreen = super::offscreen::has_texture_requests(world)
        || world
            .storage::<super::OffscreenRender>()
            .is_some_and(|storage| !storage.is_empty());

    text_output
        || text_editing
        || text_path
        || layout_output
        || layout_participants
        || output_relations
        || offscreen
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
        if let Err(failure) = compute_layout_snapshot(world, root, logical_w, logical_h, false) {
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
        && !(visual_changed && super::view::ViewRegistry::prepared_layout_count(world) != 0);
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

    let prepare_outputs = needs_geometry_preparation(world);
    let text_changed = if prepare_outputs {
        match prepare_text_geometry(world, &snapshot.entities) {
            Ok(changed) => changed,
            Err(failure) => {
                world.put_resource_box(snapshot);
                record_text_layout_failure(world, root, failure);
                plan.clear();
                return Err(failure);
            }
        }
    } else {
        false
    };
    // A reused snapshot and unchanged text handles produced no new component inputs.
    if layout_recomputed || text_changed {
        super::view::ViewRegistry::invalidate_queries(world);
    }
    if prepare_outputs {
        if let Err(failure) = publish_text_geometry(world, &snapshot.entities) {
            world.put_resource_box(snapshot);
            record_text_layout_failure(world, root, failure);
            plan.clear();
            return Err(failure);
        }
    }
    if layout_recomputed {
        snapshot.publish_scroll_bounds(world);
    }
    let output_plan = if prepare_outputs {
        snapshot.prepare_geometry_plan(world);
        super::layout_geometry::publish(
            world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &snapshot.paths,
            Rect::new(0, 0, logical_w, logical_h),
        );
        super::view::ViewRegistry::propagate_output_publications(world);
        output_plan::measure_for_dirty(world, &snapshot.layout_tree, &snapshot.entities)
    } else {
        snapshot.paths.release();
        snapshot.layout_participants.retire_inactive(world);
        None
    };

    let mut bounds = DirtyBounds {
        regions: super::dirty::RegionSet::new(),
    };

    snapshot.painted_damage.clear();
    {
        crate::trace_span!("dirty.walk");
        let mut idx = 0;
        let track_scroll = scroll::needs_tracking(world);
        collect_dirty_walk(
            &snapshot.layout_tree,
            world,
            &snapshot.entities,
            &mut idx,
            &Transform::IDENTITY,
            &Transform3D::IDENTITY,
            transform.scale(),
            track_scroll,
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
                if plan.shifts.is_empty() {
                    bounds.include(rect);
                } else {
                    for &mapped in scroll::map_painted_damage(rect, &plan.shifts).as_slice() {
                        bounds.include(mapped);
                    }
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
        &|index| snapshot.prepared_paint.subtree_end(index),
    );

    world.put_resource_box(snapshot);
    world.remove_resource::<TextLayoutFailure>();
    Ok(())
}

fn prepare_dirty_rects(world: &mut World, plan: &mut DirtyRegions) {
    if let Some(regions) = world.resource_mut::<super::dirty::ExactDirtyRegions>() {
        let start = plan.rects.len();
        regions.prepare_into(&mut plan.rects);
        if !plan.shifts.is_empty() && start < plan.rects.len() {
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

#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/clip_children_check.rs"]
mod clip_children_check;
#[cfg(test)]
#[path = "render_system/tests/coverage_tests.rs"]
mod coverage_tests;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/disabled_state_check.rs"]
mod disabled_state_check;
#[cfg(test)]
#[path = "render_system/tests/geometry_preparation_tests.rs"]
mod geometry_preparation_tests;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/hidden_check.rs"]
mod hidden_check;
#[cfg(test)]
#[path = "render_system/tests/layout_snapshot_reuse_check.rs"]
mod layout_snapshot_reuse_check;
#[cfg(test)]
#[path = "render_system/tests/local_layout_tests.rs"]
mod local_layout_tests;
#[cfg(test)]
#[path = "render_system/tests/native_target_tests.rs"]
mod native_target_tests;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/offscreen_render_check.rs"]
mod offscreen_render_check;
#[cfg(test)]
#[path = "render_system/tests/overflow_tests.rs"]
mod overflow_tests;
#[cfg(test)]
#[path = "render_system/tests/paint_request_tests.rs"]
mod paint_request_tests;
#[cfg(test)]
#[path = "render_system/tests/projective_transform_tests.rs"]
mod projective_transform_tests;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/scroll_blit_visual_check.rs"]
mod scroll_blit_visual_check;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/scroll_plan_check.rs"]
mod scroll_plan_check;
#[cfg(test)]
#[path = "render_system/tests/state_resolve_check.rs"]
mod state_resolve_check;
#[cfg(all(test, feature = "std"))]
#[path = "render_system/tests/text_layout_check.rs"]
mod text_layout_check;
#[cfg(test)]
#[path = "render_system/tests/text_layout_failure_tests.rs"]
mod text_layout_failure_tests;
#[cfg(test)]
#[path = "render_system/tests/transform_cache_tests.rs"]
mod transform_cache_tests;
