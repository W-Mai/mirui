use alloc::vec::Vec;

use crate::components::image::Image;
use crate::components::progress_bar::ProgressBar;
use crate::components::tabbar::TabBar;
use crate::components::text_input::{Placeholder, TextInput};
use crate::components::transform::WidgetTransform;
use crate::components::transform_3d::{TransformOrigin, WidgetTransform3D};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, Viewport};

use super::view::{ViewCtx, ViewRegistry};
use super::{Children, Hidden, Style, Text, Widget};

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
    parent_3d: &Transform3D,
) {
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);
    let effective_rect = quad_for(world, entity, node.rect, parent_3d)
        .map(quad_bbox)
        .unwrap_or(node.rect);
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
        seed_prev_rect_walk(child, world, entities, idx, &tf_3d);
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

/// Render a TextInput's text/placeholder/cursor + focus border. Called
/// from both draw_tree (rect) and draw_tree_offset (shifted_rect).
#[allow(clippy::too_many_arguments)]
fn draw_text_input(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    ti: &TextInput,
    rect: &Rect,
    tf: &Transform,
    quad: Option<[Point; 4]>,
    clip: &Rect,
) {
    // Focus border override (only when focused — non-focused uses the
    // standard Style border which `draw_tree` already drew).
    if ti.focused {
        renderer.draw(
            &DrawCommand::Border {
                area: *rect,
                transform: *tf,
                quad,
                color: ti.focus_border_color,
                width: Fixed::ONE,
                radius: Fixed::ZERO,
                opa: 255,
            },
            clip,
        );
    }

    let text_x = rect.x + Fixed::from_int(2);
    let text_y = rect.y + Fixed::from_int(2);
    if ti.len == 0 {
        if let Some(ph) = world.get::<Placeholder>(entity) {
            renderer.draw(
                &DrawCommand::Label {
                    pos: Point {
                        x: text_x,
                        y: text_y,
                    },
                    transform: *tf,
                    text: ph.0.as_bytes(),
                    color: ti.placeholder_color,
                    opa: 255,
                },
                clip,
            );
        }
    } else {
        renderer.draw(
            &DrawCommand::Label {
                pos: Point {
                    x: text_x,
                    y: text_y,
                },
                transform: *tf,
                text: &ti.buffer[..ti.len as usize],
                color: ti.text_color,
                opa: 255,
            },
            clip,
        );
    }

    if ti.focused {
        let blink_on = world
            .resource::<crate::event::widget_input::CursorBlinkPhase>()
            .map(|p| p.0)
            .unwrap_or(true);
        if blink_on {
            // 8×8 fixed font: each glyph advances exactly CHAR_W.
            let cursor_x = text_x + Fixed::from_int(ti.cursor as i32 * 8);
            renderer.draw(
                &DrawCommand::Fill {
                    area: Rect {
                        x: cursor_x,
                        y: text_y,
                        w: Fixed::ONE,
                        h: Fixed::from_int(8),
                    },
                    transform: *tf,
                    quad,
                    color: ti.cursor_color,
                    radius: Fixed::ZERO,
                    opa: 255,
                },
                clip,
            );
        }
    }
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
fn draw_tree(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    parent_transform: &Transform,
    parent_transform_3d: &Transform3D,
) {
    let entity = if *idx < entities.len() {
        entities[*idx]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };
    let tf = effective_transform(parent_transform, world, entity, node.rect);
    let tf_3d = accumulate_3d(parent_transform_3d, world, entity, node.rect);
    let quad = quad_for(world, entity, node.rect, parent_transform_3d);

    let cull_rect = quad.map(quad_bbox).unwrap_or(node.rect);
    if !rects_intersect(&cull_rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    if *idx < entities.len() {
        if let Some(style) = world.get::<Style>(entity) {
            let mut ctx = ViewCtx {
                style,
                transform: tf,
                quad,
                clip,
                bg_handled: false,
            };
            if let Some(registry) = world.resource::<ViewRegistry>() {
                for view in registry.iter() {
                    (view.render)(renderer, world, entity, &node.rect, &mut ctx);
                }
            }

            if let Some(pb) = world.get::<ProgressBar>(entity) {
                renderer.draw(
                    &DrawCommand::Fill {
                        area: node.rect,
                        transform: tf,
                        quad,
                        color: pb.track_color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
                let fill_w = Fixed::from_f32(node.rect.w.to_f32() * pb.value.clamp(0.0, 1.0));
                if fill_w > Fixed::ZERO {
                    renderer.draw(
                        &DrawCommand::Fill {
                            area: Rect {
                                x: node.rect.x,
                                y: node.rect.y,
                                w: fill_w,
                                h: node.rect.h,
                            },
                            transform: tf,
                            quad,
                            color: pb.fill_color,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            if let Some(tb) = world.get::<TabBar>(entity) {
                if tb.count > 0 {
                    let tab_w = node.rect.w / Fixed::from_int(tb.count as i32);
                    let indicator_x = node.rect.x + tb.indicator_offset * tab_w;
                    let indicator_y = node.rect.y + node.rect.h - tb.indicator_height;
                    renderer.draw(
                        &DrawCommand::Fill {
                            area: Rect {
                                x: indicator_x,
                                y: indicator_y,
                                w: tab_w,
                                h: tb.indicator_height,
                            },
                            transform: tf,
                            quad,
                            color: tb.indicator_color,
                            radius: Fixed::ZERO,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            if let Some(ti) = world.get::<TextInput>(entity) {
                draw_text_input(renderer, world, entity, ti, &node.rect, &tf, quad, clip);
            }
            if let Some(img) = world.get::<Image>(entity) {
                renderer.draw(
                    &DrawCommand::Blit {
                        pos: Point {
                            x: node.rect.x,
                            y: node.rect.y,
                        },
                        size: Point {
                            x: node.rect.w,
                            y: node.rect.h,
                        },
                        transform: tf,
                        quad,
                        texture: img.texture,
                    },
                    clip,
                );
            }
            if let Some(text) = world.get::<Text>(entity) {
                let color = style.text_color.unwrap_or(Color::rgb(255, 255, 255));
                renderer.draw(
                    &DrawCommand::Label {
                        pos: Point {
                            x: node.rect.x + Fixed::from_int(2),
                            y: node.rect.y + Fixed::from_int(2),
                        },
                        transform: tf,
                        text: &text.0,
                        color,
                        opa: 255,
                    },
                    clip,
                );
            }
        }
    }
    *idx += 1;

    let (child_clip, scroll_x, scroll_y) =
        if let Some(scroll) = world.get::<crate::event::scroll::ScrollOffset>(entity) {
            (intersect_with_self(clip, &node.rect), scroll.x, scroll.y)
        } else if world
            .get::<Style>(entity)
            .map(|s| s.clip_children)
            .unwrap_or(false)
        {
            (
                intersect_with_self(clip, &node.rect),
                Fixed::ZERO,
                Fixed::ZERO,
            )
        } else {
            (*clip, Fixed::ZERO, Fixed::ZERO)
        };

    for child in &node.children {
        draw_tree_offset(
            child,
            world,
            entities,
            idx,
            renderer,
            &child_clip,
            scroll_x,
            scroll_y,
            &tf,
            &tf_3d,
        );
    }
}

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
    let quad = quad_for(world, entity, shifted_rect, parent_transform_3d);

    let cull_rect = quad.map(quad_bbox).unwrap_or(shifted_rect);
    if !rects_intersect(&cull_rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    if *idx < entities.len() {
        if let Some(style) = world.get::<Style>(entity) {
            let mut ctx = ViewCtx {
                style,
                transform: tf,
                quad,
                clip,
                bg_handled: false,
            };
            if let Some(registry) = world.resource::<ViewRegistry>() {
                for view in registry.iter() {
                    (view.render)(renderer, world, entity, &shifted_rect, &mut ctx);
                }
            }

            if let Some(pb) = world.get::<ProgressBar>(entity) {
                renderer.draw(
                    &DrawCommand::Fill {
                        area: shifted_rect,
                        transform: tf,
                        quad,
                        color: pb.track_color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
                let fill_w = Fixed::from_f32(shifted_rect.w.to_f32() * pb.value.clamp(0.0, 1.0));
                if fill_w > Fixed::ZERO {
                    renderer.draw(
                        &DrawCommand::Fill {
                            area: Rect {
                                x: shifted_rect.x,
                                y: shifted_rect.y,
                                w: fill_w,
                                h: shifted_rect.h,
                            },
                            transform: tf,
                            quad,
                            color: pb.fill_color,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            if let Some(tb) = world.get::<TabBar>(entity) {
                if tb.count > 0 {
                    let tab_w = shifted_rect.w / Fixed::from_int(tb.count as i32);
                    let indicator_x = shifted_rect.x + tb.indicator_offset * tab_w;
                    let indicator_y = shifted_rect.y + shifted_rect.h - tb.indicator_height;
                    renderer.draw(
                        &DrawCommand::Fill {
                            area: Rect {
                                x: indicator_x,
                                y: indicator_y,
                                w: tab_w,
                                h: tb.indicator_height,
                            },
                            transform: tf,
                            quad,
                            color: tb.indicator_color,
                            radius: Fixed::ZERO,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            if let Some(ti) = world.get::<TextInput>(entity) {
                draw_text_input(renderer, world, entity, ti, &shifted_rect, &tf, quad, clip);
            }
            if let Some(img) = world.get::<Image>(entity) {
                renderer.draw(
                    &DrawCommand::Blit {
                        pos: Point {
                            x: shifted_rect.x,
                            y: shifted_rect.y,
                        },
                        size: Point {
                            x: shifted_rect.w,
                            y: shifted_rect.h,
                        },
                        transform: tf,
                        quad,
                        texture: img.texture,
                    },
                    clip,
                );
            }
            if let Some(text) = world.get::<Text>(entity) {
                let color = style.text_color.unwrap_or(Color::rgb(255, 255, 255));
                renderer.draw(
                    &DrawCommand::Label {
                        pos: Point {
                            x: shifted_rect.x + Fixed::from_int(2),
                            y: shifted_rect.y + Fixed::from_int(2),
                        },
                        transform: tf,
                        text: &text.0,
                        color,
                        opa: 255,
                    },
                    clip,
                );
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
        );
    }
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
    draw_tree(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        renderer,
        &clip,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
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
    draw_tree(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        renderer,
        dirty_rect,
        &Transform::IDENTITY,
        &Transform3D::IDENTITY,
    );
}

struct DirtyBounds {
    min_x: Fixed,
    min_y: Fixed,
    max_x: Fixed,
    max_y: Fixed,
}

fn collect_dirty_walk(
    node: &LayoutNode,
    world: &mut World,
    entities: &[Entity],
    idx: &mut usize,
    parent_3d: &Transform3D,
    scroll_offset: (Fixed, Fixed),
    bounds: &mut DirtyBounds,
) {
    use super::dirty::Dirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);

    if world.get::<Dirty>(entity).is_some() {
        let curr_layout = quad_for(world, entity, node.rect, parent_3d)
            .map(quad_bbox)
            .unwrap_or(node.rect);
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
        collect_dirty_walk(child, world, entities, idx, &tf_3d, child_scroll, bounds);
    }
}

/// Collect the logical-pixel rects of all dirty entities, then remove Dirty flags.
/// Returns the bounding rect of all dirty regions, or None if nothing dirty.
pub fn collect_dirty_region(world: &mut World, root: Entity, transform: &Viewport) -> Option<Rect> {
    let (logical_w, logical_h) = transform.logical_size();

    let mut layout_tree = build_layout_tree(world, root)?;
    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );

    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    let mut bounds = DirtyBounds {
        min_x: Fixed::from(logical_w),
        min_y: Fixed::from(logical_h),
        max_x: Fixed::from_int(-1),
        max_y: Fixed::from_int(-1),
    };

    let mut idx = 0;
    collect_dirty_walk(
        &layout_tree,
        world,
        &entities,
        &mut idx,
        &Transform3D::IDENTITY,
        (Fixed::ZERO, Fixed::ZERO),
        &mut bounds,
    );

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
        let mut w = World::default();
        crate::widget::view::install_default_registry(&mut w);
        w
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
                bg_color: Some(Color::rgb(0, 0, 0)),
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
                bg_color: Some(Color::rgb(255, 0, 0)),
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
                bg_color: Some(Color::rgb(0, 0, 0)),
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
                bg_color: Some(Color::rgb(255, 0, 0)),
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
                bg_color: Some(Color::rgb(0, 0, 0)),
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
                bg_color: Some(Color::rgb(255, 0, 0)),
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
        let mut w = World::default();
        crate::widget::view::install_default_registry(&mut w);
        w
    }

    #[test]
    fn hidden_widget_does_not_paint() {
        let mut world = make_world();
        let root = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 0, 0)),
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
                bg_color: Some(Color::rgb(255, 0, 0)),
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
