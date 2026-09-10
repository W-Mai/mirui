use crate::types::{Dimension, Fixed, Fixed64, Rect};

use super::node::{AlignItems, FlexDirection, FlexWrap, JustifyContent, LayoutNode, Position};

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
    let main_gap = if is_row {
        node.style.column_gap.resolve(inner_w)
    } else {
        node.style.row_gap.resolve(inner_h)
    }
    .unwrap_or(Fixed::ZERO)
    .max(Fixed::ZERO);

    let cross_gap = if is_row {
        node.style.row_gap.resolve(inner_h)
    } else {
        node.style.column_gap.resolve(inner_w)
    }
    .unwrap_or(Fixed::ZERO)
    .max(Fixed::ZERO);

    for child in &mut node.children {
        if child.style.position == Position::Absolute {
            layout_absolute(child, x, y, w, h);
        }
    }

    let should_wrap = node.style.wrap == FlexWrap::Wrap;
    let mut line_cursor = 0usize;
    let mut line_count = 0usize;
    while let Some(line) = next_line(
        &node.children,
        line_cursor,
        is_row,
        main_size,
        cross_size,
        main_gap,
        should_wrap,
    ) {
        line_count += 1;
        line_cursor = line.end;
    }
    if line_count == 0 {
        return;
    }

    line_cursor = 0;
    let mut cross_offset = Fixed::ZERO;
    while let Some(line) = next_line(
        &node.children,
        line_cursor,
        is_row,
        main_size,
        cross_size,
        main_gap,
        should_wrap,
    ) {
        let line_cross = if should_wrap && line_count > 1 {
            line.natural_cross
        } else {
            cross_size
        };
        layout_line(
            &mut node.children[line.start..line.end],
            LineLayout {
                inner_x,
                inner_y,
                is_row,
                main_size,
                cross_size,
                line_cross,
                line_offset: cross_offset,
                main_gap,
                justify: node.style.justify,
                align: node.style.align,
                item_count: line.item_count,
            },
        );
        cross_offset += line_cross + cross_gap;
        line_cursor = line.end;
    }
}

#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    item_count: usize,
    natural_cross: Fixed,
}

#[derive(Clone, Copy)]
struct LineLayout {
    inner_x: Fixed,
    inner_y: Fixed,
    is_row: bool,
    main_size: Fixed,
    cross_size: Fixed,
    line_cross: Fixed,
    line_offset: Fixed,
    main_gap: Fixed,
    justify: JustifyContent,
    align: AlignItems,
    item_count: usize,
}

fn next_line(
    children: &[LayoutNode],
    cursor: usize,
    is_row: bool,
    main_size: Fixed,
    cross_size: Fixed,
    main_gap: Fixed,
    should_wrap: bool,
) -> Option<Line> {
    let start = (cursor..children.len())
        .find(|index| children[*index].style.position != Position::Absolute)?;
    let mut used_main = Fixed::ZERO;
    let mut natural_cross = Fixed::ZERO;
    let mut item_count = 0usize;
    let mut end = start;

    for (index, child) in children.iter().enumerate().skip(start) {
        if child.style.position == Position::Absolute {
            continue;
        }
        let basis = wrap_basis_main(child, is_row, main_size);
        let candidate = if item_count == 0 {
            basis
        } else {
            used_main + main_gap + basis
        };
        if should_wrap && item_count > 0 && candidate > main_size {
            break;
        }
        used_main = candidate;
        natural_cross = natural_cross.max(natural_cross_size(child, is_row, cross_size));
        item_count += 1;
        end = index + 1;
    }

    Some(Line {
        start,
        end,
        item_count,
        natural_cross,
    })
}

fn layout_absolute(child: &mut LayoutNode, x: Fixed, y: Fixed, w: Fixed, h: Fixed) {
    let abs_x = x + child.style.left.resolve(w).unwrap_or(Fixed::ZERO);
    let abs_y = y + child.style.top.resolve(h).unwrap_or(Fixed::ZERO);
    let abs_w = constrain_size(
        resolve_dimension(child.style.width, w, child.intrinsic_width).unwrap_or(Fixed::ZERO),
        child.style.min_width,
        child.style.max_width,
        w,
        child.intrinsic_width,
    );
    let abs_h = constrain_size(
        resolve_dimension(child.style.height, h, child.intrinsic_height).unwrap_or(Fixed::ZERO),
        child.style.min_height,
        child.style.max_height,
        h,
        child.intrinsic_height,
    );
    compute_node(child, abs_x, abs_y, abs_w, abs_h, false);
}

fn layout_line(children: &mut [LayoutNode], layout: LineLayout) {
    let mut fixed_total = Fixed::ZERO;
    let mut grow_total = Fixed::ZERO;
    for child in children.iter() {
        if child.style.position == Position::Absolute {
            continue;
        }
        if flexible(child, layout.is_row) {
            grow_total += child.style.grow;
        } else {
            fixed_total += wrap_basis_main(child, layout.is_row, layout.main_size);
        }
    }

    let gap_total = if layout.item_count > 1 {
        layout.main_gap * (layout.item_count as i32 - 1)
    } else {
        Fixed::ZERO
    };
    let remaining = (layout.main_size - fixed_total - gap_total).max(Fixed::ZERO);
    let flex_unit = resolve_flex_unit(
        children,
        layout.is_row,
        layout.main_size,
        remaining,
        grow_total,
    );

    let mut total_main = gap_total;
    for child in children.iter() {
        if child.style.position != Position::Absolute {
            total_main += base_main_size(child, layout.is_row, layout.main_size, flex_unit);
        }
    }
    let shrink_plan = resolve_shrink_plan(
        children,
        layout.is_row,
        layout.main_size,
        flex_unit,
        (total_main - layout.main_size).max(Fixed::ZERO),
    );

    total_main = gap_total;
    let mut shrink = shrink_plan.cursor();
    for child in children.iter() {
        if child.style.position != Position::Absolute {
            total_main += child_size(
                child,
                layout.is_row,
                layout.main_size,
                layout.cross_size,
                layout.line_cross,
                flex_unit,
                &mut shrink,
            )
            .0;
        }
    }

    let free_space = (layout.main_size - total_main).max(Fixed::ZERO);
    let (mut offset, distributed_gap) =
        justify_spacing(layout.justify, free_space, layout.item_count);
    shrink = shrink_plan.cursor();
    for child in children {
        if child.style.position == Position::Absolute {
            continue;
        }
        let (main, cross) = child_size(
            child,
            layout.is_row,
            layout.main_size,
            layout.cross_size,
            layout.line_cross,
            flex_unit,
            &mut shrink,
        );
        let item_cross_offset = match layout.align {
            AlignItems::FlexStart | AlignItems::Stretch => Fixed::ZERO,
            AlignItems::FlexEnd => layout.line_cross - cross,
            AlignItems::Center => (layout.line_cross - cross) / 2,
        };
        let (x, y, w, h) = if layout.is_row {
            (
                layout.inner_x + offset,
                layout.inner_y + layout.line_offset + item_cross_offset,
                main,
                cross,
            )
        } else {
            (
                layout.inner_x + layout.line_offset + item_cross_offset,
                layout.inner_y + offset,
                cross,
                main,
            )
        };
        compute_node(child, x, y, w, h, false);
        offset += main + layout.main_gap + distributed_gap;
    }
}

fn justify_spacing(
    justify: JustifyContent,
    free_space: Fixed,
    item_count: usize,
) -> (Fixed, Fixed) {
    match justify {
        JustifyContent::FlexStart => (Fixed::ZERO, Fixed::ZERO),
        JustifyContent::FlexEnd => (free_space, Fixed::ZERO),
        JustifyContent::Center => (free_space / 2, Fixed::ZERO),
        JustifyContent::SpaceBetween if item_count > 1 => {
            (Fixed::ZERO, free_space / (item_count as i32 - 1))
        }
        JustifyContent::SpaceAround if item_count > 0 => {
            let gap = free_space / item_count as i32;
            (gap / 2, gap)
        }
        JustifyContent::SpaceEvenly => {
            let gap = free_space / (item_count as i32 + 1);
            (gap, gap)
        }
        JustifyContent::SpaceBetween | JustifyContent::SpaceAround => (Fixed::ZERO, Fixed::ZERO),
    }
}

fn child_size(
    child: &LayoutNode,
    is_row: bool,
    main_size: Fixed,
    cross_size: Fixed,
    line_cross: Fixed,
    flex_unit: Fixed,
    shrink: &mut ShrinkCursor,
) -> (Fixed, Fixed) {
    let base_main = base_main_size(child, is_row, main_size, flex_unit);
    let main = shrink.apply(child, is_row, main_size, base_main);

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
        resolve_dimension(cross_dimension, cross_size, cross_intrinsic).unwrap_or(line_cross),
        min_cross,
        max_cross,
        cross_size,
        cross_intrinsic,
    );
    (main, cross)
}

fn wrap_basis_main(child: &LayoutNode, is_row: bool, main_size: Fixed) -> Fixed {
    if flexible(child, is_row) {
        constrain_main(child, is_row, Fixed::ZERO, main_size)
    } else {
        constrain_main(
            child,
            is_row,
            natural_main_size(child, is_row, main_size).unwrap_or(Fixed::ZERO),
            main_size,
        )
    }
}

fn natural_cross_size(child: &LayoutNode, is_row: bool, cross_size: Fixed) -> Fixed {
    let (dimension, min, max, intrinsic) = if is_row {
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
    let natural = match dimension {
        Dimension::Auto | Dimension::Content => intrinsic.unwrap_or(Fixed::ZERO),
        Dimension::Px(_) | Dimension::Percent(_) => {
            dimension.resolve(cross_size).unwrap_or(Fixed::ZERO)
        }
    };
    constrain_size(natural, min, max, cross_size, intrinsic)
}

fn base_main_size(child: &LayoutNode, is_row: bool, main_size: Fixed, flex_unit: Fixed) -> Fixed {
    if flexible(child, is_row) {
        constrain_main(child, is_row, flex_unit * child.style.grow, main_size)
    } else {
        constrain_main(
            child,
            is_row,
            natural_main_size(child, is_row, main_size).unwrap_or(Fixed::ZERO),
            main_size,
        )
    }
}

#[derive(Clone, Copy, Default)]
struct ShrinkPlan {
    fraction: Fixed,
    active_weight: Fixed,
    active_reduction: Fixed,
}

impl ShrinkPlan {
    fn cursor(self) -> ShrinkCursor {
        ShrinkCursor {
            plan: self,
            remaining_weight: self.active_weight,
            remaining_reduction: self.active_reduction,
        }
    }
}

struct ShrinkCursor {
    plan: ShrinkPlan,
    remaining_weight: Fixed,
    remaining_reduction: Fixed,
}

impl ShrinkCursor {
    fn apply(&mut self, child: &LayoutNode, is_row: bool, main_size: Fixed, base: Fixed) -> Fixed {
        let factor = child.style.shrink.max(Fixed::ZERO);
        let (min, _) = main_bounds(child, is_row, main_size);
        if factor <= Fixed::ZERO || base <= min || self.plan.fraction <= Fixed::ZERO {
            return base;
        }

        let weight = base * factor;
        if base - weight * self.plan.fraction < min {
            return min;
        }

        let reduction = if weight >= self.remaining_weight {
            self.remaining_reduction
        } else {
            let numerator = Fixed64::from(self.remaining_reduction) * Fixed64::from(weight);
            Fixed::from(numerator / Fixed64::from(self.remaining_weight))
        }
        .min(base - min);
        self.remaining_weight -= weight;
        self.remaining_reduction -= reduction;
        constrain_main(
            child,
            is_row,
            (base - reduction).max(Fixed::ZERO),
            main_size,
        )
    }
}

fn resolve_shrink_plan(
    children: &[LayoutNode],
    is_row: bool,
    main_size: Fixed,
    flex_unit: Fixed,
    overflow: Fixed,
) -> ShrinkPlan {
    if overflow <= Fixed::ZERO {
        return ShrinkPlan::default();
    }

    let mut active_weight = Fixed::ZERO;
    for child in children {
        if child.style.position == Position::Absolute || child.style.shrink <= Fixed::ZERO {
            continue;
        }
        let base = base_main_size(child, is_row, main_size, flex_unit);
        let (min, _) = main_bounds(child, is_row, main_size);
        if base > min {
            active_weight += base * child.style.shrink;
        }
    }
    if active_weight <= Fixed::ZERO {
        return ShrinkPlan::default();
    }

    let mut fraction = overflow / active_weight;
    for _ in 0..children.len() {
        let mut frozen_reduction = Fixed::ZERO;
        active_weight = Fixed::ZERO;
        for child in children {
            if child.style.position == Position::Absolute || child.style.shrink <= Fixed::ZERO {
                continue;
            }
            let base = base_main_size(child, is_row, main_size, flex_unit);
            let (min, _) = main_bounds(child, is_row, main_size);
            if base <= min {
                continue;
            }
            let weight = base * child.style.shrink;
            if base - weight * fraction < min {
                frozen_reduction += base - min;
            } else {
                active_weight += weight;
            }
        }
        if active_weight <= Fixed::ZERO {
            break;
        }
        let next = (overflow - frozen_reduction).max(Fixed::ZERO) / active_weight;
        if next == fraction {
            break;
        }
        fraction = next;
    }

    let mut frozen_reduction = Fixed::ZERO;
    active_weight = Fixed::ZERO;
    for child in children {
        if child.style.position == Position::Absolute || child.style.shrink <= Fixed::ZERO {
            continue;
        }
        let base = base_main_size(child, is_row, main_size, flex_unit);
        let (min, _) = main_bounds(child, is_row, main_size);
        if base <= min {
            continue;
        }
        let weight = base * child.style.shrink;
        if base - weight * fraction < min {
            frozen_reduction += base - min;
        } else {
            active_weight += weight;
        }
    }
    ShrinkPlan {
        fraction,
        active_weight,
        active_reduction: (overflow - frozen_reduction).max(Fixed::ZERO),
    }
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
