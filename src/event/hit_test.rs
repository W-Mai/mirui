use alloc::vec;
use alloc::vec::Vec;

use crate::components::scroll::ScrollOffset;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Fixed, Rect};
use crate::widget::{Children, Style, Widget};

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

fn collect_rects(node: &LayoutNode, rects: &mut Vec<Rect>) {
    rects.push(node.rect);
    for child in &node.children {
        collect_rects(child, rects);
    }
}

/// Compute the accumulated scroll offset for each entity.
/// For each entity, this is the sum of all ancestor ScrollOffsets.
fn compute_scroll_offsets(world: &World, root: Entity, entities: &[Entity]) -> Vec<(i32, i32)> {
    let mut offsets = vec![(0i32, 0i32); entities.len()];
    compute_scroll_recursive(world, root, 0, 0, &mut offsets, &mut 0);
    offsets
}

fn compute_scroll_recursive(
    world: &World,
    entity: Entity,
    acc_x: i32,
    acc_y: i32,
    offsets: &mut [(i32, i32)],
    idx: &mut usize,
) {
    if *idx < offsets.len() {
        offsets[*idx] = (acc_x, acc_y);
    }
    *idx += 1;

    // If this entity has ScrollOffset, add it for children
    let (child_acc_x, child_acc_y) = if let Some(scroll) = world.get::<ScrollOffset>(entity) {
        (acc_x + scroll.x, acc_y + scroll.y)
    } else {
        (acc_x, acc_y)
    };

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if world.get::<Widget>(child).is_some() {
                compute_scroll_recursive(world, child, child_acc_x, child_acc_y, offsets, idx);
            }
        }
    }
}

/// Hit test: given a coordinate, find the deepest widget entity that contains it.
/// Accounts for scroll offsets.
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
    compute_layout(
        &mut root_node,
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(screen_w as i32),
        Fixed::from_int(screen_h as i32),
    );

    let mut rects = Vec::new();
    collect_rects(&root_node, &mut rects);

    // Compute accumulated scroll offsets
    let scroll_offsets = compute_scroll_offsets(world, root, &entities);

    // Find deepest widget that contains the point (accounting for scroll)
    let mut hit = None;
    for (i, rect) in rects.iter().enumerate() {
        let (sx, sy) = scroll_offsets[i];
        // Widget's visual position = layout position - scroll offset
        let vx = rect.x.to_int() - sx;
        let vy = rect.y.to_int() - sy;
        let rw = rect.w.to_int();
        let rh = rect.h.to_int();
        if x >= vx && x < vx + rw && y >= vy && y < vy + rh {
            hit = Some(entities[i]);
        }
    }
    hit
}
