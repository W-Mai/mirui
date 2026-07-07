use crate::types::Point;

#[derive(Clone, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(Point),
    LineTo(Point),
    QuadTo {
        ctrl: Point,
        end: Point,
    },
    CubicTo {
        ctrl1: Point,
        ctrl2: Point,
        end: Point,
    },
    Close,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub cmds: alloc::vec::Vec<PathCmd>,
}

impl Path {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_cmds(cmds: alloc::vec::Vec<PathCmd>) -> Self {
        Self { cmds }
    }
}
