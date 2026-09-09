use alloc::vec::Vec;

use crate::types::{Dimension, Fixed, Rect};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Position {
    #[default]
    Flex,
    Absolute,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutStyle {
    pub direction: FlexDirection,
    pub justify: JustifyContent,
    pub align: AlignItems,
    pub padding: Padding,
    pub row_gap: Dimension,
    pub column_gap: Dimension,
    pub width: Dimension,
    pub height: Dimension,
    pub grow: Fixed,
    pub position: Position,
    pub left: Dimension,
    pub top: Dimension,
}

impl LayoutStyle {
    pub fn with_gap(mut self, gap: impl Into<Dimension>) -> Self {
        let gap = gap.into();
        self.row_gap = gap;
        self.column_gap = gap;
        self
    }

    pub fn with_row_gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.row_gap = gap.into();
        self
    }

    pub fn with_column_gap(mut self, gap: impl Into<Dimension>) -> Self {
        self.column_gap = gap.into();
        self
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Padding {
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
    pub left: Dimension,
}

impl Padding {
    pub fn all(v: impl Into<Dimension>) -> Self {
        let v = v.into();
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }
}

pub struct LayoutNode {
    pub style: LayoutStyle,
    pub children: Vec<LayoutNode>,
    pub rect: Rect,
    pub(crate) intrinsic_width: Option<Fixed>,
    pub(crate) intrinsic_height: Option<Fixed>,
}

impl LayoutNode {
    pub fn new(style: LayoutStyle) -> Self {
        Self {
            style,
            children: Vec::new(),
            rect: Rect::ZERO,
            intrinsic_width: None,
            intrinsic_height: None,
        }
    }

    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }

    pub(crate) fn set_intrinsic_size(&mut self, width: Fixed, height: Fixed) {
        self.intrinsic_width = Some(width);
        self.intrinsic_height = Some(height);
    }
}
