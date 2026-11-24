use super::damage::{planned_visual_bounds, resolve_widget_state};
use super::layout_snapshot::{
    LayoutSnapshot, build_layout_tree, collect_entities_preorder, reset_paint_preparation,
};
use super::{output_plan, prepared_paint, prepared_paint::PreparedPaint};
use crate::core::reactive::PaintRequestScope;
use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::{DrawRequest, FallbackRegion, RenderError, RenderRoute, Renderer};
use crate::render::texture::Texture;
use crate::render::{BackendTarget, PlaneLayout, TargetPlan};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D, Viewport};
use crate::ui::geometry::transformed_clip_bounds;
use crate::ui::layout::{LayoutNode, compute_layout};
use crate::ui::view::{FrameRequest, PaintCtx, ViewRegistry};
use crate::ui::{self, Outputs, Style, WidgetTexture};
use alloc::{boxed::Box, vec::Vec};

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
pub(super) fn rects_intersect(a: &Rect, b: &Rect) -> bool {
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
        && ui::branch::is_hidden_in_tree(world, entity)
    {
        skip_subtree(node, idx, preparation, output_plan);
        return Ok(());
    }
    let parent_typography = entities.get(*idx).and_then(|entity| {
        let parent = world.get::<ui::Parent>(*entity)?.0;
        let style = world.get::<Style>(parent)?;
        Some(ui::Typography::inherited(world, parent, style))
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
    parent_typography: Option<&ui::Typography<'_>>,
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
    let result = ui::widgets::follow_bounds::clip::draw(
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
    parent_typography: Option<&ui::Typography<'_>>,
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
        .filter(|style| ui::Typography::declared_style(style))
        .map(|style| ui::Typography::with_parent(style, parent_typography));
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
pub(super) fn prepare_offscreen(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    shifted_rect: &Rect,
    key: ui::offscreen::BufferKey,
    entity: Entity,
    output_plan: Option<&output_plan::TexturePlan>,
    scratch: &mut crate::render::backends::sw::SwScratch,
) -> Result<bool, RenderError> {
    use crate::render::canvas::Canvas;
    use crate::render::sw::SwRenderer;

    if renderer.offscreen_format().is_none() || ui::branch::is_hidden_in_tree(world, entity) {
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
pub(super) fn draw_output_subtree(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    shifted_rect: &Rect,
    key: ui::offscreen::BufferKey,
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

    ui::widgets::follow_bounds::clip::draw(
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
    off: ui::OffscreenRender,
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

/// Run the render system: build layout → compute → emit logical-coord
/// DrawCommands. Backends convert to physical at draw time.
pub fn render(
    world: &World,
    root: Entity,
    transform: &Viewport,
    renderer: &mut dyn Renderer,
) -> Result<(), RenderError> {
    reset_paint_preparation(world);
    ui::offscreen::reconcile_texture_outputs(world);
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
    ui::offscreen::reconcile_texture_outputs(world);
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
