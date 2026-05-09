use crate::types::{Fixed, Point, Rect};
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

    /// Conservative bounding box (includes Bezier control points, not the
    /// true curve extrema). Returns None for empty paths.
    pub fn bbox(&self) -> Option<Rect> {
        let mut xmin = Fixed::MAX;
        let mut ymin = Fixed::MAX;
        let mut xmax = Fixed::MIN;
        let mut ymax = Fixed::MIN;
        let mut seen = false;

        let mut visit = |p: &Point| {
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

        for cmd in &self.cmds {
            match cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => visit(p),
                PathCmd::QuadTo { ctrl, end } => {
                    visit(ctrl);
                    visit(end);
                }
                PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                    visit(ctrl1);
                    visit(ctrl2);
                    visit(end);
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

    /// Construct a stroked arc path on the circle (center, radius) sweeping
    /// from start_angle to end_angle. Angles are in degrees, CCW from +X axis.
    /// Emits one cubic Bezier per ≤90° segment using k = 4/3 · tan(θ/4).
    pub fn arc(center: Point, radius: Fixed, start_angle: Fixed, end_angle: Fixed) -> Self {
        let mut p = Self::new();
        if radius <= Fixed::ZERO {
            return p;
        }

        let sweep = end_angle - start_angle;
        if sweep == Fixed::ZERO {
            return p;
        }

        let on_circle = |angle_deg: Fixed| -> Point {
            Point {
                x: center.x + Fixed::cos_deg(angle_deg) * radius,
                y: center.y + Fixed::sin_deg(angle_deg) * radius,
            }
        };

        p.move_to(on_circle(start_angle));

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

            p.cubic_to(p1, p2, p3);

            a = a_next;
            remaining -= ninety;
        }

        p
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
mod tests {
    use super::*;

    #[test]
    fn bbox_empty_returns_none() {
        let p = Path::new();
        assert!(p.bbox().is_none());
    }

    #[test]
    fn bbox_rect_matches_source() {
        let p = Path::rect(
            Fixed::from_int(10),
            Fixed::from_int(20),
            Fixed::from_int(30),
            Fixed::from_int(40),
        );
        let bb = p.bbox().unwrap();
        assert_eq!(bb.x.to_int(), 10);
        assert_eq!(bb.y.to_int(), 20);
        assert_eq!(bb.w.to_int(), 30);
        assert_eq!(bb.h.to_int(), 40);
    }

    #[test]
    fn bbox_rounded_rect_is_conservative_superset() {
        let p = Path::rounded_rect(
            Fixed::from_int(0),
            Fixed::from_int(0),
            Fixed::from_int(20),
            Fixed::from_int(20),
            Fixed::from_int(4),
        );
        let bb = p.bbox().unwrap();
        // Control points of quad corners coincide with the rect corners,
        // so bbox must equal [0,0,20,20] exactly (not larger).
        assert_eq!(bb.x.to_int(), 0);
        assert_eq!(bb.y.to_int(), 0);
        assert_eq!(bb.w.to_int(), 20);
        assert_eq!(bb.h.to_int(), 20);
    }

    #[test]
    fn arc_empty_for_zero_sweep() {
        let p = Path::arc(
            Point {
                x: Fixed::from_int(50),
                y: Fixed::from_int(50),
            },
            Fixed::from_int(10),
            Fixed::from_int(45),
            Fixed::from_int(45),
        );
        assert!(p.cmds.is_empty());
    }

    #[test]
    fn arc_empty_for_zero_radius() {
        let p = Path::arc(
            Point::ZERO,
            Fixed::ZERO,
            Fixed::from_int(0),
            Fixed::from_int(90),
        );
        assert!(p.cmds.is_empty());
    }

    #[test]
    fn arc_quarter_circle_starts_and_ends_on_circle() {
        let center = Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(50),
        };
        let r = Fixed::from_int(20);
        let p = Path::arc(center, r, Fixed::from_int(0), Fixed::from_int(90));

        let eps = Fixed::from_f32(0.1);

        let PathCmd::MoveTo(start) = &p.cmds[0] else {
            panic!("expected MoveTo first")
        };
        // 0° → (center + r, center)
        assert!((start.x - (center.x + r)).abs() <= eps);
        assert!((start.y - center.y).abs() <= eps);

        // 90° → (center, center + r)
        let PathCmd::CubicTo { end, .. } = p.cmds.last().unwrap() else {
            panic!("expected CubicTo last")
        };
        assert!((end.x - center.x).abs() <= eps);
        assert!((end.y - (center.y + r)).abs() <= eps);
    }

    #[test]
    fn arc_full_circle_uses_four_segments() {
        let p = Path::arc(
            Point::ZERO,
            Fixed::from_int(10),
            Fixed::from_int(0),
            Fixed::from_int(360),
        );
        // 1 MoveTo + 4 CubicTo
        assert_eq!(p.cmds.len(), 5);
        let cubic_count = p
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::CubicTo { .. }))
            .count();
        assert_eq!(cubic_count, 4);
    }

    #[test]
    fn arc_negative_sweep_goes_clockwise() {
        let p = Path::arc(
            Point::ZERO,
            Fixed::from_int(10),
            Fixed::from_int(0),
            Fixed::from_int(-90),
        );
        assert!(!p.cmds.is_empty());
        let PathCmd::CubicTo { end, .. } = p.cmds.last().unwrap() else {
            panic!()
        };
        // -90° → (0, -10)
        let eps = Fixed::from_f32(0.1);
        assert!(end.x.abs() <= eps);
        assert!((end.y - Fixed::from_int(-10)).abs() <= eps);
    }
}
