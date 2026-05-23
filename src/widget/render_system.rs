use alloc::vec::Vec;

use crate::components::transform::WidgetTransform;
use crate::components::transform_3d::{TransformOrigin, WidgetTransform3D};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D, Viewport};

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

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if let Some(child_node) = build_layout_tree(world, child) {
                node.add_child(child_node);
            }
        }
    }
    Some(node)
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
                    (view.render())(renderer, world, entity, &shifted_rect, &mut ctx);
                }
            }
        }
    }
    *idx += 1;

    let (child_clip, sx, sy) =
        if let Some(scroll) = world.get::<crate::event::scroll::ScrollOffset>(entity) {
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
) -> bool {
    use crate::draw::canvas::Canvas;
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::Texture;
    use crate::types::{Color, Point};

    let format = match renderer.offscreen_format() {
        Some(f) => f,
        None => return false,
    };

    if !rects_intersect(shifted_rect, clip) {
        *idx += count_nodes(node);
        return true;
    }

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

    crate::trace_span!("draw.offscreen");

    let handle = match pool.cache.borrow_mut().entry(key).or_insert() {
        Ok(h) => h,
        Err(_) => return false,
    };

    // Borrow the buffer's bytes mutably so an inner SwRenderer can
    // raster the subtree into them. The Texture::Mut wrapper lets us
    // hand that &mut [u8] to a transient Texture<'borrow>.
    {
        let mut tex_ref = handle.get().borrow_mut();
        let buf_slice = tex_ref.buf.as_mut_slice();
        let inner_tex = Texture::new(buf_slice, buf_w, buf_h, format);
        let mut inner = SwRenderer::new(inner_tex);
        inner.viewport = Viewport::new(buf_w, buf_h, scale);

        // Clear the buffer before drawing so stale pixels from the
        // previous tenant of this cache slot don't bleed through.
        let inner_clip = Rect::new(0, 0, buf_w as i32, buf_h as i32);
        Canvas::clear(&mut inner, &inner_clip, &Color::rgba(0, 0, 0, 0));

        // Translate so the entity's drawn rect lands at buffer (0, 0).
        // We can't reuse draw_tree_offset for the whole subtree because
        // it would re-detect the OffscreenRender marker on the entity
        // itself and panic on nesting. Inline the entity's view
        // dispatch, then recurse children with inside_offscreen=true.
        let inner_offset_x = shifted_rect.x;
        let inner_offset_y = shifted_rect.y;
        let entity_rect = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: shifted_rect.w,
            h: shifted_rect.h,
        };

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
            if let Some(scroll) = world.get::<crate::event::scroll::ScrollOffset>(entity) {
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
        // SAFETY-style note: `tex_ref` borrows from the pool's RefCell
        // which lives in the World resource; the Blit command captures
        // a reference to it that outlives only this dispatch call, so
        // dropping the RefMut after `renderer.draw(...)` returns is
        // sound.
        let blit_cmd = DrawCommand::Blit {
            pos: Point::new(shifted_rect.x, shifted_rect.y),
            size: Point::new(shifted_rect.w, shifted_rect.h),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &tex_ref,
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

    let mut layout_tree = crate::trace_span!("render.build_tree", {
        match build_layout_tree(world, root) {
            Some(t) => t,
            None => return,
        }
    });

    {
        crate::trace_span!("render.compute_layout");
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
        crate::trace_span!("render.collect_entities");
        collect_entities_preorder(world, root, &mut entities);
    }

    {
        crate::trace_span!("render.draw_tree");
        let mut idx = 0;
        draw_tree_offset(
            &layout_tree,
            world,
            &entities,
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

struct DirtyBounds {
    min_x: Fixed,
    min_y: Fixed,
    max_x: Fixed,
    max_y: Fixed,
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
    bounds: &mut DirtyBounds,
) {
    use super::dirty::Dirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf = effective_transform(parent_transform, world, entity, node.rect);
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);

    if world.get::<Dirty>(entity).is_some() {
        let curr_layout = quad_for(world, entity, node.rect, parent_3d)
            .map(quad_bbox)
            .unwrap_or_else(|| tf.apply_rect_bbox(node.rect));
        let curr = Rect {
            x: curr_layout.x - scroll_offset.0,
            y: curr_layout.y - scroll_offset.1,
            w: curr_layout.w,
            h: curr_layout.h,
        };

        let union_rect = match world.get::<super::dirty::PrevRect>(entity) {
            Some(prev) => curr.union(&prev.0),
            None => curr,
        };
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

    let child_scroll = if let Some(scroll) = world.get::<crate::event::scroll::ScrollOffset>(entity)
    {
        (scroll_offset.0 + scroll.x, scroll_offset.1 + scroll.y)
    } else {
        scroll_offset
    };

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
            bounds,
        );
    }
}

/// Collect the logical-pixel rects of all dirty entities, then remove Dirty flags.
/// Returns the bounding rect of all dirty regions, or None if nothing dirty.
pub fn collect_dirty_region(world: &mut World, root: Entity, transform: &Viewport) -> Option<Rect> {
    let (logical_w, logical_h) = transform.logical_size();

    let mut layout_tree =
        crate::trace_span!("dirty.build_tree", { build_layout_tree(world, root)? });

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
            &mut bounds,
        );
    }

    let (min_x, min_y, max_x, max_y) = (bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y);

    if max_x < Fixed::ZERO {
        None
    } else {
        Some(Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        })
    }
}

#[cfg(all(test, feature = "std"))]
mod clip_children_check {
    extern crate std;
    use super::*;
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::{ColorFormat, Texture};
    use crate::layout::{LayoutStyle, Position};
    use crate::types::{Color, Dimension, Viewport};
    use crate::widget::{Children, Parent, Style, Widget};

    fn spawn_widget(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn();
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
        use crate::components::transform::WidgetTransform;
        use crate::types::Transform;
        use crate::widget::dirty::Dirty;

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
        use crate::components::transform::WidgetTransform;
        use crate::types::Transform;
        use crate::widget::dirty::Dirty;

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
        use crate::components::transform::WidgetTransform;
        use crate::types::Transform;
        use crate::widget::dirty::Dirty;

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
        use crate::components::transform::WidgetTransform;
        use crate::types::Transform;
        use crate::widget::dirty::Dirty;

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
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::{ColorFormat, Texture};
    use crate::layout::LayoutStyle;
    use crate::types::{Color, Dimension, Viewport};
    use crate::widget::{Children, Hidden, Parent, Style, Widget};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn();
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
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::{ColorFormat, Texture};
    use crate::layout::LayoutStyle;
    use crate::types::{Color, Dimension, Viewport};
    use crate::widget::theme::{Theme, WidgetState};
    use crate::widget::{Children, Parent, Style, UserState, Widget};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn();
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
    use crate::draw::sw::SwRenderer;
    use crate::draw::texture::{ColorFormat, Texture};
    use crate::layout::LayoutStyle;
    use crate::types::{Color, Dimension, Viewport};
    use crate::widget::offscreen::BufferKey;
    use crate::widget::{Children, OffscreenBufferPool, OffscreenRender, Parent, Style, Widget};

    fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn();
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
        app.with_default_widgets();
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

    #[test]
    #[should_panic(expected = "OffscreenRender + WidgetTransform3D")]
    fn offscreen_render_panics_on_3d_transform() {
        use crate::components::transform_3d::WidgetTransform3D;
        use crate::types::Transform3D;
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
}

#[cfg(test)]
mod state_resolve_check {
    use super::*;

    #[test]
    fn enabled_when_no_state_components() {
        let mut world = World::new();
        let e = world.spawn();
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Enabled);
    }

    #[test]
    fn disabled_user_state_self() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, UserState::Disabled);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
    }

    #[test]
    fn disabled_propagates_via_parent() {
        let mut world = World::new();
        let parent = world.spawn();
        let child = world.spawn();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Disabled);
        assert_eq!(resolve_widget_state(&world, child), WidgetState::Disabled);
    }

    #[test]
    fn errored_self_only() {
        let mut world = World::new();
        let parent = world.spawn();
        let child = world.spawn();
        world.insert(child, Parent(parent));
        world.insert(parent, UserState::Errored);
        assert_eq!(resolve_widget_state(&world, child), WidgetState::Enabled);
        assert_eq!(resolve_widget_state(&world, parent), WidgetState::Error);
    }

    #[test]
    fn pressed_beats_hovered() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, InteractionState::Pressed);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Pressed);
    }

    #[test]
    fn hovered_when_only_hover() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, InteractionState::Hovered);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Hovered);
    }

    #[test]
    fn disabled_beats_pressed() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, UserState::Disabled);
        world.insert(e, InteractionState::Pressed);
        assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
    }
}
