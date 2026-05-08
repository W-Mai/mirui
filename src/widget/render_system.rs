use alloc::vec::Vec;

use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::image::Image;
use crate::components::progress_bar::ProgressBar;
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Color, Fixed, Point, Rect};

use super::{Children, Style, Text, Widget};

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
) {
    // Skip entire subtree if node doesn't intersect clip
    if !rects_intersect(&node.rect, clip) {
        *idx += count_nodes(node);
        return;
    }

    if *idx < entities.len() {
        let entity = entities[*idx];
        if let Some(style) = world.get::<Style>(entity) {
            // Button overrides bg_color with pressed state
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
                        color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
            }
            if let Some(border_color) = style.border_color {
                if style.border_width > 0 {
                    renderer.draw(
                        &DrawCommand::Border {
                            area: node.rect,
                            color: border_color,
                            width: style.border_width,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            // ProgressBar: draw track + fill
            if let Some(pb) = world.get::<ProgressBar>(entity) {
                renderer.draw(
                    &DrawCommand::Fill {
                        area: node.rect,
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
                            color: pb.fill_color,
                            radius: style.border_radius,
                            opa: 255,
                        },
                        clip,
                    );
                }
            }
            // Image: blit pixels
            if let Some(img) = world.get::<Image>(entity) {
                renderer.draw(
                    &DrawCommand::Blit {
                        pos: Point {
                            x: node.rect.x.to_int(),
                            y: node.rect.y.to_int(),
                        },
                        data: &img.data,
                        width: img.width,
                        height: img.height,
                    },
                    clip,
                );
            }
            // Draw text if present
            if let Some(text) = world.get::<Text>(entity) {
                let color = style.text_color.unwrap_or(Color::rgb(255, 255, 255));
                renderer.draw(
                    &DrawCommand::Label {
                        pos: Point {
                            x: node.rect.x.to_int() + 2,
                            y: node.rect.y.to_int() + 2,
                        },
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

    // Check if this widget has ScrollOffset — clip children + offset
    let entity = if *idx > 0 && (*idx - 1) < entities.len() {
        entities[*idx - 1]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };

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
            let s = world
                .resource::<crate::backend::DisplayInfo>()
                .map(|d| d.scale as i32)
                .unwrap_or(1);
            (
                new_clip,
                Fixed::from_int(scroll.x * s),
                Fixed::from_int(scroll.y * s),
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

    if *idx < entities.len() {
        let entity = entities[*idx];
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
                        color,
                        radius: style.border_radius,
                        opa: 255,
                    },
                    clip,
                );
            }
            if let Some(border_color) = style.border_color {
                if style.border_width > 0 {
                    renderer.draw(
                        &DrawCommand::Border {
                            area: shifted_rect,
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
                            x: shifted_rect.x.to_int(),
                            y: shifted_rect.y.to_int(),
                        },
                        data: &img.data,
                        width: img.width,
                        height: img.height,
                    },
                    clip,
                );
            }
            if let Some(text) = world.get::<Text>(entity) {
                let color = style.text_color.unwrap_or(Color::rgb(255, 255, 255));
                renderer.draw(
                    &DrawCommand::Label {
                        pos: Point {
                            x: shifted_rect.x.to_int() + 2,
                            y: shifted_rect.y.to_int() + 2,
                        },
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

    // Recurse — nested scroll containers stack offsets
    let cur_entity = if *idx > 0 && (*idx - 1) < entities.len() {
        entities[*idx - 1]
    } else {
        Entity {
            id: u32::MAX,
            generation: 0,
        }
    };
    let (child_clip, sx, sy) =
        if let Some(scroll) = world.get::<crate::components::scroll::ScrollOffset>(cur_entity) {
            let cx = clip.x.max(shifted_rect.x);
            let cy = clip.y.max(shifted_rect.y);
            let cx2 = (clip.x + clip.w).min(shifted_rect.x + shifted_rect.w);
            let cy2 = (clip.y + clip.h).min(shifted_rect.y + shifted_rect.h);
            let s = world
                .resource::<crate::backend::DisplayInfo>()
                .map(|d| d.scale as i32)
                .unwrap_or(1);
            (
                Rect {
                    x: cx,
                    y: cy,
                    w: if cx2 > cx { cx2 - cx } else { Fixed::ZERO },
                    h: if cy2 > cy { cy2 - cy } else { Fixed::ZERO },
                },
                offset_x + Fixed::from_int(scroll.x * s),
                offset_y + Fixed::from_int(scroll.y * s),
            )
        } else {
            (*clip, offset_x, offset_y)
        };

    for child in &node.children {
        draw_tree_offset(child, world, entities, idx, renderer, &child_clip, sx, sy);
    }
}

fn scale_rects(node: &mut LayoutNode, scale: u16) {
    let s = scale as i32;
    node.rect.x = node.rect.x * s;
    node.rect.y = node.rect.y * s;
    node.rect.w = node.rect.w * s;
    node.rect.h = node.rect.h * s;
    for child in &mut node.children {
        scale_rects(child, scale);
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

/// Run the render system: build layout → compute → draw
/// `screen_w`/`screen_h` are physical pixels, `scale` is the HiDPI factor.
/// Layout is computed in logical pixels (physical / scale), then scaled up for rendering.
pub fn render(
    world: &World,
    root: Entity,
    screen_w: u16,
    screen_h: u16,
    scale: u16,
    renderer: &mut dyn Renderer,
) {
    let scale = if scale == 0 { 1 } else { scale };
    let logical_w = screen_w / scale;
    let logical_h = screen_h / scale;

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

    // Scale all rects to physical pixels
    scale_rects(&mut layout_tree, scale);

    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: screen_w.into(),
        h: screen_h.into(),
    };
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    let mut idx = 0;
    draw_tree(&layout_tree, world, &entities, &mut idx, renderer, &clip);
}

/// Compute layout and write ComputedRect to each entity (logical pixels).
pub fn update_layout(world: &mut World, root: Entity, screen_w: u16, screen_h: u16, scale: u16) {
    let scale = if scale == 0 { 1 } else { scale };
    let logical_w = screen_w / scale;
    let logical_h = screen_h / scale;

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
    screen_w: u16,
    screen_h: u16,
    scale: u16,
    dirty_rect: &Rect,
    renderer: &mut dyn Renderer,
) {
    let scale = if scale == 0 { 1 } else { scale };
    let logical_w = screen_w / scale;
    let logical_h = screen_h / scale;

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
    scale_rects(&mut layout_tree, scale);

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
    );
}

/// Collect the physical-pixel rects of all dirty entities, then remove Dirty flags.
/// Returns the bounding rect of all dirty regions, or None if nothing dirty.
pub fn collect_dirty_region(
    world: &mut World,
    root: Entity,
    screen_w: u16,
    screen_h: u16,
    scale: u16,
) -> Option<Rect> {
    use super::dirty::Dirty;

    let scale = if scale == 0 { 1 } else { scale };
    let logical_w = screen_w / scale;
    let logical_h = screen_h / scale;

    let mut layout_tree = build_layout_tree(world, root)?;
    compute_layout(
        &mut layout_tree,
        Fixed::ZERO,
        Fixed::ZERO,
        logical_w.into(),
        logical_h.into(),
    );
    scale_rects(&mut layout_tree, scale);

    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    let mut min_x: i32 = screen_w as i32;
    let mut min_y: i32 = screen_h as i32;
    let mut max_x: i32 = -1;
    let mut max_y: i32 = -1;

    for (i, &entity) in entities.iter().enumerate() {
        if world.get::<Dirty>(entity).is_some() {
            if let Some(rect) = find_rect_at_index(&layout_tree, i, &mut 0) {
                let rx = rect.x.to_int();
                let ry = rect.y.to_int();
                let rx2 = (rect.x + rect.w).to_int_ceil();
                let ry2 = (rect.y + rect.h).to_int_ceil();
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
                let pr = prev.0;
                let s = scale as i32;
                let px = pr.x.to_int() * s;
                let py = pr.y.to_int() * s;
                let pw = pr.w.to_int() * s;
                let ph = pr.h.to_int() * s;
                if px < min_x {
                    min_x = px;
                }
                if py < min_y {
                    min_y = py;
                }
                if px + pw > max_x {
                    max_x = px + pw;
                }
                if py + ph > max_y {
                    max_y = py + ph;
                }
            }
            world.remove::<Dirty>(entity);
        }
    }

    if max_x < 0 {
        None
    } else {
        Some(Rect::new(min_x, min_y, max_x - min_x, max_y - min_y))
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
