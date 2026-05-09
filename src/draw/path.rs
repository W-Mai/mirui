use crate::types::{Fixed, Point};
use alloc::vec::Vec;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, Default)]
pub struct Path {
    pub cmds: Vec<PathCmd>,
}

impl Path {
    pub fn new() -> Self {
        Self { cmds: Vec::new() }
    }

    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.cmds.push(PathCmd::MoveTo(p));
        self
    }

    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.cmds.push(PathCmd::LineTo(p));
        self
    }

    pub fn quad_to(&mut self, ctrl: Point, end: Point) -> &mut Self {
        self.cmds.push(PathCmd::QuadTo { ctrl, end });
        self
    }

    pub fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, end: Point) -> &mut Self {
        self.cmds.push(PathCmd::CubicTo { ctrl1, ctrl2, end });
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.cmds.push(PathCmd::Close);
        self
    }

    pub fn rect(x: Fixed, y: Fixed, w: Fixed, h: Fixed) -> Self {
        let mut p = Self::new();
        let tl = Point { x, y };
        let tr = Point { x: x + w, y };
        let br = Point { x: x + w, y: y + h };
        let bl = Point { x, y: y + h };
        p.move_to(tl).line_to(tr).line_to(br).line_to(bl).close();
        p
    }

    pub fn rounded_rect(x: Fixed, y: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Self {
        if r == Fixed::ZERO {
            return Self::rect(x, y, w, h);
        }
        let r = r.min(w / 2).min(h / 2);
        let mut p = Self::new();

        // Start at top-left after corner
        p.move_to(Point { x: x + r, y });
        // Top edge
        p.line_to(Point { x: x + w - r, y });
        // Top-right corner
        p.quad_to(Point { x: x + w, y }, Point { x: x + w, y: y + r });
        // Right edge
        p.line_to(Point {
            x: x + w,
            y: y + h - r,
        });
        // Bottom-right corner
        p.quad_to(
            Point { x: x + w, y: y + h },
            Point {
                x: x + w - r,
                y: y + h,
            },
        );
        // Bottom edge
        p.line_to(Point { x: x + r, y: y + h });
        // Bottom-left corner
        p.quad_to(Point { x, y: y + h }, Point { x, y: y + h - r });
        // Left edge
        p.line_to(Point { x, y: y + r });
        // Top-left corner
        p.quad_to(Point { x, y }, Point { x: x + r, y });
        p.close();

        p
    }
}
