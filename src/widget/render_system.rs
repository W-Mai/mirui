use alloc::vec::Vec;

use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::image::Image;
use crate::components::progress_bar::ProgressBar;
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Color, Point, Rect};

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
    a.x < b.x + b.w as i32
        && a.x + a.w as i32 > b.x
        && a.y < b.y + b.h as i32
        && a.y + a.h as i32 > b.y
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
                let fill_w = ((node.rect.w as f32) * pb.value.clamp(0.0, 1.0)) as u16;
                if fill_w > 0 {
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
                            x: node.rect.x,
                            y: node.rect.y,
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
                            x: node.rect.x + 2,
                            y: node.rect.y + 2,
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

    for child in &node.children {
        draw_tree(child, world, entities, idx, renderer, clip);
    }
}

fn scale_rects(node: &mut LayoutNode, scale: u16) {
    let s = scale as i32;
    node.rect.x *= s;
    node.rect.y *= s;
    node.rect.w *= scale;
    node.rect.h *= scale;
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

    compute_layout(&mut layout_tree, 0, 0, logical_w, logical_h);

    // Scale all rects to physical pixels
    scale_rects(&mut layout_tree, scale);

    let clip = Rect {
        x: 0,
        y: 0,
        w: screen_w,
        h: screen_h,
    };
    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    let mut idx = 0;
    draw_tree(&layout_tree, world, &entities, &mut idx, renderer, &clip);
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

    compute_layout(&mut layout_tree, 0, 0, logical_w, logical_h);
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
    compute_layout(&mut layout_tree, 0, 0, logical_w, logical_h);
    scale_rects(&mut layout_tree, scale);

    let mut entities = Vec::new();
    collect_entities_preorder(world, root, &mut entities);

    let mut min_x = screen_w as i32;
    let mut min_y = screen_h as i32;
    let mut max_x: i32 = -1;
    let mut max_y: i32 = -1;

    for (i, &entity) in entities.iter().enumerate() {
        if world.get::<Dirty>(entity).is_some() {
            if let Some(rect) = find_rect_at_index(&layout_tree, i, &mut 0) {
                if rect.x < min_x {
                    min_x = rect.x;
                }
                if rect.y < min_y {
                    min_y = rect.y;
                }
                let rx = rect.x + rect.w as i32;
                let ry = rect.y + rect.h as i32;
                if rx > max_x {
                    max_x = rx;
                }
                if ry > max_y {
                    max_y = ry;
                }
            }
            // Include previous rect (old position) if present
            if let Some(prev) = world.remove::<super::dirty::PrevRect>(entity) {
                let pr = prev.0;
                let s = scale as i32;
                let px = pr.x * s;
                let py = pr.y * s;
                let pw = pr.w as i32 * s;
                let ph = pr.h as i32 * s;
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
        Some(Rect {
            x: min_x,
            y: min_y,
            w: (max_x - min_x) as u16,
            h: (max_y - min_y) as u16,
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
