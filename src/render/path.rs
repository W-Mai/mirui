use crate::types::{Fixed, Point, Rect};
use alloc::borrow::Cow;
use alloc::vec::Vec;

mod store;

pub(crate) use store::PathSubscription;
pub use store::{PathId, PathRevision, PathStore, PathStoreError};

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path {
    pub(crate) cmds: Cow<'static, [PathCmd]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PathCapacityError {
    Overflow,
    Allocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathCommandError {
    pub index: usize,
    pub len: usize,
}

impl Path {
    pub fn new() -> Self {
        Self {
            cmds: Cow::Owned(Vec::new()),
        }
    }

    pub const fn from_static(cmds: &'static [PathCmd]) -> Self {
        Self {
            cmds: Cow::Borrowed(cmds),
        }
    }

    pub fn from_owned(cmds: Vec<PathCmd>) -> Self {
        Self {
            cmds: Cow::Owned(cmds),
        }
    }

    pub fn try_with_capacity(capacity: usize) -> Result<Self, PathCapacityError> {
        let mut cmds = Vec::new();
        cmds.try_reserve_exact(capacity)
            .map_err(|_| PathCapacityError::Allocation)?;
        Ok(Self {
            cmds: Cow::Owned(cmds),
        })
    }

    pub fn commands(&self) -> &[PathCmd] {
        &self.cmds
    }

    pub fn is_borrowed(&self) -> bool {
        matches!(self.cmds, Cow::Borrowed(_))
    }

    pub fn command_capacity(&self) -> usize {
        match &self.cmds {
            Cow::Borrowed(cmds) => cmds.len(),
            Cow::Owned(cmds) => cmds.capacity(),
        }
    }

    pub fn try_reserve_commands(&mut self, additional: usize) -> Result<(), PathCapacityError> {
        match &mut self.cmds {
            Cow::Owned(cmds) => cmds
                .try_reserve_exact(additional)
                .map_err(|_| PathCapacityError::Allocation),
            Cow::Borrowed(cmds) => {
                let capacity = cmds
                    .len()
                    .checked_add(additional)
                    .ok_or(PathCapacityError::Overflow)?;
                let mut owned = Vec::new();
                owned
                    .try_reserve_exact(capacity)
                    .map_err(|_| PathCapacityError::Allocation)?;
                owned.extend_from_slice(cmds);
                self.cmds = Cow::Owned(owned);
                Ok(())
            }
        }
    }

    pub fn clear(&mut self) -> &mut Self {
        match &mut self.cmds {
            Cow::Owned(cmds) => cmds.clear(),
            Cow::Borrowed(_) => self.cmds = Cow::Owned(Vec::new()),
        }
        self
    }

    pub fn set_command(
        &mut self,
        index: usize,
        command: PathCmd,
    ) -> Result<&mut Self, PathCommandError> {
        let len = self.cmds.len();
        let Some(target) = self.cmds.to_mut().get_mut(index) else {
            return Err(PathCommandError { index, len });
        };
        *target = command;
        Ok(self)
    }

    pub fn move_to(&mut self, p: Point) -> &mut Self {
        self.cmds.to_mut().push(PathCmd::MoveTo(p));
        self
    }

    pub fn line_to(&mut self, p: Point) -> &mut Self {
        self.cmds.to_mut().push(PathCmd::LineTo(p));
        self
    }

    pub fn quad_to(&mut self, ctrl: Point, end: Point) -> &mut Self {
        self.cmds.to_mut().push(PathCmd::QuadTo { ctrl, end });
        self
    }

    pub fn cubic_to(&mut self, ctrl1: Point, ctrl2: Point, end: Point) -> &mut Self {
        self.cmds
            .to_mut()
            .push(PathCmd::CubicTo { ctrl1, ctrl2, end });
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.cmds.to_mut().push(PathCmd::Close);
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

    /// Polygonal outline of an arbitrary quad `q[0..4]` with its four
    /// corners rounded by radius `r`. Corners use the same cubic-bezier
    /// approximation of a 90° arc as `rounded_rect` (k = 4/3 · tan(22.5°)).
    ///
    /// Vertices may be either winding; the produced path matches the
    /// input order and the corners face "inward" geometrically (each
    /// arc sits inside the polygon).
    ///
    /// If `r == 0` or an edge is shorter than `2r`, the path degrades
    /// gracefully: the radius is clamped to half the shortest edge, so
    /// adjacent arcs meet at the edge midpoint rather than overlapping.
    pub fn rounded_quad(q: &[Point; 4], r: Fixed) -> Self {
        Self::from_owned(rounded_quad_commands(q, r).commands().to_vec())
    }

    pub fn rounded_rect(x: Fixed, y: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Self {
        Self::from_owned(rounded_rect_commands(x, y, w, h, r).commands().to_vec())
    }

    /// Conservative bounding box (includes Bezier control points, not the
    /// true curve extrema). Returns None for empty paths.
    pub fn bbox(&self) -> Option<Rect> {
        bbox_of_cmds(&self.cmds)
    }
}

pub(crate) struct PrimitivePath {
    commands: [PathCmd; 10],
    len: usize,
}

impl PrimitivePath {
    pub fn commands(&self) -> &[PathCmd] {
        &self.commands[..self.len]
    }
}

fn polygon_commands(q: &[Point; 4]) -> PrimitivePath {
    PrimitivePath {
        commands: [
            PathCmd::MoveTo(q[0]),
            PathCmd::LineTo(q[1]),
            PathCmd::LineTo(q[2]),
            PathCmd::LineTo(q[3]),
            PathCmd::Close,
            PathCmd::Close,
            PathCmd::Close,
            PathCmd::Close,
            PathCmd::Close,
            PathCmd::Close,
        ],
        len: 5,
    }
}

pub(crate) fn rounded_quad_commands(q: &[Point; 4], r: Fixed) -> PrimitivePath {
    // Clamp radius to half the shortest edge so rounded corners
    // from adjacent edges never overlap.
    let mut min_edge = Fixed::MAX;
    let edges: [(Point, Point); 4] = [(q[0], q[1]), (q[1], q[2]), (q[2], q[3]), (q[3], q[0])];
    let mut edge_lens = [Fixed::ZERO; 4];
    let mut edge_dirs = [Point::ZERO; 4];
    for i in 0..4 {
        let (a, b) = edges[i];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        edge_lens[i] = len;
        if len > Fixed::ZERO {
            edge_dirs[i] = Point {
                x: dx / len,
                y: dy / len,
            };
        }
        if len < min_edge {
            min_edge = len;
        }
    }
    let r = r.min(min_edge / 2).max(Fixed::ZERO);

    if r == Fixed::ZERO {
        return polygon_commands(q);
    }

    // k: bezier control offset for a 90° arc (~0.03% radius error).
    let k = r * Fixed::from_f32(0.552_284_8);

    // Per-edge start/end points, offset r from the corners.
    let mut seg_start = [Point::ZERO; 4];
    let mut seg_end = [Point::ZERO; 4];
    for i in 0..4 {
        let (a, b) = edges[i];
        let ue = edge_dirs[i];
        seg_start[i] = Point {
            x: a.x + ue.x * r,
            y: a.y + ue.y * r,
        };
        seg_end[i] = Point {
            x: b.x - ue.x * r,
            y: b.y - ue.y * r,
        };
    }

    PrimitivePath {
        commands: core::array::from_fn(|index| match index {
            0 => PathCmd::MoveTo(seg_start[0]),
            9 => PathCmd::Close,
            index if index % 2 == 1 => PathCmd::LineTo(seg_end[(index - 1) / 2]),
            index => {
                let i = (index - 2) / 2;
                let j = (i + 1) & 3;
                PathCmd::CubicTo {
                    ctrl1: Point {
                        x: seg_end[i].x + edge_dirs[i].x * k,
                        y: seg_end[i].y + edge_dirs[i].y * k,
                    },
                    ctrl2: Point {
                        x: seg_start[j].x - edge_dirs[j].x * k,
                        y: seg_start[j].y - edge_dirs[j].y * k,
                    },
                    end: seg_start[j],
                }
            }
        }),
        len: 10,
    }
}

pub(crate) fn rounded_rect_commands(
    x: Fixed,
    y: Fixed,
    w: Fixed,
    h: Fixed,
    r: Fixed,
) -> PrimitivePath {
    if r == Fixed::ZERO {
        return polygon_commands(&[
            Point { x, y },
            Point { x: x + w, y },
            Point { x: x + w, y: y + h },
            Point { x, y: y + h },
        ]);
    }
    let r = r.min(w / 2).min(h / 2);
    let k = r * Fixed::from_f32(0.552_284_8);
    let x1 = x + r;
    let x2 = x + w - r;
    let y1 = y + r;
    let y2 = y + h - r;
    PrimitivePath {
        commands: [
            PathCmd::MoveTo(Point { x: x1, y }),
            PathCmd::LineTo(Point { x: x2, y }),
            PathCmd::CubicTo {
                ctrl1: Point { x: x2 + k, y },
                ctrl2: Point {
                    x: x + w,
                    y: y1 - k,
                },
                end: Point { x: x + w, y: y1 },
            },
            PathCmd::LineTo(Point { x: x + w, y: y2 }),
            PathCmd::CubicTo {
                ctrl1: Point {
                    x: x + w,
                    y: y2 + k,
                },
                ctrl2: Point {
                    x: x2 + k,
                    y: y + h,
                },
                end: Point { x: x2, y: y + h },
            },
            PathCmd::LineTo(Point { x: x1, y: y + h }),
            PathCmd::CubicTo {
                ctrl1: Point {
                    x: x1 - k,
                    y: y + h,
                },
                ctrl2: Point { x, y: y2 + k },
                end: Point { x, y: y2 },
            },
            PathCmd::LineTo(Point { x, y: y1 }),
            PathCmd::CubicTo {
                ctrl1: Point { x, y: y1 - k },
                ctrl2: Point { x: x1 - k, y },
                end: Point { x: x1, y },
            },
            PathCmd::Close,
        ],
        len: 10,
    }
}

pub fn bbox_of_cmds(cmds: &[PathCmd]) -> Option<Rect> {
    bbox_of_cmds_transformed(cmds, None)
}

pub fn bbox_of_cmds_transformed(
    cmds: &[PathCmd],
    transform: Option<&crate::types::Transform>,
) -> Option<Rect> {
    let mut xmin = Fixed::MAX;
    let mut ymin = Fixed::MAX;
    let mut xmax = Fixed::MIN;
    let mut ymax = Fixed::MIN;
    let mut seen = false;

    let mut visit = |p: Point| {
        let p = match transform {
            Some(tf) => tf.apply_point(p),
            None => p,
        };
        seen = true;
        if p.x < xmin {
            xmin = p.x;
        }
        if p.x > xmax {
            xmax = p.x;
        }
        if p.y < ymin {
            ymin = p.y;
        }
        if p.y > ymax {
            ymax = p.y;
        }
    };

    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => visit(*p),
            PathCmd::QuadTo { ctrl, end } => {
                visit(*ctrl);
                visit(*end);
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                visit(*ctrl1);
                visit(*ctrl2);
                visit(*end);
            }
            PathCmd::Close => {}
        }
    }

    if !seen {
        return None;
    }
    Some(Rect {
        x: xmin,
        y: ymin,
        w: xmax - xmin,
        h: ymax - ymin,
    })
}

impl Path {
    /// Construct a stroked arc path on the circle (center, radius) sweeping
    /// from start_angle to end_angle. Angles are in degrees, CCW from +X axis.
    /// Emits one cubic Bezier per ≤90° segment using k = 4/3 · tan(θ/4).
    pub fn arc(center: Point, radius: Fixed, start_angle: Fixed, end_angle: Fixed) -> Self {
        let mut p = Self::new();
        p.set_arc(center, radius, start_angle, end_angle);
        p
    }

    pub(crate) fn set_arc(
        &mut self,
        center: Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
    ) {
        self.clear();
        if radius <= Fixed::ZERO {
            return;
        }

        let sweep = end_angle - start_angle;
        if sweep == Fixed::ZERO {
            return;
        }

        let on_circle = |angle_deg: Fixed| -> Point {
            Point {
                x: center.x + Fixed::cos_deg(angle_deg) * radius,
                y: center.y + Fixed::sin_deg(angle_deg) * radius,
            }
        };

        self.move_to(on_circle(start_angle));

        let ninety = Fixed::from_int(90);
        let dir = if sweep > Fixed::ZERO {
            Fixed::ONE
        } else {
            -Fixed::ONE
        };
        let remaining_init = sweep.abs();
        let mut a = start_angle;
        let mut remaining = remaining_init;

        while remaining > Fixed::ZERO {
            let step = remaining.min(ninety) * dir;
            let a_next = a + step;

            // k = 4/3 · tan(step_rad / 4). For arbitrary step θ, tan(θ/4) is
            // approximated by sin(θ/4)/cos(θ/4) — both via Fixed::*_deg.
            let quarter = step / 4;
            let k = Fixed::sin_deg(quarter) / Fixed::cos_deg(quarter) * Fixed::from_int(4) / 3;

            let t0 = tangent_on_circle(a);
            let t1 = tangent_on_circle(a_next);

            let p0 = on_circle(a);
            let p3 = on_circle(a_next);
            let p1 = Point {
                x: p0.x + t0.x * k * radius,
                y: p0.y + t0.y * k * radius,
            };
            let p2 = Point {
                x: p3.x - t1.x * k * radius,
                y: p3.y - t1.y * k * radius,
            };

            self.cubic_to(p1, p2, p3);

            a = a_next;
            remaining -= ninety;
        }
    }
}

/// Unit tangent vector to the circle at angle (degrees), CCW direction.
fn tangent_on_circle(angle_deg: Fixed) -> Point {
    Point {
        x: -Fixed::sin_deg(angle_deg),
        y: Fixed::cos_deg(angle_deg),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/path.rs"]
mod tests;
