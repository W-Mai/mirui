use alloc::vec;
use alloc::vec::Vec;

use crate::components::scroll::ScrollOffset;
use crate::components::transform::WidgetTransform;
use crate::components::transform_3d::WidgetTransform3D;
use crate::ecs::{Entity, World};
use crate::layout::{LayoutNode, compute_layout};
use crate::types::{Fixed, Point, Rect, Transform, Transform3D};
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
fn compute_scroll_offsets(world: &World, root: Entity, entities: &[Entity]) -> Vec<(Fixed, Fixed)> {
    let mut offsets = vec![(Fixed::ZERO, Fixed::ZERO); entities.len()];
    compute_scroll_recursive(world, root, Fixed::ZERO, Fixed::ZERO, &mut offsets, &mut 0);
    offsets
}

fn compute_scroll_recursive(
    world: &World,
    entity: Entity,
    acc_x: Fixed,
    acc_y: Fixed,
    offsets: &mut [(Fixed, Fixed)],
    idx: &mut usize,
) {
    if *idx < offsets.len() {
        offsets[*idx] = (acc_x, acc_y);
    }
    *idx += 1;

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

fn compute_transforms(
    world: &World,
    root: Entity,
    entities: &[Entity],
    rects: &[Rect],
) -> Vec<Transform> {
    let mut out = vec![Transform::IDENTITY; entities.len()];
    compute_transforms_recursive(world, root, &Transform::IDENTITY, rects, &mut out, &mut 0);
    out
}

fn compute_transforms_recursive(
    world: &World,
    entity: Entity,
    parent: &Transform,
    rects: &[Rect],
    out: &mut [Transform],
    idx: &mut usize,
) {
    let my_idx = *idx;
    let rect = rects.get(my_idx).copied().unwrap_or(Rect::ZERO);
    let local = world
        .get::<WidgetTransform>(entity)
        .map(|t| t.0)
        .unwrap_or(Transform::IDENTITY);
    let effective = if local.is_identity() {
        *parent
    } else {
        let cx = rect.x + rect.w / Fixed::from_int(2);
        let cy = rect.y + rect.h / Fixed::from_int(2);
        parent
            .compose(&Transform::translate(cx, cy))
            .compose(&local)
            .compose(&Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy))
    };
    if my_idx < out.len() {
        out[my_idx] = effective;
    }
    *idx += 1;

    if let Some(children) = world.get::<Children>(entity) {
        for &child in &children.0 {
            if world.get::<Widget>(child).is_some() {
                compute_transforms_recursive(world, child, &effective, rects, out, idx);
            }
        }
    }
}

/// Hit test: given a coordinate, find the deepest widget entity that contains it.
/// Accounts for scroll offsets.
pub fn hit_test(
    world: &World,
    root: Entity,
    x: Fixed,
    y: Fixed,
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
        screen_w.into(),
        screen_h.into(),
    );

    let mut rects = Vec::new();
    collect_rects(&root_node, &mut rects);

    let scroll_offsets = compute_scroll_offsets(world, root, &entities);
    let transforms = compute_transforms(world, root, &entities, &rects);

    let mut hit = None;
    for (i, rect) in rects.iter().enumerate() {
        let (sx, sy) = scroll_offsets[i];
        let shifted = Rect {
            x: rect.x - sx,
            y: rect.y - sy,
            w: rect.w,
            h: rect.h,
        };

        if let Some(wt3d) = world.get::<WidgetTransform3D>(entities[i]) {
            if !wt3d.0.is_identity() {
                let cx = shifted.x + shifted.w / Fixed::from_int(2);
                let cy = shifted.y + shifted.h / Fixed::from_int(2);
                let wrapped = Transform3D::translate(cx, cy)
                    .compose(&wt3d.0)
                    .compose(&Transform3D::translate(Fixed::ZERO - cx, Fixed::ZERO - cy));
                if let Some(q) = wrapped.apply_rect(shifted) {
                    if crate::types::transform_3d::point_in_quad(&q, Point { x, y }) {
                        hit = Some(entities[i]);
                    }
                }
                continue;
            }
        }

        let probe = if transforms[i].is_identity() {
            Point { x, y }
        } else {
            match transforms[i].inverse() {
                Some(inv) => inv.apply_point(Point { x, y }),
                None => continue,
            }
        };

        if probe.x >= shifted.x
            && probe.x < shifted.x + shifted.w
            && probe.y >= shifted.y
            && probe.y < shifted.y + shifted.h
        {
            hit = Some(entities[i]);
        }
    }
    hit
}
