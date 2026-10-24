//! Conservative solve scope collected while refreshing prepared layout inputs.

use super::{LayoutNode, Position, flex};
use crate::{ecs::Entity, types::Dimension};

#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) enum SolveScope {
    #[default]
    Unchanged,
    Leaf(Entity),
    Full,
}

impl SolveScope {
    pub(crate) fn observe(&mut self, node: &LayoutNode, was_absolute: bool) {
        let Some(entity) = node.entity else {
            *self = Self::Full;
            return;
        };
        *self = match *self {
            Self::Unchanged if was_absolute && eligible(node) => Self::Leaf(entity),
            _ => Self::Full,
        };
    }

    /// Uses the parent's already resolved border rectangle, like the complete solver.
    pub(crate) fn apply(&self, root: &mut LayoutNode) -> bool {
        let Self::Leaf(target) = *self else {
            return false;
        };
        solve_leaf(root, target)
    }
}

fn eligible(node: &LayoutNode) -> bool {
    let style = &node.style;
    let size_known = |value: Dimension, intrinsic: Option<crate::types::Fixed>| {
        matches!(value, Dimension::Px(_) | Dimension::Percent(_)) || intrinsic.is_some()
    };
    let constraint_known = |value: Dimension, intrinsic: Option<crate::types::Fixed>| {
        value != Dimension::Content || intrinsic.is_some()
    };
    style.position == Position::Absolute
        && node.children.is_empty()
        && size_known(style.width, node.intrinsic_width)
        && size_known(style.height, node.intrinsic_height)
        && constraint_known(style.min_width, node.intrinsic_width)
        && constraint_known(style.max_width, node.intrinsic_width)
        && constraint_known(style.min_height, node.intrinsic_height)
        && constraint_known(style.max_height, node.intrinsic_height)
}

fn solve_leaf(node: &mut LayoutNode, target: Entity) -> bool {
    let rect = node.rect;
    for child in &mut node.children {
        if child.entity == Some(target) {
            if !eligible(child) {
                return false;
            }
            flex::layout_absolute(child, rect.x, rect.y, rect.w, rect.h);
            return true;
        }
        if solve_leaf(child, target) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests;
