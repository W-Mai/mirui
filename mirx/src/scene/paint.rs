use alloc::vec::Vec;

use crate::types::{Color, Fixed, Point, Transform};

#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    Color(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    pub start: Point,
    pub end: Point,
    pub stops: Vec<GradientStop>,
    pub spread: SpreadMode,
    pub units: GradientUnits,
    pub transform: Transform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialGradient {
    pub center: Point,
    pub radius: Fixed,
    pub focal: Point,
    pub focal_radius: Fixed,
    pub stops: Vec<GradientStop>,
    pub spread: SpreadMode,
    pub units: GradientUnits,
    pub transform: Transform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientStop {
    pub offset: Fixed,
    pub color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpreadMode {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientUnits {
    UserSpaceOnUse,
    ObjectBoundingBox,
}
