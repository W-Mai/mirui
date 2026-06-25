use crate::types::{Fixed, Point, Rect};
use alloc::borrow::Cow;
use alloc::vec::Vec;

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

#[derive(Clone, Debug, Default)]
pub struct Path {
    pub cmds: Cow<'static, [PathCmd]>,
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
            // Degenerate: plain quad polygon.
            let mut p = Self::new();
            p.move_to(q[0]);
            p.line_to(q[1]);
            p.line_to(q[2]);
            p.line_to(q[3]);
            p.close();
            return p;
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

        let mut p = Self::new();
        p.move_to(seg_start[0]);
        for i in 0..4 {
            // Straight part of edge i.
            p.line_to(seg_end[i]);
            // Rounded corner at q[(i+1) % 4]: arc from edge i's end to
            // edge (i+1)'s start. Control points extend along the
            // incoming / outgoing edge tangents by k.
            let ue_in = edge_dirs[i];
            let j = (i + 1) & 3;
            let ue_out = edge_dirs[j];
            let c1 = Point {
                x: seg_end[i].x + ue_in.x * k,
                y: seg_end[i].y + ue_in.y * k,
            };
            let c2 = Point {
                x: seg_start[j].x - ue_out.x * k,
                y: seg_start[j].y - ue_out.y * k,
            };
            p.cubic_to(c1, c2, seg_start[j]);
        }
        p.close();
        p
    }

    pub fn rounded_rect(x: Fixed, y: Fixed, w: Fixed, h: Fixed, r: Fixed) -> Self {
        if r == Fixed::ZERO {
            return Self::rect(x, y, w, h);
        }
        let r = r.min(w / 2).min(h / 2);
        // k = 4/3 · tan(22.5°) ≈ 0.5523 — cubic-bezier control offset that
        // approximates a 90° circular arc to within ~0.03% of the true radius.
        // Much rounder than the old quad approximation (which was off by ~27%).
        let k = r * Fixed::from_f32(0.552_284_8);
        let mut p = Self::new();

        let x1 = x + r;
        let x2 = x + w - r;
        let y1 = y + r;
        let y2 = y + h - r;

        p.move_to(Point { x: x1, y });
        p.line_to(Point { x: x2, y });
        // Top-right corner: tangents +X at start, +Y at end.
        p.cubic_to(
            Point { x: x2 + k, y },
            Point {
                x: x + w,
                y: y1 - k,
            },
            Point { x: x + w, y: y1 },
        );
        p.line_to(Point { x: x + w, y: y2 });
        // Bottom-right corner: tangents +Y at start, -X at end.
        p.cubic_to(
            Point {
                x: x + w,
                y: y2 + k,
            },
            Point {
                x: x2 + k,
                y: y + h,
            },
            Point { x: x2, y: y + h },
        );
        p.line_to(Point { x: x1, y: y + h });
        // Bottom-left corner: tangents -X at start, -Y at end.
        p.cubic_to(
            Point {
                x: x1 - k,
                y: y + h,
            },
            Point { x, y: y2 + k },
            Point { x, y: y2 },
        );
        p.line_to(Point { x, y: y1 });
        // Top-left corner: tangents -Y at start, +X at end.
        p.cubic_to(
            Point { x, y: y1 - k },
            Point { x: x1 - k, y },
            Point { x: x1, y },
        );
        p.close();

        p
    }

    /// Conservative bounding box (includes Bezier control points, not the
    /// true curve extrema). Returns None for empty paths.
    pub fn bbox(&self) -> Option<Rect> {
        bbox_of_cmds(&self.cmds)
    }
}

pub fn bbox_of_cmds(cmds: &[PathCmd]) -> Option<Rect> {
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

    for cmd in cmds {
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

impl Path {
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

    #[test]
    fn rounded_quad_zero_radius_is_plain_polygon() {
        let q = [
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(10),
            },
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(10),
            },
        ];
        let p = Path::rounded_quad(&q, Fixed::ZERO);
        // Expect MoveTo + 3 × LineTo + Close. No CubicTo.
        let mut has_cubic = false;
        for c in p.cmds.iter() {
            if matches!(c, PathCmd::CubicTo { .. }) {
                has_cubic = true;
            }
        }
        assert!(!has_cubic);
    }

    #[test]
    fn rounded_quad_axis_aligned_matches_rounded_rect_bbox() {
        // When the quad is axis-aligned, its bbox after rounding must
        // equal the input rect (control points of the bezier arcs sit
        // on the rect corners, so bbox doesn't expand).
        let q = [
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(20),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(20),
                y: Fixed::from_int(20),
            },
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(20),
            },
        ];
        let p = Path::rounded_quad(&q, Fixed::from_int(4));
        let bb = p.bbox().unwrap();
        assert_eq!(bb.x.to_int(), 0);
        assert_eq!(bb.y.to_int(), 0);
        assert_eq!(bb.w.to_int(), 20);
        assert_eq!(bb.h.to_int(), 20);
    }

    #[test]
    fn rounded_quad_tilted_still_convex_bbox() {
        // Tilted quad (kite shape). Just check the path builds without
        // panicking and produces 4 arcs.
        let q = [
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(0),
            },
            Point {
                x: Fixed::from_int(20),
                y: Fixed::from_int(10),
            },
            Point {
                x: Fixed::from_int(10),
                y: Fixed::from_int(20),
            },
            Point {
                x: Fixed::from_int(0),
                y: Fixed::from_int(10),
            },
        ];
        let p = Path::rounded_quad(&q, Fixed::from_int(2));
        let cubic_count = p
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::CubicTo { .. }))
            .count();
        assert_eq!(cubic_count, 4);
    }

    // Path::from_static must keep the slice in Cow::Borrowed and never
    // promote to Owned — that's the whole point of the const path for
    // icons / path!-emitted statics on heap-constrained MCUs.
    #[test]
    fn from_static_stays_borrowed() {
        use alloc::borrow::Cow;
        static CMDS: &[PathCmd] = &[
            PathCmd::MoveTo(Point {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
            }),
            PathCmd::Close,
        ];
        let p = Path::from_static(CMDS);
        assert!(matches!(p.cmds, Cow::Borrowed(_)));
        // sanity: iterating doesn't trigger a copy either.
        assert_eq!(p.cmds.iter().count(), 2);
        assert!(matches!(p.cmds, Cow::Borrowed(_)));
    }
}
