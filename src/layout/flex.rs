use crate::types::Rect;

use super::node::{AlignItems, FlexDirection, JustifyContent, LayoutNode, Position};

pub fn compute_layout(node: &mut LayoutNode, x: i32, y: i32, available_w: u16, available_h: u16) {
    let pad = &node.style.padding;
    let w = node.style.width.unwrap_or(available_w);
    let h = node.style.height.unwrap_or(available_h);

    node.rect = Rect { x, y, w, h };

    if node.children.is_empty() {
        return;
    }

    let inner_w = w.saturating_sub(pad.left + pad.right);
    let inner_h = h.saturating_sub(pad.top + pad.bottom);
    let inner_x = x + pad.left as i32;
    let inner_y = y + pad.top as i32;

    let is_row = node.style.direction == FlexDirection::Row;
    let main_size = if is_row { inner_w } else { inner_h } as f32;
    let cross_size = if is_row { inner_h } else { inner_w };

    // Calculate fixed sizes and total grow (only flex children)
    let mut fixed_total: f32 = 0.0;
    let mut grow_total: f32 = 0.0;
    for child in &node.children {
        if child.style.position == Position::Absolute {
            continue;
        }
        let child_main = if is_row {
            child.style.width
        } else {
            child.style.height
        };
        if let Some(s) = child_main {
            fixed_total += s as f32;
        } else if child.style.grow > 0.0 {
            grow_total += child.style.grow;
        }
    }

    let remaining = (main_size - fixed_total).max(0.0);

    // Compute each child's main axis size
    let child_count = node
        .children
        .iter()
        .filter(|c| c.style.position != Position::Absolute)
        .count();
    let mut sizes: alloc::vec::Vec<(u16, u16)> =
        alloc::vec::Vec::with_capacity(node.children.len());

    for child in &node.children {
        if child.style.position == Position::Absolute {
            sizes.push((0, 0));
            continue;
        }
        let child_main = if is_row {
            child.style.width
        } else {
            child.style.height
        };
        let m = if let Some(s) = child_main {
            s
        } else if child.style.grow > 0.0 && grow_total > 0.0 {
            (remaining * child.style.grow / grow_total) as u16
        } else {
            0
        };

        let child_cross = if is_row {
            child.style.height
        } else {
            child.style.width
        };
        let c = match node.style.align {
            AlignItems::Stretch => child_cross.unwrap_or(cross_size),
            _ => child_cross.unwrap_or(cross_size),
        };

        sizes.push((m, c));
    }

    // Justify: compute starting offset and gap (flex children only)
    let total_main: f32 = sizes
        .iter()
        .enumerate()
        .filter(|(i, _)| node.children[*i].style.position != Position::Absolute)
        .map(|(_, (m, _))| *m as f32)
        .sum();
    let free_space = (main_size - total_main).max(0.0);

    let (mut offset, gap) = match node.style.justify {
        JustifyContent::FlexStart => (0.0, 0.0),
        JustifyContent::FlexEnd => (free_space, 0.0),
        JustifyContent::Center => (free_space / 2.0, 0.0),
        JustifyContent::SpaceBetween => {
            if child_count > 1 {
                (0.0, free_space / (child_count - 1) as f32)
            } else {
                (0.0, 0.0)
            }
        }
        JustifyContent::SpaceAround => {
            let g = free_space / child_count as f32;
            (g / 2.0, g)
        }
        JustifyContent::SpaceEvenly => {
            let g = free_space / (child_count + 1) as f32;
            (g, g)
        }
    };

    // Position children
    for (i, child) in node.children.iter_mut().enumerate() {
        if child.style.position == Position::Absolute {
            // Absolute: position relative to parent's top-left
            let abs_x = x + child.style.left.unwrap_or(0);
            let abs_y = y + child.style.top.unwrap_or(0);
            let abs_w = child.style.width.unwrap_or(0);
            let abs_h = child.style.height.unwrap_or(0);
            compute_layout(child, abs_x, abs_y, abs_w, abs_h);
            continue;
        }

        let (m, c) = sizes[i];

        // Cross axis alignment
        let cross_offset = match node.style.align {
            AlignItems::FlexStart => 0.0,
            AlignItems::FlexEnd => (cross_size - c) as f32,
            AlignItems::Center => (cross_size - c) as f32 / 2.0,
            AlignItems::Stretch => 0.0,
        };

        let (cx, cy, cw, ch) = if is_row {
            (inner_x + offset as i32, inner_y + cross_offset as i32, m, c)
        } else {
            (inner_x + cross_offset as i32, inner_y + offset as i32, c, m)
        };

        compute_layout(child, cx, cy, cw, ch);
        offset += m as f32 + gap;
    }
}
