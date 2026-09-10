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
    let natural_w = if resolve_self {
        resolve_dimension(node.style.width, avail_w, node.intrinsic_width).unwrap_or(avail_w)
    } else {
        avail_w
    };
    let natural_h = if resolve_self {
        resolve_dimension(node.style.height, avail_h, node.intrinsic_height).unwrap_or(avail_h)
    } else {
        avail_h
    };
    let w = constrain_size(
        natural_w,
        node.style.min_width,
        node.style.max_width,
        avail_w,
        node.intrinsic_width,
    );
    let h = constrain_size(
        natural_h,
        node.style.min_height,
        node.style.max_height,
        avail_h,
        node.intrinsic_height,
    );

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
    let base_gap = if is_row {
        node.style.column_gap.resolve(inner_w)
    } else {
        node.style.row_gap.resolve(inner_h)
    }
    .unwrap_or(Fixed::ZERO)
    .max(Fixed::ZERO);

    let mut fixed_total = Fixed::ZERO;
    let mut grow_total = Fixed::ZERO;
    let mut child_count = 0usize;
    for child in &node.children {
        if child.style.position == Position::Absolute {
            continue;
        }
        child_count += 1;
        if flexible(child, is_row) {
            grow_total += child.style.grow;
        } else {
            fixed_total += natural_main_size(child, is_row, main_size)
                .map(|size| constrain_main(child, is_row, size, main_size))
                .unwrap_or_else(|| constrain_main(child, is_row, Fixed::ZERO, main_size));
        }
    }

    let gap_total = if child_count > 1 {
        base_gap * (child_count as i32 - 1)
    } else {
        Fixed::ZERO
    };
    let remaining = (main_size - fixed_total - gap_total).max(Fixed::ZERO);
    let flex_unit = resolve_flex_unit(&node.children, is_row, main_size, remaining, grow_total);

    let mut total_main = gap_total;
    for child in &node.children {
        if child.style.position == Position::Absolute {
            continue;
        }
        total_main += child_size(child, is_row, main_size, cross_size, flex_unit).0;
    }

    let free_space = (main_size - total_main).max(Fixed::ZERO);

    let (mut offset, distributed_gap) = match node.style.justify {
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
            if child_count == 0 {
                (Fixed::ZERO, Fixed::ZERO)
            } else {
                let gap = free_space / child_count as i32;
                (gap / 2, gap)
            }
        }
        JustifyContent::SpaceEvenly => {
            let gap = free_space / (child_count as i32 + 1);
            (gap, gap)
        }
    };

    for child in &mut node.children {
        if child.style.position == Position::Absolute {
            let abs_x = x + child.style.left.resolve(w).unwrap_or(Fixed::ZERO);
            let abs_y = y + child.style.top.resolve(h).unwrap_or(Fixed::ZERO);
            let abs_w = constrain_size(
                resolve_dimension(child.style.width, w, child.intrinsic_width)
                    .unwrap_or(Fixed::ZERO),
                child.style.min_width,
                child.style.max_width,
                w,
                child.intrinsic_width,
            );
            let abs_h = constrain_size(
                resolve_dimension(child.style.height, h, child.intrinsic_height)
                    .unwrap_or(Fixed::ZERO),
                child.style.min_height,
                child.style.max_height,
                h,
                child.intrinsic_height,
            );
            compute_node(child, abs_x, abs_y, abs_w, abs_h, false);
            continue;
        }

        let (m, c) = child_size(child, is_row, main_size, cross_size, flex_unit);

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
        offset += m + base_gap + distributed_gap;
    }
}

fn child_size(
    child: &LayoutNode,
    is_row: bool,
    main_size: Fixed,
    cross_size: Fixed,
    flex_unit: Fixed,
) -> (Fixed, Fixed) {
    let main = if flexible(child, is_row) {
        constrain_main(child, is_row, flex_unit * child.style.grow, main_size)
    } else {
        constrain_main(
            child,
            is_row,
            natural_main_size(child, is_row, main_size).unwrap_or(Fixed::ZERO),
            main_size,
        )
    };

    let (cross_dimension, min_cross, max_cross, cross_intrinsic) = if is_row {
        (
            child.style.height,
            child.style.min_height,
            child.style.max_height,
            child.intrinsic_height,
        )
    } else {
        (
            child.style.width,
            child.style.min_width,
            child.style.max_width,
            child.intrinsic_width,
        )
    };
    let cross = constrain_size(
        resolve_dimension(cross_dimension, cross_size, cross_intrinsic).unwrap_or(cross_size),
        min_cross,
        max_cross,
        cross_size,
        cross_intrinsic,
    );
    (main, cross)
}

fn resolve_flex_unit(
    children: &[LayoutNode],
    is_row: bool,
    main_size: Fixed,
    available: Fixed,
    grow_total: Fixed,
) -> Fixed {
    if grow_total <= Fixed::ZERO {
        return Fixed::ZERO;
    }
    let mut unit = available / grow_total;
    for _ in 0..children.len() {
        let mut bounded_total = Fixed::ZERO;
        let mut active_grow = Fixed::ZERO;
        for child in children {
            if !flexible(child, is_row) {
                continue;
            }
            let proposed = unit * child.style.grow;
            let (min, max) = main_bounds(child, is_row, main_size);
            if proposed < min {
                bounded_total += min;
            } else if max.is_some_and(|max| proposed > max) {
                bounded_total += max.unwrap_or(Fixed::ZERO);
            } else {
                active_grow += child.style.grow;
            }
        }
        if active_grow <= Fixed::ZERO {
            break;
        }
        let next = (available - bounded_total).max(Fixed::ZERO) / active_grow;
        if next == unit {
            break;
        }
        unit = next;
    }
    unit
}

fn flexible(child: &LayoutNode, is_row: bool) -> bool {
    let dimension = if is_row {
        child.style.width
    } else {
        child.style.height
    };
    dimension == Dimension::Auto && child.style.grow > Fixed::ZERO
}

fn natural_main_size(child: &LayoutNode, is_row: bool, main_size: Fixed) -> Option<Fixed> {
    let (dimension, intrinsic) = if is_row {
        (child.style.width, child.intrinsic_width)
    } else {
        (child.style.height, child.intrinsic_height)
    };
    resolve_dimension(dimension, main_size, intrinsic)
}

fn constrain_main(child: &LayoutNode, is_row: bool, size: Fixed, parent: Fixed) -> Fixed {
    let (min, max, intrinsic) = if is_row {
        (
            child.style.min_width,
            child.style.max_width,
            child.intrinsic_width,
        )
    } else {
        (
            child.style.min_height,
            child.style.max_height,
            child.intrinsic_height,
        )
    };
    constrain_size(size, min, max, parent, intrinsic)
}

fn main_bounds(child: &LayoutNode, is_row: bool, parent: Fixed) -> (Fixed, Option<Fixed>) {
    let (min, max, intrinsic) = if is_row {
        (
            child.style.min_width,
            child.style.max_width,
            child.intrinsic_width,
        )
    } else {
        (
            child.style.min_height,
            child.style.max_height,
            child.intrinsic_height,
        )
    };
    constraint_bounds(min, max, parent, intrinsic)
}

fn constrain_size(
    size: Fixed,
    min: Dimension,
    max: Dimension,
    parent: Fixed,
    intrinsic: Option<Fixed>,
) -> Fixed {
    let (min, max) = constraint_bounds(min, max, parent, intrinsic);
    size.min(max.unwrap_or(Fixed::MAX)).max(min)
}

fn constraint_bounds(
    min: Dimension,
    max: Dimension,
    parent: Fixed,
    intrinsic: Option<Fixed>,
) -> (Fixed, Option<Fixed>) {
    let min = resolve_constraint(min, parent, intrinsic)
        .unwrap_or(Fixed::ZERO)
        .max(Fixed::ZERO);
    let max = resolve_constraint(max, parent, intrinsic).map(|max| max.max(min));
    (min, max)
}

fn resolve_constraint(
    dimension: Dimension,
    parent: Fixed,
    intrinsic: Option<Fixed>,
) -> Option<Fixed> {
    match dimension {
        Dimension::Auto => None,
        Dimension::Content => intrinsic,
        Dimension::Px(_) | Dimension::Percent(_) => dimension.resolve(parent),
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
