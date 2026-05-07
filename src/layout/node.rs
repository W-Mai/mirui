use alloc::vec::Vec;

use crate::types::Rect;

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
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub grow: f32,
    pub position: Position,
    pub left: Option<i32>,
    pub top: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Padding {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Padding {
    pub fn all(v: u16) -> Self {
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
