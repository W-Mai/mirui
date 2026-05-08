use crate::types::{Fixed, Rect};

use super::node::{AlignItems, FlexDirection, JustifyContent, LayoutNode, Position};

pub fn compute_layout(node: &mut LayoutNode, x: i32, y: i32, available_w: u16, available_h: u16) {
    let avail_w = Fixed::from_int(available_w as i32);
    let avail_h = Fixed::from_int(available_h as i32);

    let w = node.style.width.resolve(avail_w).unwrap_or(avail_w);
    let h = node.style.height.resolve(avail_h).unwrap_or(avail_h);

    node.rect = Rect {
        x,
        y,
        w: w.to_int() as u16,
        h: h.to_int() as u16,
    };

    if node.children.is_empty() {
        return;
    }

    let pad_l = node.style.padding.left.resolve(w).unwrap_or(Fixed::ZERO);
    let pad_r = node.style.padding.right.resolve(w).unwrap_or(Fixed::ZERO);
    let pad_t = node.style.padding.top.resolve(h).unwrap_or(Fixed::ZERO);
    let pad_b = node.style.padding.bottom.resolve(h).unwrap_or(Fixed::ZERO);

    let inner_w = (w - pad_l - pad_r).max(Fixed::ZERO);
    let inner_h = (h - pad_t - pad_b).max(Fixed::ZERO);
    let inner_x = x + pad_l.to_int();
    let inner_y = y + pad_t.to_int();

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
        let child_main = if is_row {
            child.style.width.resolve(main_size)
        } else {
            child.style.height.resolve(main_size)
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
        let child_main = if is_row {
            child.style.width.resolve(main_size)
        } else {
            child.style.height.resolve(main_size)
        };
        let m = if let Some(s) = child_main {
            s
        } else if child.style.grow > Fixed::ZERO && grow_total > Fixed::ZERO {
            remaining * child.style.grow / grow_total
        } else {
            Fixed::ZERO
        };

        let child_cross = if is_row {
            child.style.height.resolve(cross_size)
        } else {
            child.style.width.resolve(cross_size)
        };
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
            let abs_x = x + child.style.left.resolve(w).unwrap_or(Fixed::ZERO).to_int();
            let abs_y = y + child.style.top.resolve(h).unwrap_or(Fixed::ZERO).to_int();
            let abs_w = child.style.width.resolve(w).unwrap_or(Fixed::ZERO).to_int() as u16;
            let abs_h = child
                .style
                .height
                .resolve(h)
                .unwrap_or(Fixed::ZERO)
                .to_int() as u16;
            compute_layout(child, abs_x, abs_y, abs_w, abs_h);
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
            (
                inner_x + (offset).to_int(),
                inner_y + cross_offset.to_int(),
                m.to_int() as u16,
                c.to_int() as u16,
            )
        } else {
            (
                inner_x + cross_offset.to_int(),
                inner_y + (offset).to_int(),
                c.to_int() as u16,
                m.to_int() as u16,
            )
        };

        compute_layout(child, cx, cy, cw, ch);
        offset += m + gap;
    }
}
