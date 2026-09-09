use crate::types::{Dimension, Fixed, Rect};

use super::node::{AlignItems, FlexDirection, JustifyContent, LayoutNode, Position};

pub fn compute_layout(node: &mut LayoutNode, x: Fixed, y: Fixed, avail_w: Fixed, avail_h: Fixed) {
    compute_node(node, x, y, avail_w, avail_h, true);
}

fn compute_node(
    node: &mut LayoutNode,
    x: Fixed,
    y: Fixed,
    avail_w: Fixed,
    avail_h: Fixed,
    resolve_self: bool,
) {
    let w = if resolve_self {
        resolve_dimension(node.style.width, avail_w, node.intrinsic_width).unwrap_or(avail_w)
    } else {
        avail_w
    };
    let h = if resolve_self {
        resolve_dimension(node.style.height, avail_h, node.intrinsic_height).unwrap_or(avail_h)
    } else {
        avail_h
    };

    node.rect = Rect { x, y, w, h };

    if node.children.is_empty() {
        return;
    }

    let pad_l = node.style.padding.left.resolve(w).unwrap_or(Fixed::ZERO);
    let pad_r = node.style.padding.right.resolve(w).unwrap_or(Fixed::ZERO);
    let pad_t = node.style.padding.top.resolve(h).unwrap_or(Fixed::ZERO);
    let pad_b = node.style.padding.bottom.resolve(h).unwrap_or(Fixed::ZERO);

    let inner_w = (w - pad_l - pad_r).max(Fixed::ZERO);
    let inner_h = (h - pad_t - pad_b).max(Fixed::ZERO);
    let inner_x = x + pad_l;
    let inner_y = y + pad_t;

    let is_row = node.style.direction == FlexDirection::Row;
    let main_size = if is_row { inner_w } else { inner_h };
    let cross_size = if is_row { inner_h } else { inner_w };

    // Calculate fixed sizes and total grow (only flex children)
    let mut fixed_total = Fixed::ZERO;
    let mut grow_total = Fixed::ZERO;
    for child in &node.children {
        if child.style.position == Position::Absolute {
            continue;
        }
        let (dimension, intrinsic) = if is_row {
            (child.style.width, child.intrinsic_width)
        } else {
            (child.style.height, child.intrinsic_height)
        };
        let child_main = if dimension == Dimension::Auto && child.style.grow > Fixed::ZERO {
            None
        } else {
            resolve_dimension(dimension, main_size, intrinsic)
        };
        if let Some(s) = child_main {
            fixed_total += s;
        } else if child.style.grow > Fixed::ZERO {
            grow_total += child.style.grow;
        }
    }

    let remaining = (main_size - fixed_total).max(Fixed::ZERO);

    // Compute each child's main axis size
    let child_count = node
        .children
        .iter()
        .filter(|c| c.style.position != Position::Absolute)
        .count();
    let mut sizes: alloc::vec::Vec<(Fixed, Fixed)> =
        alloc::vec::Vec::with_capacity(node.children.len());

    for child in &node.children {
        if child.style.position == Position::Absolute {
            sizes.push((Fixed::ZERO, Fixed::ZERO));
            continue;
        }
        let (main_dimension, main_intrinsic) = if is_row {
            (child.style.width, child.intrinsic_width)
        } else {
            (child.style.height, child.intrinsic_height)
        };
        let child_main = if main_dimension == Dimension::Auto && child.style.grow > Fixed::ZERO {
            None
        } else {
            resolve_dimension(main_dimension, main_size, main_intrinsic)
        };
        let m = if let Some(s) = child_main {
            s
        } else if child.style.grow > Fixed::ZERO && grow_total > Fixed::ZERO {
            remaining * child.style.grow / grow_total
        } else {
            Fixed::ZERO
        };

        let (cross_dimension, cross_intrinsic) = if is_row {
            (child.style.height, child.intrinsic_height)
        } else {
            (child.style.width, child.intrinsic_width)
        };
        let child_cross = resolve_dimension(cross_dimension, cross_size, cross_intrinsic);
        let c = child_cross.unwrap_or(cross_size);

        sizes.push((m, c));
    }

    // Justify: compute starting offset and gap (flex children only)
    let total_main: Fixed = sizes
        .iter()
        .enumerate()
        .filter(|(i, _)| node.children[*i].style.position != Position::Absolute)
        .map(|(_, (m, _))| *m)
        .fold(Fixed::ZERO, |acc, v| acc + v);
    let free_space = (main_size - total_main).max(Fixed::ZERO);

    let (mut offset, gap) = match node.style.justify {
        JustifyContent::FlexStart => (Fixed::ZERO, Fixed::ZERO),
        JustifyContent::FlexEnd => (free_space, Fixed::ZERO),
        JustifyContent::Center => (free_space / 2, Fixed::ZERO),
        JustifyContent::SpaceBetween => {
            if child_count > 1 {
                (Fixed::ZERO, free_space / (child_count as i32 - 1))
            } else {
                (Fixed::ZERO, Fixed::ZERO)
            }
        }
        JustifyContent::SpaceAround => {
            let g = free_space / child_count as i32;
            (g / 2, g)
        }
        JustifyContent::SpaceEvenly => {
            let g = free_space / (child_count as i32 + 1);
            (g, g)
        }
    };

    // Position children
    for (i, child) in node.children.iter_mut().enumerate() {
        if child.style.position == Position::Absolute {
            let abs_x = x + child.style.left.resolve(w).unwrap_or(Fixed::ZERO);
            let abs_y = y + child.style.top.resolve(h).unwrap_or(Fixed::ZERO);
            let abs_w = resolve_dimension(child.style.width, w, child.intrinsic_width)
                .unwrap_or(Fixed::ZERO);
            let abs_h = resolve_dimension(child.style.height, h, child.intrinsic_height)
                .unwrap_or(Fixed::ZERO);
            compute_node(child, abs_x, abs_y, abs_w, abs_h, false);
            continue;
        }

        let (m, c) = sizes[i];

        // Cross axis alignment
        let cross_offset = match node.style.align {
            AlignItems::FlexStart | AlignItems::Stretch => Fixed::ZERO,
            AlignItems::FlexEnd => cross_size - c,
            AlignItems::Center => (cross_size - c) / 2,
        };

        let (cx, cy, cw, ch) = if is_row {
            (inner_x + offset, inner_y + cross_offset, m, c)
        } else {
            (inner_x + cross_offset, inner_y + offset, c, m)
        };

        compute_node(child, cx, cy, cw, ch, false);
        offset += m + gap;
    }
}

fn resolve_dimension(
    dimension: Dimension,
    parent: Fixed,
    intrinsic: Option<Fixed>,
) -> Option<Fixed> {
    match dimension {
        Dimension::Content => Some(intrinsic.unwrap_or(Fixed::ZERO)),
        Dimension::Auto => intrinsic,
        Dimension::Px(_) | Dimension::Percent(_) => dimension.resolve(parent),
    }
}
