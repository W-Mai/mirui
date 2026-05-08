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
    pub width: Dimension,
    pub height: Dimension,
    pub grow: Fixed,
    pub position: Position,
    pub left: Dimension,
    pub top: Dimension,
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
}

impl LayoutNode {
    pub fn new(style: LayoutStyle) -> Self {
        Self {
            style,
            children: Vec::new(),
            rect: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
        }
    }

    pub fn add_child(&mut self, child: LayoutNode) {
        self.children.push(child);
    }
}
