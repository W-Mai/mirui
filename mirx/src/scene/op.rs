use crate::path::Path;
use crate::types::{Color, Fixed, Point, Rect, Transform};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq)]
pub enum ResourceRef {
    Token(String),
    Index(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    EvenOdd,
    NonZero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositeMode {
    SourceOver,
    Add,
    Screen,
    Multiply,
    Darken,
    Lighten,
    Difference,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SceneOp {
    GroupBegin {
        transform: Option<Transform>,
        opacity: Option<u8>,
        clip: Option<ResourceRef>,
        mask: Option<ResourceRef>,
        filter: Option<ResourceRef>,
        disjoint_hint: bool,
    },
    GroupEnd,
    FillPath {
        path: Path,
        transform: Transform,
        color: Color,
        opa: u8,
        fill_rule: FillRule,
    },
    FillRect {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        radius: Fixed,
        opa: u8,
    },
    Border {
        area: Rect,
        transform: Transform,
        quad: Option<[Point; 4]>,
        color: Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
    },
    Label {
        font: ResourceRef,
        pos: Point,
        transform: Transform,
        color: Color,
        opa: u8,
        text: String,
    },
    Line {
        p1: Point,
        p2: Point,
        transform: Transform,
        color: Color,
        width: Fixed,
        opa: u8,
    },
    Arc {
        center: Point,
        transform: Transform,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
        opa: u8,
    },
    Blit {
        texture: ResourceRef,
        pos: Point,
        size: Point,
        transform: Transform,
        quad: Option<[Point; 4]>,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub ops: Vec<SceneOp>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_ops(ops: Vec<SceneOp>) -> Self {
        Self { ops }
    }
}
