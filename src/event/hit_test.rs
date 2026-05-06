use alloc::vec::Vec;

use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::Rect;
use crate::widget::{Children, Style, Widget};

/// Build layout tree and collect (entity, rect) pairs in pre-order
fn build_rects(
    world: &World,
    entity: Entity,
    parent_node: &mut LayoutNode,
    entities: &mut Vec<Entity>,
) {
    entities.push(entity);
    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if world.get::<Widget>(child).is_none() {
                continue;
            }
            if let Some(style) = world.get::<Style>(child) {
                let mut child_node = LayoutNode::new(style.layout);
                build_rects(world, child, &mut child_node, entities);
                parent_node.add_child(child_node);
            }
        }
    }
}

/// Collect computed rects from layout tree in pre-order
fn collect_rects(node: &LayoutNode, rects: &mut Vec<Rect>) {
    rects.push(node.rect);
    for child in &node.children {
        collect_rects(child, rects);
    }
}

/// Hit test: given a coordinate, find the deepest widget entity that contains it.
/// Returns entities from deepest to shallowest (last drawn = first hit).
pub fn hit_test(
    world: &World,
    root: Entity,
    x: i32,
    y: i32,
    screen_w: u16,
    screen_h: u16,
) -> Option<Entity> {
    let root_style = world.get::<Style>(root)?;
    let mut root_node = LayoutNode::new(root_style.layout);
    let mut entities = Vec::new();
    build_rects(world, root, &mut root_node, &mut entities);
    compute_layout(&mut root_node, 0, 0, screen_w, screen_h);

    let mut rects = Vec::new();
    collect_rects(&root_node, &mut rects);

    // Find deepest (last in pre-order that contains point)
    let mut hit = None;
    for (i, rect) in rects.iter().enumerate() {
        if x >= rect.x && x < rect.x + rect.w as i32 && y >= rect.y && y < rect.y + rect.h as i32 {
            hit = Some(entities[i]);
        }
    }
    hit
}
