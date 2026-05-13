use alloc::vec::Vec;

use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::image::Image;
use crate::components::progress_bar::ProgressBar;
use crate::components::transform::WidgetTransform;
use crate::components::transform_3d::{TransformOrigin, WidgetTransform3D};
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, Viewport};

use super::{Children, Style, Text, Widget};

/// Compose `parent` with the entity's local transform (if any),
/// wrapped so rotation/scale pivot on the widget's centre instead
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
    let mut min_x = q[0].x;
    let mut max_x = q[0].x;
    let mut min_y = q[0].y;
    let mut max_y = q[0].y;
    for p in &q[1..] {
        if p.x < min_x {
            min_x = p.x;
        }
        if p.x > max_x {
            max_x = p.x;
        }
        if p.y < min_y {
            min_y = p.y;
        }
        if p.y > max_y {
            max_y = p.y;
        }
    }
    Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    }
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

/// Recursively build a LayoutNode tree from ECS entities
fn build_layout_tree(world: &World, entity: Entity) -> Option<LayoutNode> {
    world.get::<Widget>(entity)?;
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
            let bg = if let Some(btn) = world.get::<Button>(entity) {
                Some(btn.current_color())
            } else if let Some(cb) = world.get::<Checkbox>(entity) {
                Some(cb.current_color())
            } else {
                style.bg_color
            };

            if let Some(color) = bg {
                renderer.draw(
                    &DrawCommand::Fill {
                        area: node.rect,
                        transform: tf,
                        quad,
                        color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
            }
            if let Some(border_color) = style.border_color {
                if style.border_width > Fixed::ZERO {
                    renderer.draw(
                        &DrawCommand::Border {
                            area: node.rect,
                            transform: tf,
                            quad,
                            color: border_color,
                            width: style.border_width,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
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
        if let Some(scroll) = world.get::<crate::components::scroll::ScrollOffset>(entity) {
            let cx = clip.x.max(node.rect.x);
            let cy = clip.y.max(node.rect.y);
            let cx2 = (clip.x + clip.w).min(node.rect.x + node.rect.w);
            let cy2 = (clip.y + clip.h).min(node.rect.y + node.rect.h);
            let new_clip = Rect {
                x: cx,
                y: cy,
                w: if cx2 > cx { cx2 - cx } else { Fixed::ZERO },
                h: if cy2 > cy { cy2 - cy } else { Fixed::ZERO },
            };
            (new_clip, scroll.x, scroll.y)
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
            let bg = if let Some(btn) = world.get::<Button>(entity) {
                Some(btn.current_color())
            } else if let Some(cb) = world.get::<Checkbox>(entity) {
                Some(cb.current_color())
            } else {
                style.bg_color
            };

            if let Some(color) = bg {
                renderer.draw(
                    &DrawCommand::Fill {
                        area: shifted_rect,
                        transform: tf,
                        quad,
                        color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
            }
            if let Some(border_color) = style.border_color {
                if style.border_width > Fixed::ZERO {
                    renderer.draw(
                        &DrawCommand::Border {
                            area: shifted_rect,
                            transform: tf,
                            quad,
                            color: border_color,
                            width: style.border_width,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
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
        if let Some(scroll) = world.get::<crate::components::scroll::ScrollOffset>(entity) {
            let cx = clip.x.max(shifted_rect.x);
            let cy = clip.y.max(shifted_rect.y);
            let cx2 = (clip.x + clip.w).min(shifted_rect.x + shifted_rect.w);
            let cy2 = (clip.y + clip.h).min(shifted_rect.y + shifted_rect.h);
            (
                Rect {
                    x: cx,
                    y: cy,
                    w: if cx2 > cx { cx2 - cx } else { Fixed::ZERO },
                    h: if cy2 > cy { cy2 - cy } else { Fixed::ZERO },
                },
                offset_x + scroll.x,
                offset_y + scroll.y,
            )
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
    bounds: &mut DirtyBounds,
) {
    use super::dirty::Dirty;
    if *idx >= entities.len() {
        return;
    }
    let entity = entities[*idx];
    let tf_3d = accumulate_3d(parent_3d, world, entity, node.rect);

    if world.get::<Dirty>(entity).is_some() {
        let effective_rect = quad_for(world, entity, node.rect, parent_3d)
            .map(quad_bbox)
            .unwrap_or(node.rect);
        let mut x0 = effective_rect.x.floor();
        let mut y0 = effective_rect.y.floor();
        let mut x1 = (effective_rect.x + effective_rect.w).ceil();
        let mut y1 = (effective_rect.y + effective_rect.h).ceil();

        if let Some(prev) = world.remove::<super::dirty::PrevRect>(entity) {
            let p_x1 = (prev.0.x + prev.0.w).ceil();
            let p_y1 = (prev.0.y + prev.0.h).ceil();
            if prev.0.x < x0 {
                x0 = prev.0.x;
            }
            if prev.0.y < y0 {
                y0 = prev.0.y;
            }
            if p_x1 > x1 {
                x1 = p_x1;
            }
            if p_y1 > y1 {
                y1 = p_y1;
            }
        }

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
        world.insert(
            entity,
            super::dirty::PrevRect(Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }),
        );
        world.remove::<Dirty>(entity);
    }

    *idx += 1;
    for child in &node.children {
        collect_dirty_walk(child, world, entities, idx, &tf_3d, bounds);
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
