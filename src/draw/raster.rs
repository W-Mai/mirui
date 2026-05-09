use alloc::vec::Vec;

use crate::types::{Fixed, Point};

use super::path::{Path, PathCmd};

/// Straight line segment produced by flatten().
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Consumed by fill_path in upcoming commit
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
#[allow(dead_code)] // Consumed by fill_path in upcoming commit
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
