use alloc::vec::Vec;

use crate::types::{Fixed, Point};

use super::path::{Path, PathCmd};

/// Straight line segment produced by flatten().
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineSeg {
    pub p1: Point,
    pub p2: Point,
}

/// Subdivision step counts. Chosen to keep a single control-point radius
/// visually smooth on 128×128 screens; larger paths may show facets but UI
/// radii are small.
const QUAD_STEPS: i32 = 8;
const CUBIC_STEPS: i32 = 16;

/// Flatten path into a sequence of LineSegs via De Casteljau subdivision.
/// Degenerate zero-length segments are kept — downstream fill_path handles them.
pub(crate) fn flatten(path: &Path) -> Vec<LineSeg> {
    let mut out = Vec::new();
    let mut subpath_start = Point::ZERO;
    let mut current = Point::ZERO;

    for cmd in &path.cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                subpath_start = *p;
                current = *p;
            }
            PathCmd::LineTo(p) => {
                out.push(LineSeg {
                    p1: current,
                    p2: *p,
                });
                current = *p;
            }
            PathCmd::QuadTo { ctrl, end } => {
                let p0 = current;
                for i in 1..=QUAD_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(QUAD_STEPS);
                    let next = quad_at(p0, *ctrl, *end, t);
                    out.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                let p0 = current;
                for i in 1..=CUBIC_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(CUBIC_STEPS);
                    let next = cubic_at(p0, *ctrl1, *ctrl2, *end, t);
                    out.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::Close => {
                if current != subpath_start {
                    out.push(LineSeg {
                        p1: current,
                        p2: subpath_start,
                    });
                }
                current = subpath_start;
            }
        }
    }
    out
}

/// Point-in-polygon test via +X ray casting with even-odd rule.
/// Uses the half-open convention [y_min, y_max) to avoid double-counting at
/// shared vertices.
pub(crate) fn point_in_segments(pt: Point, segs: &[LineSeg]) -> bool {
    let mut crossings = 0u32;
    for s in segs {
        let (y_lo, y_hi) = if s.p1.y <= s.p2.y {
            (s.p1.y, s.p2.y)
        } else {
            (s.p2.y, s.p1.y)
        };
        if pt.y < y_lo || pt.y >= y_hi {
            continue;
        }
        // x of the edge at this y-row. Skip horizontal edges (can't intersect).
        let dy = s.p2.y - s.p1.y;
        if dy == Fixed::ZERO {
            continue;
        }
        let t = (pt.y - s.p1.y) / dy;
        let x_cross = s.p1.x + (s.p2.x - s.p1.x) * t;
        if x_cross > pt.x {
            crossings += 1;
        }
    }
    crossings & 1 == 1
}

/// Minimum unsigned distance from `pt` to any segment in `segs`. Empty returns
/// Fixed::MAX so callers treat it as "infinitely far".
pub(crate) fn min_dist_to_segments(pt: Point, segs: &[LineSeg]) -> Fixed {
    let mut best = Fixed::MAX;
    for s in segs {
        let d = dist_point_to_segment(pt, s.p1, s.p2);
        if d < best {
            best = d;
        }
    }
    best
}

fn dist_point_to_segment(p: Point, a: Point, b: Point) -> Fixed {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len_sq = abx * abx + aby * aby;
    if len_sq == Fixed::ZERO {
        // Degenerate segment — distance to the single point a.
        let dx = p.x - a.x;
        let dy = p.y - a.y;
        return (dx * dx + dy * dy).sqrt();
    }
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let t_raw = (apx * abx + apy * aby) / len_sq;
    let t = t_raw.max(Fixed::ZERO).min(Fixed::ONE);
    let cx = a.x + abx * t;
    let cy = a.y + aby * t;
    let dx = p.x - cx;
    let dy = p.y - cy;
    (dx * dx + dy * dy).sqrt()
}

fn lerp(a: Point, b: Point, t: Fixed) -> Point {
    Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    }
}

fn quad_at(p0: Point, p1: Point, p2: Point, t: Fixed) -> Point {
    lerp(lerp(p0, p1, t), lerp(p1, p2, t), t)
}

fn cubic_at(p0: Point, p1: Point, p2: Point, p3: Point, t: Fixed) -> Point {
    let q0 = lerp(p0, p1, t);
    let q1 = lerp(p1, p2, t);
    let q2 = lerp(p2, p3, t);
    lerp(lerp(q0, q1, t), lerp(q1, q2, t), t)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn pt(x: i32, y: i32) -> Point {
        Point {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
        }
    }

    #[test]
    fn flatten_empty_path() {
        assert!(flatten(&Path::new()).is_empty());
    }

    #[test]
    fn flatten_single_line() {
        let mut p = Path::new();
        p.move_to(pt(0, 0)).line_to(pt(10, 0));
        let segs = flatten(&p);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].p1, pt(0, 0));
        assert_eq!(segs[0].p2, pt(10, 0));
    }

    #[test]
    fn flatten_closed_triangle_emits_closing_edge() {
        let mut p = Path::new();
        p.move_to(pt(0, 0))
            .line_to(pt(10, 0))
            .line_to(pt(5, 10))
            .close();
        let segs = flatten(&p);
        // 3 explicit lines + 1 implicit close back to (0,0)
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[2].p2, pt(0, 0));
    }

    #[test]
    fn flatten_close_is_noop_when_current_equals_start() {
        let mut p = Path::new();
        p.move_to(pt(0, 0))
            .line_to(pt(10, 0))
            .line_to(pt(0, 0))
            .close();
        let segs = flatten(&p);
        // close() sees current == subpath_start, so it must not add a redundant edge
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn flatten_quad_emits_n_segments_connected() {
        let mut p = Path::new();
        p.move_to(pt(0, 0)).quad_to(pt(10, 10), pt(20, 0));
        let segs = flatten(&p);
        assert_eq!(segs.len(), QUAD_STEPS as usize);
        // Chain: seg[i].p2 == seg[i+1].p1
        for i in 0..segs.len() - 1 {
            assert_eq!(segs[i].p2, segs[i + 1].p1);
        }
        assert_eq!(segs.last().unwrap().p2, pt(20, 0));
    }

    #[test]
    fn flatten_cubic_emits_n_segments_connected() {
        let mut p = Path::new();
        p.move_to(pt(0, 0))
            .cubic_to(pt(0, 10), pt(10, 10), pt(10, 0));
        let segs = flatten(&p);
        assert_eq!(segs.len(), CUBIC_STEPS as usize);
        for i in 0..segs.len() - 1 {
            assert_eq!(segs[i].p2, segs[i + 1].p1);
        }
        assert_eq!(segs.last().unwrap().p2, pt(10, 0));
    }

    #[test]
    fn flatten_multi_subpath() {
        // Two independent subpaths: a line then a separate triangle.
        let mut p = Path::new();
        p.move_to(pt(0, 0)).line_to(pt(10, 0));
        p.move_to(pt(50, 50)).line_to(pt(60, 50)).close();
        let segs = flatten(&p);
        // Subpath 1: 1 line. Subpath 2: 1 explicit + 1 close = 2.
        assert_eq!(segs.len(), 3);
        // Second subpath closes back to (50,50), not (0,0)
        assert_eq!(segs.last().unwrap().p2, pt(50, 50));
    }

    fn square_segs() -> Vec<LineSeg> {
        // Unit square [0, 10] × [0, 10] — 4 edges CCW.
        vec![
            LineSeg {
                p1: pt(0, 0),
                p2: pt(10, 0),
            },
            LineSeg {
                p1: pt(10, 0),
                p2: pt(10, 10),
            },
            LineSeg {
                p1: pt(10, 10),
                p2: pt(0, 10),
            },
            LineSeg {
                p1: pt(0, 10),
                p2: pt(0, 0),
            },
        ]
    }

    #[test]
    fn point_in_square_interior_detected() {
        assert!(point_in_segments(pt(5, 5), &square_segs()));
    }

    #[test]
    fn point_outside_square_rejected() {
        assert!(!point_in_segments(pt(-1, 5), &square_segs()));
        assert!(!point_in_segments(pt(11, 5), &square_segs()));
        assert!(!point_in_segments(pt(5, -1), &square_segs()));
        assert!(!point_in_segments(pt(5, 11), &square_segs()));
    }

    #[test]
    fn point_on_horizontal_edge_half_open() {
        // Half-open [y_lo, y_hi) — bottom edge is inclusive, top edge is exclusive.
        assert!(point_in_segments(pt(5, 0), &square_segs()));
        assert!(!point_in_segments(pt(5, 10), &square_segs()));
    }

    #[test]
    fn dist_on_segment_is_zero() {
        let segs = square_segs();
        let d = min_dist_to_segments(pt(5, 0), &segs);
        assert_eq!(d, Fixed::ZERO);
    }

    #[test]
    fn dist_center_to_square_is_half_width() {
        let segs = square_segs();
        let d = min_dist_to_segments(pt(5, 5), &segs).to_f32();
        assert!((d - 5.0).abs() < 0.01, "d = {d}");
    }

    #[test]
    fn dist_beyond_endpoint_uses_endpoint() {
        // Single segment from (0,0) to (10,0). Query (15, 0) — past end.
        // Closest point must be (10, 0), distance = 5.
        let segs = vec![LineSeg {
            p1: pt(0, 0),
            p2: pt(10, 0),
        }];
        let d = min_dist_to_segments(pt(15, 0), &segs).to_f32();
        assert!((d - 5.0).abs() < 0.01);
    }

    #[test]
    fn dist_to_degenerate_segment() {
        let segs = vec![LineSeg {
            p1: pt(3, 4),
            p2: pt(3, 4),
        }];
        let d = min_dist_to_segments(pt(0, 0), &segs).to_f32();
        assert!((d - 5.0).abs() < 0.01);
    }

    #[test]
    fn flatten_rounded_rect_produces_expected_count() {
        let p = Path::rounded_rect(
            Fixed::from_int(0),
            Fixed::from_int(0),
            Fixed::from_int(20),
            Fixed::from_int(20),
            Fixed::from_int(4),
        );
        let segs = flatten(&p);
        // rounded_rect = 4 lines + 4 quad corners (N=8 each) + possibly 1 close.
        // Layout: move, line, quad, line, quad, line, quad, line, quad, close.
        // Close may add 0 or 1 edge depending on whether final point equals start.
        let expected_min = 4 + 4 * QUAD_STEPS as usize;
        assert!(
            segs.len() >= expected_min,
            "got {} segs, expected ≥{}",
            segs.len(),
            expected_min
        );
    }
}
