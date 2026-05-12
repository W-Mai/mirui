use alloc::vec::Vec;

use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::image::Image;
use crate::components::progress_bar::ProgressBar;
use crate::components::transform::WidgetTransform;
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Color, Fixed, Point, Rect, Transform, Viewport};

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
fn draw_tree(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
    parent_transform: &Transform,
) {
    if !rects_intersect(&node.rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    let entity = if *idx < entities.len() {
        entities[*idx]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };
    let tf = effective_transform(parent_transform, world, entity, node.rect);

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
                        quad: None,
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
                        quad: None,
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
                            quad: None,
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
                        quad: None,
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
) {
    let shifted_rect = Rect {
        x: node.rect.x - offset_x,
        y: node.rect.y - offset_y,
        w: node.rect.w,
        h: node.rect.h,
    };

    if !rects_intersect(&shifted_rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    let entity = if *idx < entities.len() {
        entities[*idx]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };
    let tf = effective_transform(parent_transform, world, entity, shifted_rect);

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
                        quad: None,
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
                        quad: None,
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
                            quad: None,
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
                        quad: None,
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
    );
}

/// Collect the logical-pixel rects of all dirty entities, then remove Dirty flags.
/// Returns the bounding rect of all dirty regions, or None if nothing dirty.
pub fn collect_dirty_region(world: &mut World, root: Entity, transform: &Viewport) -> Option<Rect> {
    use super::dirty::Dirty;

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

    let mut min_x: Fixed = Fixed::from(logical_w);
    let mut min_y: Fixed = Fixed::from(logical_h);
    let mut max_x: Fixed = Fixed::from_int(-1);
    let mut max_y: Fixed = Fixed::from_int(-1);

    for (i, &entity) in entities.iter().enumerate() {
        if world.get::<Dirty>(entity).is_some() {
            if let Some(rect) = find_rect_at_index(&layout_tree, i, &mut 0) {
                let rx = rect.x;
                let ry = rect.y;
                let rx2 = (rect.x + rect.w).ceil();
                let ry2 = (rect.y + rect.h).ceil();
                if rx < min_x {
                    min_x = rx;
                }
                if ry < min_y {
                    min_y = ry;
                }
                if rx2 > max_x {
                    max_x = rx2;
                }
                if ry2 > max_y {
                    max_y = ry2;
                }
            }
            // Include previous rect (old position) if present
            if let Some(prev) = world.remove::<super::dirty::PrevRect>(entity) {
                let px = prev.0.x;
                let py = prev.0.y;
                let px2 = (prev.0.x + prev.0.w).ceil();
                let py2 = (prev.0.y + prev.0.h).ceil();
                if px < min_x {
                    min_x = px;
                }
                if py < min_y {
                    min_y = py;
                }
                if px2 > max_x {
                    max_x = px2;
                }
                if py2 > max_y {
                    max_y = py2;
                }
            }
            world.remove::<Dirty>(entity);
        }
    }

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

fn find_rect_at_index(node: &LayoutNode, target: usize, idx: &mut usize) -> Option<Rect> {
    if *idx == target {
        return Some(node.rect);
    }
    *idx += 1;
    for child in &node.children {
        if let Some(r) = find_rect_at_index(child, target, idx) {
            return Some(r);
        }
    }
    None
}
