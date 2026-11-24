use super::layout_snapshot::{
    LayoutSnapshot, build_layout_tree, collect_entities_preorder, compute_layout_snapshot,
    reconcile_layout_snapshot, record_text_layout_failure, reset_paint_preparation,
};
use super::text_layout::{TextLayoutFailure, prepare_text_geometry, publish_text_geometry};
use super::{output_plan, prepared_paint, prepared_paint::PreparedPaint, scroll};
use crate::ecs::{Entity, World};
use crate::text::TextGeometry;
use crate::types::{Fixed, Point, Rect, Transform, Transform3D, Viewport};
use crate::ui::dirty::{DirtyRegions, PaintCoverage, RegionShift};
use crate::ui::geometry::transformed_clip_bounds;
use crate::ui::layout::{LayoutNode, compute_layout};
use crate::ui::state::{InteractionState, UserState};
use crate::ui::theme::WidgetState;
use crate::ui::view::ViewRegistry;
use crate::ui::{self, Outputs, Parent, Style, WidgetTexture};
use alloc::vec::Vec;

pub(super) fn resolve_widget_state(world: &World, entity: Entity) -> WidgetState {
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

pub(super) fn quad_bbox(q: [Point; 4]) -> Rect {
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

pub(super) fn visual_bounds(
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
pub(super) fn planned_visual_bounds(
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
pub(super) fn visible_paint_coverage(
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
                || !ui::branch::is_effectively_hidden(world, entity),
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
    if let Some(painted) = world.get_mut::<ui::dirty::PaintedCoverage>(entity) {
        painted.0 = coverage;
    } else {
        world.insert(entity, ui::dirty::PaintedCoverage(coverage));
    }
    ui::dirty::VisualDirty::acknowledge(world, entity);
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
        .resource::<ui::dirty::ExactDirtyRegions>()
        .is_some_and(|regions| !regions.is_empty());
    if let Some(regions) = world.resource_mut::<ui::dirty::ExactDirtyRegions>() {
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
                    !ui::branch::is_hidden_in_tree(world, root),
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
        if ui::branch::is_authored_hidden(world, root) {
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
        !ui::branch::is_hidden_in_tree(world, root),
        None,
        None,
    );
}

pub(crate) const DIRTY_REGION_CAPACITY: usize = 8;
pub(crate) const DIRTY_PLAN_RECT_CAPACITY: usize = DIRTY_REGION_CAPACITY * 2;

struct DirtyBounds {
    regions: ui::dirty::RegionSet<DIRTY_REGION_CAPACITY>,
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
    use ui::dirty::VisualDirty;
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
            .get::<ui::dirty::PaintedCoverage>(entity)
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
                .get::<ui::OffscreenGeneration>(entity)
                .map(|g| g.0.wrapping_add(1))
                .unwrap_or(1);
            world.insert(entity, ui::OffscreenGeneration(next_gen));
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
    if world.has::<ui::dirty::PaintedCoverage>(entity) {
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
    let Some(inflate) = world.get::<ui::dirty::PaintInflate>(entity) else {
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
    left: Option<ui::dirty::VisualDirty>,
    right: Option<ui::dirty::VisualDirty>,
) -> Option<ui::dirty::VisualDirty> {
    use ui::dirty::VisualDirty;
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
) -> Option<ui::dirty::VisualDirty> {
    use ui::dirty::VisualDirty;
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
            .get::<ui::OffscreenGeneration>(entity)
            .map_or(1, |g| g.0.wrapping_add(1));
        world.insert(entity, ui::OffscreenGeneration(generation));
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

pub(super) fn needs_geometry_preparation(world: &World) -> bool {
    let text_output = world
        .resource::<Outputs<TextGeometry>>()
        .is_some_and(Outputs::has_requests);
    let text_editing = world.has_any_by_id(core::any::TypeId::of::<ui::widgets::TextEditing>());
    let text_path = world.resource::<Outputs<TextGeometry>>().is_some()
        && world.has_any_by_id(core::any::TypeId::of::<crate::text::TextPath>());
    let layout_output = world
        .resource::<Outputs<ui::LayoutGeometry>>()
        .is_some_and(Outputs::has_requests);
    let layout_participants = ui::view::ViewRegistry::prepared_layout_count(world) != 0;
    let output_relations = world
        .resource::<ui::outputs::relations::OutputRelations>()
        .is_some_and(|relations| relations.has_current_bindings());
    let offscreen = ui::offscreen::has_texture_requests(world)
        || world
            .storage::<ui::OffscreenRender>()
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
    ui::view::ViewRegistry::invalidate_queries(world);
    let (logical_w, logical_h) = transform.logical_size();
    plan.clear();

    if let Some(cache) = world.resource::<Outputs<TextGeometry>>() {
        cache.storage_mut().paths.begin_layout();
    }

    if ui::branch::is_authored_hidden(world, root) {
        let had_snapshot = world.resource::<LayoutSnapshot>().is_some();
        if let Err(failure) = compute_layout_snapshot(world, root, logical_w, logical_h, false) {
            record_text_layout_failure(world, root, failure);
            return Err(failure);
        }
        crate::input::event::scroll::bounds::publish(world, &[]);
        ui::view::ViewRegistry::invalidate_queries(world);
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
    use ui::dirty::Dirty;
    let mut has_layout_dirty = ui::dirty::has_visible_dirty::<Dirty>(world, root);
    let has_visual_dirty = ui::dirty::has_visible_dirty::<ui::dirty::VisualDirty>(world, root);
    if !has_layout_dirty && has_visual_dirty && ui::widgets::lazy_list::reconcile(world) {
        crate::core::reactive::flush_signal_dirty(world);
        ui::view::ViewRegistry::invalidate_queries(world);
        has_layout_dirty = true;
    }
    let has_exact_dirty = world
        .resource::<ui::dirty::ExactDirtyRegions>()
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
        && !(visual_changed && ui::view::ViewRegistry::prepared_layout_count(world) != 0);
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
        if let Some(outputs) = world.resource::<Outputs<ui::LayoutGeometry>>() {
            outputs.reset_all();
        }
        world.remove_resource::<TextLayoutFailure>();
        ui::view::ViewRegistry::invalidate_queries(world);
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
        ui::view::ViewRegistry::invalidate_queries(world);
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
        ui::layout_geometry::publish(
            world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &snapshot.paths,
            Rect::new(0, 0, logical_w, logical_h),
        );
        ui::view::ViewRegistry::propagate_output_publications(world);
        output_plan::measure_for_dirty(world, &snapshot.layout_tree, &snapshot.entities)
    } else {
        snapshot.paths.release();
        snapshot.layout_participants.retire_inactive(world);
        None
    };

    let mut bounds = DirtyBounds {
        regions: ui::dirty::RegionSet::new(),
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
            !ui::branch::is_hidden_in_tree(world, root),
            &mut bounds,
            plan,
            &mut snapshot.painted_damage,
            output_plan.as_ref(),
            &snapshot.prepared_paint,
        );
    }
    output_plan::return_plan(world, output_plan);

    for entity in &snapshot.painted_damage {
        if let Some(painted) = world.get::<ui::dirty::PaintedCoverage>(*entity) {
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

pub(super) fn prepare_dirty_rects(world: &mut World, plan: &mut DirtyRegions) {
    if let Some(regions) = world.resource_mut::<ui::dirty::ExactDirtyRegions>() {
        let start = plan.rects.len();
        regions.prepare_into(&mut plan.rects);
        if !plan.shifts.is_empty() && start < plan.rects.len() {
            let mut exact = ui::dirty::RegionSet::<DIRTY_REGION_CAPACITY>::new();
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
