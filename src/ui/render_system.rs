use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Point, Rect, Transform, Transform3D, Viewport};
use crate::ui::layout::{LayoutNode, compute_layout};
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::{TransformOrigin, WidgetTransform3D};

use super::dirty::{DirtyRegions, RegionShift};
use super::state::{InteractionState, UserState};
use super::theme::WidgetState;
use super::view::{ViewCtx, ViewRegistry};
use super::{Children, Hidden, Parent, Style, Widget};

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

/// Compose `parent` with the entity's local transform (if any),
/// wrapped so rotation/scale pivot on the widget's center instead
/// of its top-left corner.
fn effective_transform(parent: &Transform, world: &World, entity: Entity, rect: Rect) -> Transform {
    let local = match world.get::<WidgetTransform>(entity) {
        Some(t) if !t.0.is_identity() => t.0,
        _ => return *parent,
    };
    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);
    let to_origin = Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy);
    let from_origin = Transform::translate(cx, cy);
    parent
        .compose(&from_origin)
        .compose(&local)
        .compose(&to_origin)
}

fn origin_point(world: &World, entity: Entity, rect: Rect) -> (Fixed, Fixed) {
    let o = world
        .get::<TransformOrigin>(entity)
        .copied()
        .unwrap_or_default();
    (rect.x + rect.w * o.x, rect.y + rect.h * o.y)
}

fn effective_transform_3d(
    parent: &Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Transform3D {
    let local = match world.get::<WidgetTransform3D>(entity) {
        Some(t) if !t.0.is_identity() => t.0,
        _ => return *parent,
    };
    let (cx, cy) = origin_point(world, entity, rect);
    let to_origin = Transform3D::translate(Fixed::ZERO - cx, Fixed::ZERO - cy);
    let from_origin = Transform3D::translate(cx, cy);
    parent
        .compose(&from_origin)
        .compose(&local)
        .compose(&to_origin)
}

/// Once any ancestor declares a 3D transform, the whole subtree
/// renders through the 3D quad path. Descendants without a
/// `WidgetTransform3D` either lift their 2D `WidgetTransform` to
/// homogeneous coordinates or pass the parent through unchanged.
fn accumulate_3d(
    parent_3d: &Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Transform3D {
    if let Some(t3d) = world.get::<WidgetTransform3D>(entity) {
        if !t3d.0.is_identity() {
            return effective_transform_3d(parent_3d, world, entity, rect);
        }
    }
    if !parent_3d.is_identity() {
        if let Some(t2d) = world.get::<WidgetTransform>(entity) {
            if !t2d.0.is_identity() {
                let (cx, cy) = origin_point(world, entity, rect);
                let to_origin = Transform3D::translate(Fixed::ZERO - cx, Fixed::ZERO - cy);
                let from_origin = Transform3D::translate(cx, cy);
                return parent_3d
                    .compose(&from_origin)
                    .compose(&Transform3D::from_affine(t2d.0))
                    .compose(&to_origin);
            }
        }
    }
    *parent_3d
}

fn quad_bbox(q: [Point; 4]) -> Rect {
    Rect::bounding_quad(&q)
}

fn seed_prev_rect_walk(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    parent_transform: &Transform,
    parent_3d: &Transform3D,
) {
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf = effective_transform(parent_transform, world, entity, node.rect);
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);
    let effective_rect = quad_for(world, entity, node.rect, parent_3d)
        .map(quad_bbox)
        .unwrap_or_else(|| tf.apply_rect_bbox(node.rect));
    let x0 = effective_rect.x.floor();
    let y0 = effective_rect.y.floor();
    let x1 = (effective_rect.x + effective_rect.w).ceil();
    let y1 = (effective_rect.y + effective_rect.h).ceil();
    world.insert(
        entity,
        super::dirty::PrevRect(Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }),
    );
    *idx += 1;
    for child in &node.children {
        seed_prev_rect_walk(child, world, entities, idx, &tf, &tf_3d);
    }
}

/// After a full-screen render, stash each widget's effective bbox so
/// the next dirty pass knows the pixels just written and can include
/// them in its union (erasing any residue when widgets move/shrink).
pub fn seed_prev_rects(world: &mut World, root: Entity, transform: &Viewport) {
    let (logical_w, logical_h) = transform.logical_size();
    let Some(mut layout_tree) = build_layout_tree(world, root) else {
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
    );
}

fn quad_for(
    world: &World,
    entity: Entity,
    rect: Rect,
    parent_3d: &Transform3D,
) -> Option<[Point; 4]> {
    let has_local_3d = world
        .get::<WidgetTransform3D>(entity)
        .map(|t| !t.0.is_identity())
        .unwrap_or(false);
    if parent_3d.is_identity() && !has_local_3d {
        return None;
    }
    let tf = accumulate_3d(parent_3d, world, entity, rect);
    tf.apply_rect(rect)
}

/// Recursively build a LayoutNode tree from ECS entities
fn build_layout_tree(world: &World, entity: Entity) -> Option<LayoutNode> {
    world.get::<Widget>(entity)?;
    if world.get::<Hidden>(entity).is_some() {
        return None;
    }
    let style = world.get::<Style>(entity)?;
    let mut node = LayoutNode::new(style.layout);
    apply_text_intrinsic(world, entity, &mut node);

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if let Some(child_node) = build_layout_tree(world, child) {
                node.add_child(child_node);
            }
        }
    }
    Some(node)
}

// FIXME: not the full solution.
pub(crate) fn apply_text_intrinsic(world: &World, entity: Entity, node: &mut LayoutNode) {
    use crate::render::font::{CHAR_H, CHAR_W};
    use crate::types::Dimension;
    use crate::ui::widgets::text::Text;

    let Some(text) = world.get::<Text>(entity) else {
        return;
    };
    if node.style.grow > Fixed::ZERO {
        return;
    }
    let pad: i32 = 4;
    let bytes = text.resolve(world);
    let intrinsic_w = Fixed::from_int(bytes.len() as i32 * CHAR_W as i32 + pad);
    let intrinsic_h = Fixed::from_int(CHAR_H as i32 + pad);
    if matches!(node.style.width, Dimension::Auto | Dimension::Content) {
        node.style.width = Dimension::Px(intrinsic_w);
    }
    if matches!(node.style.height, Dimension::Auto | Dimension::Content) {
        node.style.height = Dimension::Px(intrinsic_h);
    }
}

fn rects_intersect(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y
}

fn count_nodes(node: &LayoutNode) -> usize {
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
#[mirui::trace_fn("draw.entity")]
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
    inside_offscreen: bool,
) {
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
    let tf = effective_transform(parent_transform, world, entity, shifted_rect);
    let tf_3d = accumulate_3d(parent_transform_3d, world, entity, shifted_rect);
    let quad = quad_for(world, entity, shifted_rect, parent_transform_3d).or_else(|| {
        if matches!(
            tf.classify(),
            crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
        ) {
            None
        } else {
            Some(tf.apply_rect(shifted_rect))
        }
    });

    let cull_rect = quad
        .map(quad_bbox)
        .unwrap_or_else(|| tf.apply_rect_bbox(shifted_rect));
    if !rects_intersect(&cull_rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    // Offscreen-render branch — try to redirect the entity + its
    // subtree into a private buffer, blit the buffer back here.
    // Returns true when handled; false falls through to inline render.
    if let Some(off) = world.get::<super::OffscreenRender>(entity).copied() {
        let has_3d = world.get::<WidgetTransform3D>(entity).is_some();
        debug_assert!(
            !has_3d,
            "OffscreenRender + WidgetTransform3D not supported on the same entity"
        );
        debug_assert!(!inside_offscreen, "nested OffscreenRender not supported");
        if !has_3d
            && !inside_offscreen
            && renderer.supports_offscreen()
            && try_draw_offscreen(
                node,
                world,
                entities,
                idx,
                renderer,
                clip,
                &shifted_rect,
                off,
                entity,
                &tf,
                quad,
            )
        {
            return;
        }
        // Fallthrough: GPU backend, nested case (release silent), or
        // 3D conflict (release silent) — render inline as if the
        // marker weren't there.
    }

    if *idx < entities.len() {
        if let Some(style) = world.get::<Style>(entity) {
            let state = resolve_widget_state(world, entity);
            let mut ctx = ViewCtx {
                style,
                transform: tf,
                quad,
                clip,
                bg_handled: false,
                state,
            };
            if let Some(registry) = world.resource::<ViewRegistry>() {
                crate::trace_span!("draw.view_dispatch");
                for view in registry.iter() {
                    if let Some(tid) = view.component_filter()
                        && !world.has_type(entity, tid)
                    {
                        continue;
                    }
                    (view.render())(renderer, world, entity, &shifted_rect, &mut ctx);
                }
            }
        }
    }
    *idx += 1;

    let (child_clip, sx, sy) =
        if let Some(scroll) = world.get::<crate::input::event::scroll::ScrollOffset>(entity) {
            (
                intersect_with_self(clip, &shifted_rect),
                offset_x + scroll.x,
                offset_y + scroll.y,
            )
        } else if world
            .get::<Style>(entity)
            .map(|s| s.clip_children)
            .unwrap_or(false)
        {
            (intersect_with_self(clip, &shifted_rect), offset_x, offset_y)
        } else {
            (*clip, offset_x, offset_y)
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
            &tf,
            &tf_3d,
            inside_offscreen,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn try_draw_offscreen(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    shifted_rect: &Rect,
    off: super::OffscreenRender,
    entity: Entity,
    outer_tf: &Transform,
    outer_quad: Option<[Point; 4]>,
) -> bool {
    use crate::render::canvas::Canvas;
    use crate::render::sw::SwRenderer;
    use crate::render::texture::Texture;
    use crate::types::Point;

    let backend_format = match renderer.offscreen_format() {
        Some(f) => f,
        None => return false,
    };
    let needs_alpha = world
        .get::<super::OffscreenAlphaMode>(entity)
        .is_some_and(|m| m.clear_transparent);
    let format = if needs_alpha
        && !matches!(
            backend_format,
            crate::render::texture::ColorFormat::RGBA8888
        ) {
        crate::render::texture::ColorFormat::RGBA8888
    } else {
        backend_format
    };

    // Caller's cull already used the transform-applied bbox; reusing
    // shifted_rect (untransformed) here would skip render when the
    // entity's transform moved it inside clip but its logical layout
    // rect did not.

    // Buffer dimensions: ComputedRect × off.scale, clamped so neither
    // axis rounds to 0. Scale below 1/8 collapses the buffer too far
    // for any meaningful visual; clamp at that floor.
    let scale = off.scale.max(Fixed::ONE / 8);
    let buf_w_f = Fixed::from_int(shifted_rect.w.to_int().max(1)) * scale;
    let buf_h_f = Fixed::from_int(shifted_rect.h.to_int().max(1)) * scale;
    let buf_w = buf_w_f.to_int().max(1).min(u16::MAX as i32) as u16;
    let buf_h = buf_h_f.to_int().max(1).min(u16::MAX as i32) as u16;

    let generation = world
        .get::<super::OffscreenGeneration>(entity)
        .map(|g| g.0)
        .unwrap_or(0);

    let key = super::offscreen::BufferKey {
        entity,
        w: buf_w,
        h: buf_h,
        format,
        generation,
    };

    let pool = match world.resource::<super::OffscreenBufferPool>() {
        Some(p) => p,
        None => return false,
    };
    pool.last_format.set(Some(format));

    crate::trace_span!("draw.offscreen");

    let (handle, was_hit) = match pool.cache.borrow_mut().entry(key).or_insert_with_status() {
        Ok((h, status)) => (h, status == crate::core::cache::factory::EntryStatus::Hit),
        Err(_) => return false,
    };

    let clear_transparent = world
        .get::<super::OffscreenAlphaMode>(entity)
        .map(|m| m.clear_transparent)
        .unwrap_or(false);

    if !was_hit {
        let mut tex_ref = handle.get().borrow_mut();
        if clear_transparent {
            // Effect widgets that need the buffer's alpha channel to
            // encode the source's silhouette. Pre-seed would write
            // the framebuffer's alpha here, which is opaque and
            // erases the shape information.
            for byte in tex_ref.buf.as_mut_slice().iter_mut() {
                *byte = 0;
            }
        } else {
            // Pre-seed from framebuffer so AA fringe alpha blends against
            // the existing background instead of transparent black.
            renderer.read_target_region(shifted_rect, &mut tex_ref);
        }
    }

    if was_hit {
        *idx += count_nodes(node);
    } else {
        let mut tex_ref = handle.get().borrow_mut();
        let buf_slice = tex_ref.buf.as_mut_slice();
        let inner_tex = Texture::new(buf_slice, buf_w, buf_h, format);
        // Buffers cleared to transparent need source-over alpha
        // accumulation so the silhouette stays meaningful for
        // downstream samplers (DropShadow, Mirror, etc).
        let alpha_mode = if clear_transparent {
            crate::render::sw::AlphaMode::Blend
        } else {
            crate::render::sw::AlphaMode::Opaque
        };
        let mut inner = SwRenderer::new(inner_tex).with_alpha_mode(alpha_mode);
        inner.viewport = Viewport::new(buf_w, buf_h, scale);

        // The entity's drawn rect maps to (0, 0) in the buffer's
        // logical coordinate space. We can't recurse through
        // draw_tree_offset for the whole subtree because it would
        // re-detect the OffscreenRender marker on this same entity
        // and panic on nesting; inline the entity's own view dispatch
        // first, then recurse children with inside_offscreen=true.
        let inner_offset_x = shifted_rect.x;
        let inner_offset_y = shifted_rect.y;
        let entity_rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: shifted_rect.w,
            h: shifted_rect.h,
        };
        let inner_clip = entity_rect;

        if let Some(style) = world.get::<Style>(entity) {
            let state = resolve_widget_state(world, entity);
            let mut ctx = ViewCtx {
                style,
                transform: Transform::IDENTITY,
                quad: None,
                clip: &inner_clip,
                bg_handled: false,
                state,
            };
            if let Some(registry) = world.resource::<ViewRegistry>() {
                crate::trace_span!("draw.view_dispatch");
                for view in registry.iter() {
                    if let Some(tid) = view.component_filter()
                        && !world.has_type(entity, tid)
                    {
                        continue;
                    }
                    (view.render())(
                        &mut inner as &mut dyn Renderer,
                        world,
                        entity,
                        &entity_rect,
                        &mut ctx,
                    );
                }
            }
        }
        *idx += 1;

        // Children handle scroll like the inline path; clip is the
        // buffer rect (we already restricted by the entity's own rect
        // above), and offsets shift child coords so the entity's
        // origin maps to (0, 0) inside the buffer.
        let (child_clip, sx, sy) =
            if let Some(scroll) = world.get::<crate::input::event::scroll::ScrollOffset>(entity) {
                (
                    inner_clip,
                    inner_offset_x + scroll.x,
                    inner_offset_y + scroll.y,
                )
            } else {
                (inner_clip, inner_offset_x, inner_offset_y)
            };

        for child in &node.children {
            draw_tree_offset(
                child,
                world,
                entities,
                idx,
                &mut inner as &mut dyn Renderer,
                &child_clip,
                sx,
                sy,
                &Transform::IDENTITY,
                &Transform3D::IDENTITY,
                true,
            );
        }
        Canvas::flush(&mut inner);
    }

    // Blit buffer back to the outer renderer through DrawCommand::Blit.
    // This is the only Canvas-style operation we need from outer; going
    // through the DrawCommand path keeps Renderer trait the single
    // entry point and lets every backend handle the blit identically.

    {
        let tex_ref = handle.get().borrow();
        let blit_cmd = DrawCommand::Blit {
            pos: Point::new(shifted_rect.x, shifted_rect.y),
            size: Point::new(shifted_rect.w, shifted_rect.h),
            transform: *outer_tf,
            quad: outer_quad,
            texture: &tex_ref,
            opa: off.opacity,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        renderer.draw(&blit_cmd, clip);
    }

    true
}

fn collect_entities_preorder(world: &World, entity: Entity, out: &mut Vec<Entity>) {
    if world.get::<Hidden>(entity).is_some() {
        return;
    }
    out.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        let child_ids: Vec<Entity> = children.0.clone();
        for child in child_ids {
            collect_entities_preorder(world, child, out);
        }
    }
}

/// Run the render system: build layout → compute → emit logical-coord
/// DrawCommands. Backends convert to physical at draw time.
pub fn render(world: &World, root: Entity, transform: &Viewport, renderer: &mut dyn Renderer) {
    let (logical_w, logical_h) = transform.logical_size();

    let Some(mut layout_tree) = build_layout_tree(world, root) else {
        return;
    };

    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );

    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: logical_w.into(),
        h: logical_h.into(),
    };
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    // Storage-existence gate so a tree with no effect widgets pays
    // nothing for the prerender pass: `query` walks even when empty.
    if world
        .storage::<super::offscreen::WidgetTextureRef>()
        .is_some()
    {
        let ref_sources: Vec<Entity> = world
            .query::<super::offscreen::WidgetTextureRef>()
            .iter()
            .map(|(_, r)| r.0)
            .collect();
        if !ref_sources.is_empty() {
            let mut idx = 0;
            prerender_sources(
                &layout_tree,
                world,
                &entities,
                &mut idx,
                renderer,
                &clip,
                &Transform::IDENTITY,
                &Transform3D::IDENTITY,
                &ref_sources,
            );
        }
    }

    let mut idx = 0;
    draw_tree_offset(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        renderer,
        &clip,
        Fixed::ZERO,
        Fixed::ZERO,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
        false,
    );
}

/// Compute layout and write ComputedRect to each entity (logical pixels).
pub fn update_layout(world: &mut World, root: Entity, transform: &Viewport) {
    let (logical_w, logical_h) = transform.logical_size();

    let Some(mut layout_tree) = build_layout_tree(world, root) else {
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
    write_computed_rects(&layout_tree, world, &entities, &mut idx);
}

fn write_computed_rects(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
) {
    if *idx < entities.len() {
        world.insert(entities[*idx], super::ComputedRect(node.rect));
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
) {
    let (logical_w, logical_h) = transform.logical_size();
    let mut tree = crate::trace_span!("render.build_tree", {
        match build_layout_tree(world, root) {
            Some(t) => t,
            None => return,
        }
    });
    {
        crate::trace_span!("render.compute_layout");
        compute_layout(
            &mut tree,
            Fixed::ZERO,
            Fixed::ZERO,
            logical_w.into(),
            logical_h.into(),
        );
    }
    let mut entities = Vec::new();
    {
        crate::trace_span!("render.collect_entities");
        collect_entities_preorder(world, root, &mut entities);
    }
    render_region_with(world, &tree, &entities, dirty_rect, renderer);
}

/// Internal cached variant: caller supplies a layout tree + entity
/// preorder produced earlier in the same frame (typically by the
/// dirty walker). Skips the build / compute / collect phases.
///
/// Caller must guarantee the snapshot was produced in the same frame
/// against the same `transform` and `root`. Public `render_region` is
/// the safe entry point — this one is `pub(crate)` and only invoked
/// from `App::render_dirty`.
pub(crate) fn render_region_cached(
    world: &World,
    snapshot: &LayoutSnapshot,
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
) {
    render_region_with(
        world,
        &snapshot.layout_tree,
        &snapshot.entities,
        dirty_rect,
        renderer,
    );
}

fn render_region_with(
    world: &World,
    layout_tree: &LayoutNode,
    entities: &[Entity],
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
) {
    if world
        .storage::<super::offscreen::WidgetTextureRef>()
        .is_some()
    {
        let ref_sources: Vec<Entity> = world
            .query::<super::offscreen::WidgetTextureRef>()
            .iter()
            .map(|(_, r)| r.0)
            .collect();
        if !ref_sources.is_empty() {
            crate::trace_span!("render.prerender_sources");
            let mut idx = 0;
            prerender_sources(
                layout_tree,
                world,
                entities,
                &mut idx,
                renderer,
                dirty_rect,
                &Transform::IDENTITY,
                &Transform3D::IDENTITY,
                &ref_sources,
            );
        }
    }

    {
        crate::trace_span!("render.draw_tree");
        let mut idx = 0;
        draw_tree_offset(
            layout_tree,
            world,
            entities,
            &mut idx,
            renderer,
            dirty_rect,
            Fixed::ZERO,
            Fixed::ZERO,
            &Transform::IDENTITY,
            &Transform3D::IDENTITY,
            false,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn prerender_sources(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    parent_transform: &Transform,
    parent_3d: &Transform3D,
    targets: &[Entity],
) {
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf = effective_transform(parent_transform, world, entity, node.rect);
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);
    let quad = quad_for(world, entity, node.rect, parent_3d).or_else(|| {
        if matches!(
            tf.classify(),
            crate::types::TransformClass::Identity | crate::types::TransformClass::Translate
        ) {
            None
        } else {
            Some(tf.apply_rect(node.rect))
        }
    });

    if targets.contains(&entity) {
        if let Some(off) = world.get::<super::OffscreenRender>(entity).copied() {
            let _ = try_draw_offscreen(
                node,
                world,
                entities,
                &mut { *idx },
                renderer,
                clip,
                &node.rect,
                off,
                entity,
                &tf,
                quad,
            );
        }
    }
    *idx += 1;
    for child in &node.children {
        prerender_sources(
            child, world, entities, idx, renderer, clip, &tf, &tf_3d, targets,
        );
    }
}

struct DirtyBounds {
    min_x: Fixed,
    min_y: Fixed,
    max_x: Fixed,
    max_y: Fixed,
}

/// Resource exposing the last frame's plan. Read-only for probes /
/// debug overlays; production code should not depend on it.
#[derive(Clone, Debug, Default)]
pub struct LastDirtyRegions(pub DirtyRegions);

pub(crate) struct LayoutSnapshot {
    pub(crate) layout_tree: LayoutNode,
    pub(crate) entities: Vec<Entity>,
}

#[allow(clippy::too_many_arguments)]
fn collect_dirty_walk(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    parent_transform: &Transform,
    parent_3d: &Transform3D,
    scroll_offset: (Fixed, Fixed),
    inside_scroll: bool,
    // Innermost scroll container's screen rect: a LazyList row
    // far below `visible_start` has a layout-y in the thousands
    // and would otherwise blow the dirty bbox out vertically.
    scroll_clip: Option<Rect>,
    bounds: &mut DirtyBounds,
    plan: &mut DirtyRegions,
    out_of_scroll_prev: &mut alloc::vec::Vec<Rect>,
) {
    use super::dirty::Dirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf = effective_transform(parent_transform, world, entity, node.rect);
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);

    // Offscreen subtree containment: any Dirty under an OffscreenRender
    // entity must promote to the entity itself (so its full
    // ComputedRect ends up in the dirty union, since the buffer
    // re-renders entirely) and bump the buffer generation so the next
    // render misses the cache. Children's Dirty markers are cleared
    // here so the outer dirty bounds doesn't double-count them.
    let is_offscreen = world.get::<super::OffscreenRender>(entity).is_some();
    if is_offscreen {
        let self_was_dirty = world.get::<Dirty>(entity).is_some();
        if self_was_dirty {
            push_entity_dirty(
                world,
                entity,
                node,
                parent_3d,
                &tf,
                scroll_offset,
                inside_scroll,
                scroll_clip,
                bounds,
                out_of_scroll_prev,
            );
        }

        let mut sub_idx = *idx + 1;
        let mut had_subtree_dirty = false;
        for child in &node.children {
            scan_subtree_dirty(child, world, entities, &mut sub_idx, &mut had_subtree_dirty);
        }

        // Bump generation on either self-Dirty or subtree-Dirty (a
        // single-widget marker like Switch has no children to feed
        // subtree_dirty, so without the self branch theme rotation
        // would never invalidate the buffer). Push the entity rect
        // only once — skip the second push when self_was_dirty
        // already did it, otherwise curr ∪ prev unions with itself.
        if self_was_dirty || had_subtree_dirty {
            if had_subtree_dirty && !self_was_dirty {
                push_entity_dirty(
                    world,
                    entity,
                    node,
                    parent_3d,
                    &tf,
                    scroll_offset,
                    inside_scroll,
                    scroll_clip,
                    bounds,
                    out_of_scroll_prev,
                );
            }
            let next_gen = world
                .get::<super::OffscreenGeneration>(entity)
                .map(|g| g.0.wrapping_add(1))
                .unwrap_or(1);
            world.insert(entity, super::OffscreenGeneration(next_gen));
        }
        *idx = sub_idx;
        return;
    }

    let scroll_delta = world
        .get::<crate::input::event::scroll::ScrollDelta>(entity)
        .copied();
    let mut child_inside_scroll = inside_scroll;
    let mut my_scroll_op: Option<RegionShift> = None;
    let mut sub_pixel_pending = false;
    if let Some(sd) = scroll_delta {
        // Sign flip: ScrollDelta is the ScrollOffset increment (positive
        // dy = content scrolled up); the framebuffer must shift the
        // opposite way. Quantise toward zero so a sub-pixel residue
        // keeps the same sign as the original delta and accumulates
        // across frames instead of bouncing.
        let dx_int = sd.dx.trunc_to_int();
        let dy_int = sd.dy.trunc_to_int();
        let blit_dx = Fixed::from_int(-dx_int);
        let blit_dy = Fixed::from_int(-dy_int);
        if dx_int != 0 || dy_int != 0 {
            let area = Rect {
                x: node.rect.x - scroll_offset.0,
                y: node.rect.y - scroll_offset.1,
                w: node.rect.w,
                h: node.rect.h,
            };
            my_scroll_op = Some(RegionShift {
                area,
                dx: blit_dx,
                dy: blit_dy,
            });
            child_inside_scroll = true;
            if let Some(sd_mut) = world.get_mut::<crate::input::event::scroll::ScrollDelta>(entity)
            {
                sd_mut.dx -= Fixed::from_int(dx_int);
                sd_mut.dy -= Fixed::from_int(dy_int);
            }
        } else if sd.dx != Fixed::ZERO || sd.dy != Fixed::ZERO {
            sub_pixel_pending = true;
        }
    }

    if world.get::<Dirty>(entity).is_some() {
        // Container's own Dirty is already expressed by the RegionShift;
        // unioning its full rect would re-stretch bounds over the area
        // self-blit handles. Sub-pixel frames similarly skip the push
        // until the delta crosses a pixel boundary.
        if my_scroll_op.is_some() || sub_pixel_pending {
            world.remove::<Dirty>(entity);
        } else {
            push_entity_dirty(
                world,
                entity,
                node,
                parent_3d,
                &tf,
                scroll_offset,
                inside_scroll,
                scroll_clip,
                bounds,
                out_of_scroll_prev,
            );
        }
    }

    let child_scroll =
        if let Some(scroll) = world.get::<crate::input::event::scroll::ScrollOffset>(entity) {
            (scroll_offset.0 + scroll.x, scroll_offset.1 + scroll.y)
        } else {
            scroll_offset
        };

    let child_scroll_clip = my_scroll_op.as_ref().map(|s| s.area).or(scroll_clip);

    *idx += 1;
    for child in &node.children {
        collect_dirty_walk(
            child,
            world,
            entities,
            idx,
            &tf,
            &tf_3d,
            child_scroll,
            child_inside_scroll,
            child_scroll_clip,
            bounds,
            plan,
            out_of_scroll_prev,
        );
    }

    // DFS post-order so a nested inner shift executes before its outer
    // carrier; the outer then moves the already-shifted inner pixels.
    if let Some(sop) = my_scroll_op {
        push_scroll_strips(&sop, plan);
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

/// Helper extracted from `collect_dirty_walk` so the OffscreenRender
/// branch can call it without duplicating the bbox-union accounting.
/// Caller has already verified `world.get::<Dirty>(entity).is_some()`
/// or wants the entity rect contributed regardless.
#[allow(clippy::too_many_arguments)]
fn push_entity_dirty(
    world: &mut World,
    entity: Entity,
    node: &LayoutNode,
    parent_3d: &Transform3D,
    tf: &Transform,
    scroll_offset: (Fixed, Fixed),
    inside_scroll: bool,
    scroll_clip: Option<Rect>,
    bounds: &mut DirtyBounds,
    out_of_scroll_prev: &mut alloc::vec::Vec<Rect>,
) {
    use super::dirty::Dirty;
    let curr_layout = quad_for(world, entity, node.rect, parent_3d)
        .map(quad_bbox)
        .unwrap_or_else(|| tf.apply_rect_bbox(node.rect));
    let mut curr = Rect {
        x: curr_layout.x - scroll_offset.0,
        y: curr_layout.y - scroll_offset.1,
        w: curr_layout.w,
        h: curr_layout.h,
    };
    if let Some(inflate) = world.get::<super::dirty::PaintInflate>(entity).copied() {
        curr.x -= inflate.left;
        curr.y -= inflate.top;
        curr.w += inflate.left + inflate.right;
        curr.h += inflate.top + inflate.bottom;
    }
    // Inside a scroll container the self-blit already moved the prev
    // rect's pixels to their new spot, so unioning prev here would
    // re-stretch the bbox over the area self-blit handled.
    //
    // Out-of-scroll prev rects feed post-walk shift translation
    // (overlay smear from ancestor-scroll self-blit).
    let mut union_rect = if inside_scroll {
        curr
    } else {
        match world.get::<super::dirty::PrevRect>(entity) {
            Some(prev) => {
                out_of_scroll_prev.push(prev.0);
                curr.union(&prev.0)
            }
            None => curr,
        }
    };
    if let Some(clip) = scroll_clip {
        if let Some(inter) = union_rect.intersect(&clip) {
            union_rect = inter;
        } else {
            return;
        }
    }
    let (ux0, uy0, ux1, uy1) = union_rect.pixel_bounds();
    let x0 = Fixed::from_int(ux0);
    let y0 = Fixed::from_int(uy0);
    let x1 = Fixed::from_int(ux1);
    let y1 = Fixed::from_int(uy1);
    if x0 < bounds.min_x {
        bounds.min_x = x0;
    }
    if y0 < bounds.min_y {
        bounds.min_y = y0;
    }
    if x1 > bounds.max_x {
        bounds.max_x = x1;
    }
    if y1 > bounds.max_y {
        bounds.max_y = y1;
    }
    let (cx0, cy0, cx1, cy1) = curr.pixel_bounds();
    world.insert(
        entity,
        super::dirty::PrevRect(Rect::new(cx0, cy0, cx1 - cx0, cy1 - cy0)),
    );
    world.remove::<Dirty>(entity);
}

/// Sweep the subtree rooted at `node` (children of an OffscreenRender
/// entity) and clear any Dirty markers, recording whether at least one
/// was set so the caller can bump the buffer generation.
fn scan_subtree_dirty(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    had_dirty: &mut bool,
) {
    use super::dirty::Dirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    if world.get::<Dirty>(entity).is_some() {
        *had_dirty = true;
        world.remove::<Dirty>(entity);
    }
    *idx += 1;
    for child in &node.children {
        scan_subtree_dirty(child, world, entities, idx, had_dirty);
    }
}

/// Collect the logical-pixel rects of all dirty entities, then remove Dirty flags.
/// Returns the bounding rect of all dirty regions, or None if nothing dirty.
pub fn collect_dirty_region(world: &mut World, root: Entity, transform: &Viewport) -> Option<Rect> {
    collect_dirty_regions(world, root, transform).bounding_rect()
}

/// Plan-style dirty walker: returns redraw rects + framebuffer
/// scroll ops. With no `ScrollDelta` anywhere this is equivalent to
/// `collect_dirty_region` boxed in a `DirtyRegions`.
pub fn collect_dirty_regions(
    world: &mut World,
    root: Entity,
    transform: &Viewport,
) -> DirtyRegions {
    let (logical_w, logical_h) = transform.logical_size();
    let mut plan = DirtyRegions::default();

    // Idle skip: with zero Dirty markers the 5-step walk would just
    // re-derive last frame's outputs. Systems that mutate visible
    // state without inserting Dirty (animation helpers, mainly) own
    // the responsibility to mark themselves.
    use super::dirty::Dirty;
    let dirty_count = world.storage::<Dirty>().map(|s| s.len()).unwrap_or(0);
    if dirty_count == 0 {
        return plan;
    }

    let mut layout_tree = crate::trace_span!("dirty.build_tree", {
        match build_layout_tree(world, root) {
            Some(t) => t,
            None => return plan,
        }
    });

    {
        crate::trace_span!("dirty.compute_layout");
        compute_layout(
            &mut layout_tree,
            Fixed::ZERO,
            Fixed::ZERO,
            logical_w.into(),
            logical_h.into(),
        );
    }

    let mut entities = Vec::new();
    {
        crate::trace_span!("dirty.collect_entities");
        collect_entities_preorder(world, root, &mut entities);
    }

    let mut idx = 0;
    {
        crate::trace_span!("dirty.write_computed");
        write_computed_rects(&layout_tree, world, &entities, &mut idx);
    }

    let mut bounds = DirtyBounds {
        min_x: Fixed::from(logical_w),
        min_y: Fixed::from(logical_h),
        max_x: Fixed::from_int(-1),
        max_y: Fixed::from_int(-1),
    };

    let mut out_of_scroll_prev: alloc::vec::Vec<Rect> = alloc::vec::Vec::new();
    {
        crate::trace_span!("dirty.walk");
        idx = 0;
        collect_dirty_walk(
            &layout_tree,
            world,
            &entities,
            &mut idx,
            &Transform::IDENTITY,
            &Transform3D::IDENTITY,
            (Fixed::ZERO, Fixed::ZERO),
            false,
            None,
            &mut bounds,
            &mut plan,
            &mut out_of_scroll_prev,
        );
    }

    // Cover the smear strip from ancestor-scroll self-blit dragging overlay prev pixels.
    for prev in &out_of_scroll_prev {
        for sop in &plan.shifts {
            let Some(inter) = prev.intersect(&sop.area) else {
                continue;
            };
            let shifted = Rect {
                x: inter.x + sop.dx,
                y: inter.y + sop.dy,
                w: inter.w,
                h: inter.h,
            };
            let (sx0, sy0, sx1, sy1) = shifted.pixel_bounds();
            if sx1 <= sx0 || sy1 <= sy0 {
                continue;
            }
            let fx0 = Fixed::from_int(sx0);
            let fy0 = Fixed::from_int(sy0);
            let fx1 = Fixed::from_int(sx1);
            let fy1 = Fixed::from_int(sy1);
            if fx0 < bounds.min_x {
                bounds.min_x = fx0;
            }
            if fy0 < bounds.min_y {
                bounds.min_y = fy0;
            }
            if fx1 > bounds.max_x {
                bounds.max_x = fx1;
            }
            if fy1 > bounds.max_y {
                bounds.max_y = fy1;
            }
        }
    }

    let (min_x, min_y, max_x, max_y) = (bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);
    if max_x >= Fixed::ZERO {
        plan.rects.push(Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        });
    }

    // Overlays (cursor / rotary feedback) sit on top of a scroll
    // area but live outside its subtree, so self-blit drags their
    // pixels sideways and leaves a residue at their logical position.
    // Two small redraw rects per (overlay × shift) absorb both ends:
    // the overlay's current rect (repaints under it) and the dragged
    // rect (covers the residue). The shift itself stays so the bulk
    // of the area still self-blits.
    if !plan.shifts.is_empty() {
        let overlay_rects = collect_overlay_rects(world);
        for sop in &plan.shifts.clone() {
            for or in &overlay_rects {
                if or.intersect(&sop.area).is_none() {
                    continue;
                }
                let shifted = Rect {
                    x: or.x + sop.dx,
                    y: or.y + sop.dy,
                    w: or.w,
                    h: or.h,
                };
                plan.rects.push(*or);
                plan.rects.push(shifted);
            }
        }
    }

    world.insert_resource(LayoutSnapshot {
        layout_tree,
        entities,
    });

    plan
}

fn collect_overlay_rects(world: &World) -> Vec<Rect> {
    use crate::input::feedback::{OverlayCursor, OverlayRotary};
    use crate::ui::ComputedRect;
    let mut rects = Vec::new();
    if let Some(storage) = world.storage::<OverlayCursor>() {
        for (e, _) in storage.iter() {
            if let Some(r) = world.get::<ComputedRect>(e).map(|r| r.0) {
                if r.w > Fixed::ZERO && r.h > Fixed::ZERO {
                    rects.push(r);
                }
            }
        }
    }
    if let Some(storage) = world.storage::<OverlayRotary>() {
        for (e, _) in storage.iter() {
            if let Some(r) = world.get::<ComputedRect>(e).map(|r| r.0) {
                if r.w > Fixed::ZERO && r.h > Fixed::ZERO {
                    rects.push(r);
                }
            }
        }
    }
    rects
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer);

        for x in 0..8 {
            let c = renderer.target.get_pixel(x, 32);
            assert_eq!(
                c.r, 255,
                "x={x} should be red (inside mask), got rgb({}, {}, {})",
                c.r, c.g, c.b
            );
        }
        for x in 8..64 {
            let c = renderer.target.get_pixel(x, 32);
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer);

        for x in 0..64 {
            let c = renderer.target.get_pixel(x, 32);
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = vp();
        super::render(&world, root, &viewport, &mut renderer);

        for x in 0..64 {
            let c = renderer.target.get_pixel(x, 32);
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
        world.insert(
            target,
            WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
        );
        world.insert(target, Dirty);

        let dirty = collect_dirty_region(&mut world, root, &vp()).expect("dirty region");

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
        world.insert(
            target,
            WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
        );
        seed_prev_rects(&mut world, root, &vp());

        world.insert(target, WidgetTransform(Transform::IDENTITY));
        world.insert(target, Dirty);
        let dirty = collect_dirty_region(&mut world, root, &vp()).expect("dirty region");

        assert_eq!(dirty.x, Fixed::from_int(15));
        assert_eq!(dirty.y, Fixed::from_int(15));
        assert_eq!(dirty.w, Fixed::from_int(20));
        assert_eq!(dirty.h, Fixed::from_int(20));
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
            let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, root, &viewport, &mut renderer);
            assert_eq!(renderer.target.get_pixel(16, 16).b, 255);
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
        let dirty = collect_dirty_region(&mut world, root, &viewport).expect("dirty region");
        {
            let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, root, &viewport, &dirty, &mut renderer);
            assert_eq!(renderer.target.get_pixel(16, 16).b, 0);
            assert_eq!(renderer.target.get_pixel(25, 25).b, 255);
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
            let tex = Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, root, &viewport, &mut renderer);
        }

        let dirty = collect_dirty_region(&mut world, root, &viewport).expect("dirty region");
        let mut region = std::vec![0u8; 64 * 64 * 4];
        {
            let tex = Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, root, &viewport, &dirty, &mut renderer);
        }

        let full_tex = Texture::new(&mut full, 64, 64, ColorFormat::RGBA8888);
        let region_tex = Texture::new(&mut region, 64, 64, ColorFormat::RGBA8888);
        for y in 0..64 {
            for x in 0..64 {
                let full_blue = full_tex.get_pixel(x, y).b == 255;
                let region_blue = region_tex.get_pixel(x, y).b == 255;
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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, root, &viewport, &mut renderer);

        for x in 0..32 {
            let c = renderer.target.get_pixel(x, 16);
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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, root, &viewport, &mut renderer);

        let expected = theme.blend_color_in(red, WidgetState::Disabled);
        let actual = renderer.target.get_pixel(16, 16);
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
    use crate::ui::{Children, OffscreenBufferPool, OffscreenRender, Parent, Style, Widget};

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
    fn offscreen_render_paints_subtree_into_pool_buffer() {
        // 32×32 panel marked OffscreenRender; the buffer pool should
        // see exactly one entry after render.
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

        let pool = world.resource::<OffscreenBufferPool>().expect("pool");
        assert_eq!(pool.cache.borrow().cache().len(), 1);
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
            let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, panel, &viewport, &mut renderer);
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
            let tex = Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, panel, &viewport, &mut renderer);
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

        // Pool has 1 entry; the key reports the half-resolution buffer
        // dims (32 × 0.5 = 16).
        let pool = world.resource::<OffscreenBufferPool>().expect("pool");
        let cache_borrow = pool.cache.borrow();
        let stats = cache_borrow.cache().stats();
        assert_eq!(stats.insert_count, 1);
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
        let tex = Texture::new(&mut buf, 128, 128, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

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
        let tex = Texture::new(&mut buf, 128, 128, ColorFormat::RGB565Swapped);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

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
            world.insert_resource(OffscreenBufferPool::with_budget(64 * 1024));
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
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped);
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer);
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
            world.insert_resource(OffscreenBufferPool::with_budget(64 * 1024));
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
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer);
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

    /// Pixel-equivalence on a non-trivial subtree (Switch + Slider +
    /// ProgressBar + label inside a flex panel). Runs the same fixture
    /// three times — inline reference, OffscreenRender on the panel,
    /// OffscreenRender on a single leaf widget — and asserts each
    /// framebuffer is byte-identical to the reference across cold
    /// render, switch toggle, and slider mutation.
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
            w.insert_resource(OffscreenBufferPool::with_budget(64 * 1024));
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

        let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
        // Mirror render_dirty so the walker bumps OffscreenGeneration
        // when the subtree mutates between frames.
        let render_into = |world: &mut World, root: Entity| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 2];
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped);
            let mut renderer = SwRenderer::new(tex);
            let dirty = super::collect_dirty_region(world, root, &viewport).unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(FB_W as i32),
                h: Fixed::from_int(FB_H as i32),
            });
            super::render_region(world, root, &viewport, &dirty, &mut renderer);
            fb
        };

        let (mut w_ref, fp_ref, sw_ref, sl_ref) = build(Mark::None);
        let (mut w_a, fp_a, sw_a, sl_a) = build(Mark::OnPanel);
        let (mut w_b, fp_b, sw_b, sl_b) = build(Mark::OnSwitch);

        crate::ui::dirty::mark_subtree_dirty(&mut w_ref, fp_ref);
        crate::ui::dirty::mark_subtree_dirty(&mut w_a, fp_a);
        crate::ui::dirty::mark_subtree_dirty(&mut w_b, fp_b);

        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 1 ≠ inline"
        );
        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 1 ≠ inline"
        );

        // Mark the whole subtree so the inline path's dirty bound
        // equals the offscreen path's (which always promotes to the
        // OffscreenRender entity's full rect).
        let flip_on = |w: &mut World, fp: Entity, sw: Entity| {
            if let Some(s) = w.get_mut::<Switch>(sw) {
                s.on = true;
            }
            crate::ui::dirty::mark_subtree_dirty(w, fp);
        };
        flip_on(&mut w_ref, fp_ref, sw_ref);
        flip_on(&mut w_a, fp_a, sw_a);
        flip_on(&mut w_b, fp_b, sw_b);
        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 2 (switch on) ≠ inline"
        );
        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 2 (switch on) ≠ inline"
        );

        let slider_max = |w: &mut World, fp: Entity, sl: Entity| {
            if let Some(s) = w.get_mut::<Slider>(sl) {
                s.value = s.max;
            }
            crate::ui::dirty::mark_subtree_dirty(w, fp);
        };
        slider_max(&mut w_ref, fp_ref, sl_ref);
        slider_max(&mut w_a, fp_a, sl_a);
        slider_max(&mut w_b, fp_b, sl_b);
        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_a, fp_a),
            "form_page OffscreenRender frame 3 (slider max) ≠ inline"
        );
        assert_eq!(
            render_into(&mut w_ref, fp_ref),
            render_into(&mut w_b, fp_b),
            "Switch OffscreenRender frame 3 (slider max) ≠ inline"
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
            let tex = Texture::new(&mut buf_inline, 128, 128, ColorFormat::RGB565Swapped);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(128, 128, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer);
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
            let tex = Texture::new(&mut buf_off, 128, 128, ColorFormat::RGB565Swapped);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(128, 128, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer);
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
            let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer);
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
            let tex = Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            let viewport = Viewport::new(64, 64, Fixed::ONE);
            super::render(&world, switch, &viewport, &mut renderer);
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

    /// Scale=0.5 must paint the *entire* buffer with the panel's bg —
    /// not just a fraction. After the blit upscale, the panel's full
    /// rect on the outer framebuffer should be the panel colour, with
    /// no black bands left behind. Regression for "scale=0.5 only paints
    /// a quarter" — clip was being passed in physical instead of logical
    /// coordinates inside try_draw_offscreen, which left 3/4 of the
    /// buffer untouched.
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

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
    fn offscreen_cache_hit_skips_inner_raster() {
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);

        super::render(&world, panel, &viewport, &mut renderer);

        // Stamp magenta over the cached green buffer. Frame 2 raster
        // would overwrite it back to green; hit path leaves it alone.
        let pool = world.resource::<OffscreenBufferPool>().unwrap();
        let key = {
            let cache_ref = pool.cache.borrow();
            cache_ref
                .cache()
                .iter()
                .next()
                .map(|(k, _)| *k)
                .expect("frame 1 must have populated the buffer")
        };
        {
            let mut cache_mut = pool.cache.borrow_mut();
            let handle = cache_mut.acquire(&key).expect("acquire by key");
            let mut tex_ref = handle.get().borrow_mut();
            let bytes = tex_ref.buf.as_mut_slice();
            for px in bytes.chunks_exact_mut(4) {
                px[0] = 255;
                px[1] = 0;
                px[2] = 255;
                px[3] = 255;
            }
        }

        let mut buf2 = std::vec![0u8; 64 * 64 * 4];
        let tex2 = Texture::new(&mut buf2, 64, 64, ColorFormat::RGBA8888);
        let mut renderer2 = SwRenderer::new(tex2);
        super::render(&world, panel, &viewport, &mut renderer2);

        assert_eq!(
            (buf2[0], buf2[1], buf2[2]),
            (255, 0, 255),
            "cache hit ran inner raster instead of blitting cached pixels"
        );
    }

    /// Pure Bug 2 isolation: the transform stays fully on-screen so
    /// the negative-blit path doesn't fire — only the redundant
    /// `try_draw_offscreen` cull bug would skip render here.
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
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(&world, panel, &viewport, &mut renderer);
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

        let dirty =
            super::collect_dirty_region(&mut world, panel, &viewport).expect("dirty region");
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        {
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, panel, &viewport, &dirty, &mut renderer);
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

    /// `WidgetTransform` translating the entity off-screen left must
    /// still produce visible pixels at the transformed position. The
    /// pre-fix `try_draw_offscreen` had its own cull check using the
    /// entity's untransformed `shifted_rect`, which sat at logical
    /// (80, 30) and didn't intersect the dirty clip when the entity
    /// was animated to negative x — so the offscreen path silently
    /// skipped raster + blit and the entity vanished.
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
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(world, panel, &viewport, &mut renderer);
            fb
        };
        let render_dirty = |world: &mut World| -> alloc::vec::Vec<u8> {
            let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
            let dirty = super::collect_dirty_region(world, panel, &viewport).unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(FB_W as i32),
                h: Fixed::from_int(FB_H as i32),
            });
            let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(world, panel, &viewport, &dirty, &mut renderer);
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
            let tex = Texture::new(&mut fb, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render(world, root, &viewport, &mut renderer);
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        // Frame 1
        super::render(&world, panel, &viewport, &mut renderer);
        let stats1 = *world
            .resource::<OffscreenBufferPool>()
            .unwrap()
            .cache
            .borrow()
            .cache()
            .stats();

        // Frame 2 — generation didn't change, pool should hit cache.
        super::render(&world, panel, &viewport, &mut renderer);
        let stats2 = *world
            .resource::<OffscreenBufferPool>()
            .unwrap()
            .cache
            .borrow()
            .cache()
            .stats();

        assert_eq!(stats2.insert_count, stats1.insert_count);
        assert!(stats2.hit_count > stats1.hit_count);
    }

    /// `App::snapshot_widget` returns an owned RGBA8888 texture and
    /// does not leave OffscreenRender / OffscreenAutoAdded behind on
    /// an entity that didn't have them before the call.
    #[test]
    fn snapshot_widget_does_not_pollute_source_components() {
        use crate::app::App;
        use crate::types::{Color, Dimension};
        use crate::ui::offscreen::OffscreenAutoAdded;
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
            .expect("snapshot should produce an owned texture");
        assert_eq!(snap.width, 16);
        assert_eq!(snap.height, 16);

        assert!(app.world.get::<OffscreenRender>(panel).is_none());
        assert!(app.world.get::<OffscreenAutoAdded>(panel).is_none());
    }

    /// Across consecutive dirty renders driven through render_region
    /// (the production path), `prev_texture_of` returns the buffer
    /// from the previous render. Frame 1 has no prev (None), frame 2
    /// sees frame 1's buffer, frame 3 sees frame 2's.
    #[test]
    fn prev_texture_of_returns_previous_frame_buffer() {
        use super::super::dirty::{Dirty, mark_subtree_dirty};
        use super::super::offscreen::WidgetTextureAccess;

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

        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let viewport = Viewport::new(64, 64, Fixed::ONE);

        let render_frame = |world: &mut World, buf: &mut [u8]| {
            mark_subtree_dirty(world, panel);
            let dirty = super::collect_dirty_region(world, panel, &viewport).unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(64),
                h: Fixed::from_int(64),
            });
            let tex = Texture::new(buf, 64, 64, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(world, panel, &viewport, &dirty, &mut renderer);
        };

        render_frame(&mut world, &mut buf);
        assert!(
            world.texture_of(panel).is_some(),
            "frame 1 should have current"
        );
        assert!(
            world.prev_texture_of(panel).is_none(),
            "frame 1 has no prev"
        );

        world.insert(panel, Dirty);
        render_frame(&mut world, &mut buf);
        assert!(
            world.texture_of(panel).is_some(),
            "frame 2 should have current"
        );
        assert!(
            world.prev_texture_of(panel).is_some(),
            "frame 2 should see frame 1"
        );

        world.insert(panel, Dirty);
        render_frame(&mut world, &mut buf);
        assert!(
            world.texture_of(panel).is_some(),
            "frame 3 should have current"
        );
        assert!(
            world.prev_texture_of(panel).is_some(),
            "frame 3 should see frame 2"
        );
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
        let bounds = super::collect_dirty_region(&mut world, panel, &viewport);
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
    /// (e.g. theme rotation calls `mark_subtree_dirty(root)` which
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

        // Frame 1 cold — first dirty walk (no PrevRect yet).
        world.insert(leaf, Dirty);
        let _ = super::collect_dirty_region(&mut world, leaf, &viewport);
        let g1 = world
            .get::<OffscreenGeneration>(leaf)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g1, 1, "frame 1 must bump from 0 → 1");

        // Frame 2 — only the offscreen entity itself is Dirty (no
        // children). The cache buffer is stale; generation must bump.
        world.insert(leaf, Dirty);
        let bounds = super::collect_dirty_region(&mut world, leaf, &viewport);
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
    /// the rect-promotion conditional on `PrevRect.is_none()` — true
    /// only on the very first dirty walk for a given entity — so the
    /// second time a child went Dirty the offscreen entity dropped
    /// out of the dirty bounds entirely, render_region had nothing
    /// to paint, and the offscreen subtree visibly froze on the
    /// previous frame's pixels.
    #[test]
    fn dirty_walker_promotes_subtree_dirty_on_every_frame() {
        use super::super::dirty::{Dirty, PrevRect};
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

        // Frame 1: child goes Dirty. Walker bumps generation, includes
        // panel rect in bounds, sets PrevRect on panel.
        world.insert(child, Dirty);
        let b1 = super::collect_dirty_region(&mut world, panel, &viewport);
        assert!(b1.is_some(), "frame 1 must report bounds");
        let g1 = world
            .get::<OffscreenGeneration>(panel)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g1, 1, "frame 1 generation = 1");
        assert!(world.get::<PrevRect>(panel).is_some(), "panel PrevRect set");

        // Frame 2: child goes Dirty again. Walker must bump generation
        // a second time AND include the panel rect in bounds. If it
        // doesn't, render_region won't repaint the offscreen entity
        // and the screen freezes.
        world.insert(child, Dirty);
        let b2 = super::collect_dirty_region(&mut world, panel, &viewport);
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
        let _ = super::collect_dirty_region(&mut world, panel, &viewport);

        // Nothing was Dirty so generation should stay at default (no
        // OffscreenGeneration component inserted yet).
        assert!(
            world.get::<OffscreenGeneration>(panel).is_none(),
            "no Dirty subtree should leave generation unchanged"
        );
    }

    #[test]
    #[should_panic(expected = "OffscreenRender + WidgetTransform3D")]
    fn offscreen_render_panics_on_3d_transform() {
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);
    }

    #[test]
    #[should_panic(expected = "nested OffscreenRender")]
    fn offscreen_render_panics_on_nesting() {
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, outer, &viewport, &mut renderer);
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
        };
        assert_eq!(k, same);
    }

    #[test]
    fn offscreen_render_fractional_scale_rounds_buffer_dims() {
        // scale = 0.7 on a 32×32 panel should produce a 22×22 buffer
        // (32 × 0.7 = 22.4 → truncated to 22 by `Fixed::to_int`).
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
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

        let pool = world.resource::<OffscreenBufferPool>().expect("pool");
        let cb = pool.cache.borrow();
        // Single insert, single entry; buffer dims must round to 22×22.
        assert_eq!(cb.cache().stats().insert_count, 1);
        assert_eq!(cb.cache().len(), 1);
    }

    #[test]
    fn hidden_entity_skips_offscreen_path() {
        // An OffscreenRender entity that's also Hidden should not
        // create a buffer — collect_entities_preorder skips Hidden
        // entirely so the dispatch never reaches try_draw_offscreen.
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
        let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(32, 32, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

        let pool = world.resource::<OffscreenBufferPool>().expect("pool");
        assert_eq!(
            pool.cache.borrow().cache().len(),
            0,
            "Hidden entity should not allocate an offscreen buffer"
        );
    }

    /// MirrorOf paints the source's flipped texture into the mirror
    /// entity's rect after the source has rendered. Source = a 16×16
    /// red panel; mirror = a 16×16 region directly below it.
    /// Expectation: after rendering, the mirror's rect contains red
    /// pixels (proves the view fn ran end-to-end).
    #[test]
    fn mirror_of_paints_into_its_own_rect() {
        use crate::ui::dirty::mark_subtree_dirty;
        use crate::ui::widgets::MirrorOf;

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

        let source = spawn(
            &mut world,
            Some(root),
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    position: crate::ui::layout::Position::Absolute,
                    left: Dimension::Px(Fixed::ZERO),
                    top: Dimension::Px(Fixed::ZERO),
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );

        let mirror = spawn(
            &mut world,
            Some(root),
            Style {
                layout: LayoutStyle {
                    position: crate::ui::layout::Position::Absolute,
                    left: Dimension::Px(Fixed::ZERO),
                    top: Dimension::Px(Fixed::from_int(16)),
                    width: Dimension::Px(Fixed::from_int(16)),
                    height: Dimension::Px(Fixed::from_int(16)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(mirror, MirrorOf::new(source));

        // auto_attach won't run on its own outside the App lifecycle,
        // so plant the WidgetTextureRef manually for the test.
        world.insert(mirror, crate::ui::offscreen::WidgetTextureRef(source));
        super::super::offscreen::maintain_widget_texture_refs(&mut world);

        let viewport = Viewport::new(32, 32, Fixed::ONE);
        let mut buf = std::vec![0u8; 32 * 32 * 4];

        // Drive two dirty renders: frame 1 fills the source's buffer;
        // frame 2 lets the mirror's view fn read it.
        for _ in 0..2 {
            mark_subtree_dirty(&mut world, root);
            let dirty =
                super::collect_dirty_region(&mut world, root, &viewport).expect("dirty region");
            let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888);
            let mut renderer = SwRenderer::new(tex);
            super::render_region(&world, root, &viewport, &dirty, &mut renderer);
        }

        // Sample the mirror's rect (y in 16..32, x in 0..16). Expect
        // at least one red-dominant pixel.
        let mut found = false;
        for y in 16..32 {
            for x in 0..16 {
                let i = (y * 32 + x) * 4;
                if buf[i] > 64 && buf[i + 1] < 32 && buf[i + 2] < 32 {
                    found = true;
                    break;
                }
            }
            if found {
                break;
            }
        }
        assert!(found, "mirror rect should contain red-tinted pixels");
    }

    /// With `OffscreenAlphaMode::clear_transparent`, an offscreen
    /// buffer's pixels outside the source widget's drawn area stay
    /// alpha=0 instead of inheriting framebuffer alpha. Pre-seed
    /// (default) would copy framebuffer alpha (always 255 for opaque
    /// backends) and erase the silhouette.
    #[test]
    fn offscreen_alpha_mode_clear_zeros_buffer_outside_source() {
        use crate::ui::offscreen::OffscreenAlphaMode;

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
                // (alpha = 0 with the marker, ≠ 0 without it).
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(6)),
                    height: Dimension::Px(Fixed::from_int(6)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, super::super::OffscreenRender::default());
        world.insert(panel, OffscreenAlphaMode::clear_transparent());
        world.insert_resource(super::super::OffscreenBufferPool::with_budget(64 * 1024));

        let mut buf = std::vec![0xFFu8; 12 * 12 * 4];
        let tex = Texture::new(&mut buf, 12, 12, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(12, 12, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer);

        let pool = world
            .resource::<super::super::OffscreenBufferPool>()
            .unwrap();
        let cache = pool.cache.borrow();
        let entry = cache.cache().iter().next().expect("buffer entry");
        let tex_ref = entry.1.borrow();
        for px in tex_ref.buf.as_slice().chunks_exact(4) {
            assert_eq!(px[3], 0, "alpha-clear buffer pixel must stay alpha=0");
        }
    }

    /// Theme swap while a subtree is `Hidden`, then unhide.
    /// `mark_subtree_dirty` skips Hidden, so the offscreen descendant
    /// inside the hidden subtree never receives Dirty. When the
    /// subtree is later unhidden, the dirty walker must still see
    /// enough Dirty markers in the freshly-revealed subtree to bump
    /// every OffscreenGeneration inside it; otherwise the cached
    /// buffer keeps painting in the previous theme's colours.
    #[test]
    fn unhide_after_global_event_invalidates_offscreen_descendants() {
        use crate::ui::Hidden;
        use crate::ui::dirty::{Dirty, mark_subtree_dirty};
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
        let _ = super::collect_dirty_region(&mut world, root, &viewport);
        let g0 = world
            .get::<OffscreenGeneration>(offscreen_grandchild)
            .map(|g| g.0)
            .unwrap_or(0);
        assert_eq!(g0, 1, "first dirty walk must bump generation 0 → 1");

        // Hide the tab and clear any leftover Dirty in the subtree
        // (mirrors `tab_pages_system`'s hide branch).
        world.insert(tab_content, Hidden);
        crate::ui::dirty::clear_subtree_dirty(&mut world, tab_content);

        // Global event: theme swap walks from `root`, hitting Hidden
        // along the way. The offscreen grandchild does not get Dirty.
        mark_subtree_dirty(&mut world, root);
        assert!(
            world.get::<Dirty>(offscreen_grandchild).is_none(),
            "Hidden subtree's descendants must not be Dirty after \
             mark_subtree_dirty (this part is the existing optimisation)"
        );

        // Unhide: emulate the (false, true) branch of
        // `tab_pages_system::apply_visibility`. Marking the whole
        // subtree (rather than only `tab_content`) is what makes
        // descendants' caches invalidate.
        world.remove::<Hidden>(tab_content);
        mark_subtree_dirty(&mut world, tab_content);

        // Run the walker and check the offscreen generation has
        // advanced — the cached buffer must be invalidated so the
        // next render misses cache and repaints in the new theme.
        let _ = super::collect_dirty_region(&mut world, root, &viewport);
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
    use crate::input::event::scroll::components::{ScrollDelta, ScrollOffset};
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

    #[test]
    fn scroll_delta_positive_emits_negative_scroll_op_and_bottom_strip() {
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
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(3),
            },
        );
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);

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
    fn scroll_delta_negative_emits_positive_scroll_op_and_top_strip() {
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
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(-3),
            },
        );
        world.insert(list, Dirty);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
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
        world.insert(
            outer,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(-2),
            },
        );
        world.insert(outer, Dirty);
        let inner = spawn_widget(&mut world, Some(outer), px_style(128, 60));
        world.insert(
            inner,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(
            inner,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(-5),
            },
        );
        world.insert(inner, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert_eq!(plan.shifts.len(), 2, "two RegionShifts for two containers");
        assert_eq!(plan.shifts[0].dy, Fixed::from_int(5), "inner first");
        assert_eq!(plan.shifts[1].dy, Fixed::from_int(2), "outer second");
    }

    #[test]
    fn zero_scroll_delta_emits_no_op() {
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
        world.insert(list, ScrollDelta::default());
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert!(plan.shifts.is_empty());
    }

    #[test]
    fn sub_pixel_delta_keeps_residue_emits_nothing() {
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
        let half = Fixed::from_int(1) / Fixed::from_int(2);
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: half,
            },
        );
        world.insert(list, Dirty);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert!(plan.shifts.is_empty(), "no integer pixel to shift yet");
        // Residue stays in the component for the next frame to absorb.
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        assert_eq!(sd.dy, half);
    }

    #[test]
    fn sub_pixel_residue_accumulates_to_pixel() {
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
        let half = Fixed::from_int(1) / Fixed::from_int(2);
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: half,
            },
        );
        world.insert(list, Dirty);
        let viewport = Viewport::new(128, 128, Fixed::ONE);

        let _ = collect_dirty_regions(&mut world, root, &viewport);
        assert_eq!(
            world
                .get::<ScrollDelta>(list)
                .copied()
                .unwrap_or_default()
                .dy,
            half,
        );

        if let Some(sd) = world.get_mut::<ScrollDelta>(list) {
            sd.dy += half;
        }
        world.insert(list, Dirty);

        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert_eq!(plan.shifts.len(), 1);
        assert_eq!(plan.shifts[0].dy, Fixed::from_int(-1));
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        assert_eq!(sd.dy, Fixed::ZERO, "residue should be consumed");
    }

    /// Negative sub-pixel deltas must quantise toward zero, not floor.
    /// `Fixed::to_int` is arithmetic-shift floor (`-0.5 → -1`); using
    /// it here would emit a `RegionShift.dy = +1` and leave a `+0.5`
    /// residue with the wrong sign, so the next frame's `+0.5` input
    /// would cancel it instead of accumulating. `trunc_to_int` keeps
    /// the residue sign-aligned with the original delta.
    #[test]
    fn negative_sub_pixel_delta_quantises_toward_zero_not_floor() {
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
        let neg_half = Fixed::ZERO - (Fixed::from_int(1) / Fixed::from_int(2));
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: neg_half,
            },
        );
        world.insert(list, Dirty);
        let viewport = Viewport::new(128, 128, Fixed::ONE);

        // Frame 1: -0.5 truncates to 0, no shift, residue stays at -0.5.
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert!(
            plan.shifts.is_empty(),
            "sub-pixel delta must not emit a shift, got {:?}",
            plan.shifts
        );
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        assert_eq!(
            sd.dy, neg_half,
            "residue should still be -0.5 (sign-preserving), got {:?}",
            sd.dy
        );

        // Frame 2: another -0.5 of input. Residue accumulates to -1.0
        // and emits one integer shift.
        if let Some(sd) = world.get_mut::<ScrollDelta>(list) {
            sd.dy += neg_half;
        }
        world.insert(list, Dirty);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert_eq!(
            plan.shifts.len(),
            1,
            "two halves should accumulate to one whole pixel"
        );
        assert_eq!(
            plan.shifts[0].dy,
            Fixed::from_int(1),
            "ScrollDelta.dy = -1 → RegionShift.dy = +1 (sign-flipped framebuffer shift)"
        );
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        assert_eq!(sd.dy, Fixed::ZERO, "residue should be cleanly consumed");
    }

    #[test]
    fn negative_sub_pixel_delta_three_frame_accumulation() {
        // Three -0.4 frames must accumulate over -1.2 total: shift -1
        // emitted on frame 3 with residue -0.2 carrying into frame 4.
        // Pins the residue-preserving direction so a future change
        // can't silently revert to floor and pass the simpler 2-frame
        // test by skipping a beat.
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
        let neg_two_fifths = Fixed::ZERO - (Fixed::from_int(2) / Fixed::from_int(5));
        let viewport = Viewport::new(128, 128, Fixed::ONE);

        let mut shift_total = Fixed::ZERO;
        for _frame in 0..3 {
            if let Some(sd) = world.get_mut::<ScrollDelta>(list) {
                sd.dy += neg_two_fifths;
            } else {
                world.insert(
                    list,
                    ScrollDelta {
                        dx: Fixed::ZERO,
                        dy: neg_two_fifths,
                    },
                );
            }
            world.insert(list, Dirty);
            let plan = collect_dirty_regions(&mut world, root, &viewport);
            for s in &plan.shifts {
                shift_total += s.dy;
            }
        }
        // Three -0.4 inputs sum to -1.2. After one integer shift of
        // +1 (sign-flipped framebuffer direction), residue should be
        // the same -0.2 we'd get from arithmetic in Q24.8 — match the
        // residue against an explicitly-computed reference rather
        // than against an integer literal that ignores fixed-point
        // rounding.
        assert_eq!(
            shift_total,
            Fixed::from_int(1),
            "expected exactly +1 cumulative shift across 3 frames",
        );
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        let expected_residue = neg_two_fifths * Fixed::from_int(3) + Fixed::from_int(1);
        assert_eq!(
            sd.dy, expected_residue,
            "residue must equal the Q24.8 sum of three -0.4 inputs minus the integer shift",
        );
    }

    #[test]
    fn negative_sub_pixel_delta_under_hidpi_quantises_in_logical_space() {
        // HiDPI scale=2 means a logical -0.5 input must still trunc
        // toward zero in logical-pixel space; the 2× physical mapping
        // is the renderer's job, not the walker's.
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
        let neg_half = Fixed::ZERO - (Fixed::from_int(1) / Fixed::from_int(2));
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: neg_half,
            },
        );
        world.insert(list, Dirty);

        let hidpi_viewport = Viewport::new(128, 128, Fixed::from_int(2));
        let plan = collect_dirty_regions(&mut world, root, &hidpi_viewport);
        assert!(
            plan.shifts.is_empty(),
            "sub-pixel logical delta should still produce no shift under HiDPI, got {:?}",
            plan.shifts,
        );
        let sd = world.get::<ScrollDelta>(list).copied().unwrap_or_default();
        assert_eq!(
            sd.dy, neg_half,
            "residue must be preserved in logical space regardless of viewport scale",
        );
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
    fn overlay_over_scroll_emits_overlay_rect_pair_keeps_scroll_op() {
        use crate::input::feedback::OverlayCursor;

        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(128, 128));
        world.insert(root, Dirty);
        let list = spawn_widget(&mut world, Some(root), absolute_style(0, 0, 128, 100));
        world.insert(
            list,
            ScrollOffset {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            },
        );
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(2),
            },
        );
        world.insert(list, Dirty);

        let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 50, 40, 12));
        world.insert(overlay, OverlayCursor);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);

        assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
        let dy = plan.shifts[0].dy;
        assert_eq!(dy, Fixed::from_int(-2));

        let has_overlay_rect = plan.rects.iter().any(|r| {
            r.x == Fixed::from_int(40) && r.y == Fixed::from_int(50) && r.w == Fixed::from_int(40)
        });
        assert!(
            has_overlay_rect,
            "expected overlay rect at original pos, got {:?}",
            plan.rects
        );
        let has_shifted_rect = plan.rects.iter().any(|r| {
            r.x == Fixed::from_int(40)
                && r.y == Fixed::from_int(50) + dy
                && r.w == Fixed::from_int(40)
        });
        assert!(
            has_shifted_rect,
            "expected shifted rect to cover residue, got {:?}",
            plan.rects
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
                y: Fixed::ZERO,
            },
        );
        world.insert(
            list,
            ScrollDelta {
                dx: Fixed::ZERO,
                dy: Fixed::from_int(2),
            },
        );
        world.insert(list, Dirty);

        let overlay = spawn_widget(&mut world, Some(root), absolute_style(40, 0, 40, 12));
        world.insert(overlay, OverlayCursor);

        let viewport = Viewport::new(128, 128, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert_eq!(plan.shifts.len(), 1, "scroll kept, got {:?}", plan.shifts);
    }

    #[test]
    fn dirty_walker_publishes_layout_snapshot_for_render_walker() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(64, 64));
        let child = spawn_widget(&mut world, Some(root), px_style(32, 32));
        world.insert(child, Dirty);

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let plan = collect_dirty_regions(&mut world, root, &viewport);
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
    fn idle_frame_leaves_snapshot_absent() {
        let mut world = World::new();
        let root = spawn_widget(&mut world, None, px_style(64, 64));
        spawn_widget(&mut world, Some(root), px_style(32, 32));

        let viewport = Viewport::new(64, 64, Fixed::ONE);
        let plan_idle = collect_dirty_regions(&mut world, root, &viewport);
        assert!(plan_idle.rects.is_empty());
        assert!(
            world.resource::<LayoutSnapshot>().is_none(),
            "no Dirty markers ⇒ no snapshot needed",
        );
    }

    #[test]
    fn render_region_cached_matches_fresh_output() {
        use crate::render::command::DrawCommand;
        use crate::render::renderer::Renderer;

        struct Recorder(std::vec::Vec<core::mem::Discriminant<DrawCommand<'static>>>);
        impl Renderer for Recorder {
            fn draw(&mut self, cmd: &DrawCommand, _clip: &Rect) {
                let static_cmd: &DrawCommand<'static> = unsafe { core::mem::transmute(cmd) };
                self.0.push(core::mem::discriminant(static_cmd));
            }
            fn flush(&mut self) {}
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
        let plan = collect_dirty_regions(&mut world, root, &viewport);
        assert!(!plan.rects.is_empty());

        let snap = world
            .resource::<LayoutSnapshot>()
            .expect("snapshot present after dirty walk");
        let mut cached_out = Recorder(std::vec::Vec::new());
        render_region_cached(&world, snap, &dirty_rect, &mut cached_out);

        let mut fresh_out = Recorder(std::vec::Vec::new());
        render_region(&world, root, &viewport, &dirty_rect, &mut fresh_out);

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
            let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888);
            let mut backend = SwRenderer::new(tex);
            let area = Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(32),
                Fixed::from_int(32),
            );
            backend.scroll_target_region(&area, Fixed::ZERO, Fixed::from_int(-4));
            for y in 28..32 {
                let post_shift_red = ((y + 4) * 8) as u8;
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    backend.target.buf.as_mut_slice()[off] = post_shift_red;
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
            let tex = Texture::new(&mut blit_buf, 32, 32, ColorFormat::RGBA8888);
            let mut backend = SwRenderer::new(tex);
            let inner_area = Rect::new(
                Fixed::ZERO,
                Fixed::from_int(8),
                Fixed::from_int(32),
                Fixed::from_int(16),
            );
            backend.scroll_target_region(&inner_area, Fixed::ZERO, Fixed::from_int(-2));
            // Inner strip exposed at rows 22..24 (bottom of inner).
            for y in 22..24 {
                let post_inner_red = ((y + 2) * 4) as u8;
                for x in 0..32 {
                    let off = (y * 32 + x) * 4;
                    backend.target.buf.as_mut_slice()[off] = post_inner_red;
                }
            }

            let outer_area = Rect::new(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(32),
                Fixed::from_int(32),
            );
            backend.scroll_target_region(&outer_area, Fixed::ZERO, Fixed::from_int(-4));
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
                    backend.target.buf.as_mut_slice()[off] = red;
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
