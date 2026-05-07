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

/// Recursively emit draw commands from the computed layout tree
fn draw_tree(
    node: &LayoutNode,
    world: &World,
    entities: &[Entity],
    idx: &mut usize,
    renderer: &mut dyn Renderer,
    clip: &Rect,
) {
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

/// Collect entities in pre-order (matching layout tree traversal)
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
pub fn render(
    world: &World,
    root: Entity,
    screen_w: u16,
    screen_h: u16,
    renderer: &mut dyn Renderer,
) {
    let Some(mut layout_tree) = build_layout_tree(world, root) else {
        return;
    };

    compute_layout(&mut layout_tree, 0, 0, screen_w, screen_h);

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
