use alloc::vec::Vec;

use crate::types::{Fixed, Point};

use super::path::{Path, PathCmd};

/// Straight line segment produced by flatten().
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LineSeg {
    pub p1: Point,
    pub p2: Point,
}

/// Polygon fill rule.
#[allow(dead_code)] // NonZero is used by the ttf-parser path; tests verify both rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillRule {
    /// Pixel is inside if a ray from it crosses an odd number of edges.
    /// Default for SVG / rectangles / shapes that don't self-overlap.
    EvenOdd,
    /// Pixel is inside if the signed winding count (downward-going edges
    /// add 1, upward subtract 1) is non-zero. Required for TrueType /
    /// CFF outlines where the same contour can wrap around itself.
    NonZero,
}

/// One contiguous subpath produced by flatten_subpaths().
/// `closed` is true when the subpath ended with a Close command.
#[derive(Clone, Debug)]
pub(crate) struct SubPath {
    pub segs: Vec<LineSeg>,
    pub closed: bool,
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

/// Flatten path into per-subpath groups, tracking whether each ended with
/// Close. stroke_path needs this to decide between offset-ring (closed) and
/// butt-capped strip (open) handling.
pub(crate) fn flatten_subpaths(path: &Path) -> Vec<SubPath> {
    let mut out: Vec<SubPath> = Vec::new();
    let mut subpath_start = Point::ZERO;
    let mut current = Point::ZERO;
    let mut current_segs: Vec<LineSeg> = Vec::new();
    let mut has_moveto = false;

    let flush = |segs: &mut Vec<LineSeg>, out: &mut Vec<SubPath>, closed: bool| {
        if !segs.is_empty() {
            out.push(SubPath {
                segs: core::mem::take(segs),
                closed,
            });
        }
    };

    for cmd in &path.cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                flush(&mut current_segs, &mut out, false);
                subpath_start = *p;
                current = *p;
                has_moveto = true;
            }
            PathCmd::LineTo(p) => {
                if !has_moveto {
                    continue;
                }
                current_segs.push(LineSeg {
                    p1: current,
                    p2: *p,
                });
                current = *p;
            }
            PathCmd::QuadTo { ctrl, end } => {
                if !has_moveto {
                    continue;
                }
                let p0 = current;
                for i in 1..=QUAD_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(QUAD_STEPS);
                    let next = quad_at(p0, *ctrl, *end, t);
                    current_segs.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                if !has_moveto {
                    continue;
                }
                let p0 = current;
                for i in 1..=CUBIC_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(CUBIC_STEPS);
                    let next = cubic_at(p0, *ctrl1, *ctrl2, *end, t);
                    current_segs.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::Close => {
                if current != subpath_start {
                    current_segs.push(LineSeg {
                        p1: current,
                        p2: subpath_start,
                    });
                }
                current = subpath_start;
                flush(&mut current_segs, &mut out, true);
                has_moveto = false;
            }
        }
    }
    flush(&mut current_segs, &mut out, false);
    out
}

/// Vertical supersampling count per pixel row. 4 sub-scanlines gives 5-level
/// (0/4, 1/4, 2/4, 3/4, 4/4) coverage — enough to hide the worst jaggies
/// without making the rasterizer too heavy on ESP32.
const SUB_SCANLINES: i32 = 4;

/// Coverage-based fill rasterizer with 4 sub-scanlines per pixel row.
/// Emits `cov ∈ [0, 1]` per pixel under the chosen [`FillRule`].
pub(crate) fn scanline_fill(
    segs: &[LineSeg],
    px_x0: i32,
    py_y0: i32,
    px_x1: i32,
    py_y1: i32,
    rule: FillRule,
    mut emit: impl FnMut(i32, i32, Fixed),
) {
    if segs.is_empty() || px_x1 <= px_x0 || py_y1 <= py_y0 {
        return;
    }
    let row_w = (px_x1 - px_x0) as usize;
    let mut acc: Vec<Fixed> = alloc::vec![Fixed::ZERO; row_w];
    // (x_intersection, winding) — winding only consulted for NonZero rule.
    let mut crossings: Vec<(Fixed, i8)> = Vec::with_capacity(segs.len());
    let sub_weight = Fixed::ONE / SUB_SCANLINES;

    for py in py_y0..py_y1 {
        for a in acc.iter_mut() {
            *a = Fixed::ZERO;
        }

        for sub in 0..SUB_SCANLINES {
            let y_sample =
                Fixed::from_int(py) + (Fixed::from_int(sub) + Fixed::ONE / 2) / SUB_SCANLINES;

            crossings.clear();
            for s in segs {
                let (y_lo, y_hi, winding) = if s.p1.y <= s.p2.y {
                    (s.p1.y, s.p2.y, 1i8)
                } else {
                    (s.p2.y, s.p1.y, -1i8)
                };
                if y_sample < y_lo || y_sample >= y_hi {
                    continue;
                }
                let dy = s.p2.y - s.p1.y;
                if dy == Fixed::ZERO {
                    continue;
                }
                let t = (y_sample - s.p1.y) / dy;
                let x_cross = s.p1.x + (s.p2.x - s.p1.x) * t;
                crossings.push((x_cross, winding));
            }

            crossings.sort_by_key(|x| x.0);

            match rule {
                FillRule::EvenOdd => {
                    let mut i = 0;
                    while i + 1 < crossings.len() {
                        let xa = crossings[i].0;
                        let xb = crossings[i + 1].0;
                        accumulate_interval(&mut acc, px_x0, px_x1, xa, xb, sub_weight);
                        i += 2;
                    }
                }
                FillRule::NonZero => {
                    let mut count: i32 = 0;
                    let mut span_start: Option<Fixed> = None;
                    for (x, winding) in crossings.iter() {
                        let prev = count;
                        count += *winding as i32;
                        let was_inside = prev != 0;
                        let now_inside = count != 0;
                        match (was_inside, now_inside) {
                            (false, true) => span_start = Some(*x),
                            (true, false) => {
                                if let Some(start) = span_start.take() {
                                    accumulate_interval(
                                        &mut acc, px_x0, px_x1, start, *x, sub_weight,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        for (i, cov) in acc.iter().enumerate() {
            if *cov > Fixed::ZERO {
                emit(px_x0 + i as i32, py, *cov);
            }
        }
    }
}

/// Add `weight` × (fraction of each pixel covered by [xa, xb]) into `acc`.
fn accumulate_interval(
    acc: &mut [Fixed],
    px_x0: i32,
    px_x1: i32,
    xa: Fixed,
    xb: Fixed,
    weight: Fixed,
) {
    let (xlo, xhi) = if xa <= xb { (xa, xb) } else { (xb, xa) };
    if xhi <= Fixed::from_int(px_x0) || xlo >= Fixed::from_int(px_x1) {
        return;
    }

    let lo_int = xlo.to_int().max(px_x0);
    let hi_int = xhi.ceil().to_int().min(px_x1);

    for px in lo_int..hi_int {
        let pixel_left = Fixed::from_int(px);
        let pixel_right = Fixed::from_int(px + 1);
        let left = xlo.max(pixel_left);
        let right = xhi.min(pixel_right);
        let frac = right - left;
        if frac > Fixed::ZERO {
            acc[(px - px_x0) as usize] += frac * weight;
        }
    }
}

/// Point-in-polygon test via +X ray casting with even-odd rule.
/// Uses the half-open convention [y_min, y_max) to avoid double-counting at
/// shared vertices.
#[allow(dead_code)] // Retained for hit-test use-cases and future rasterizers
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

/// Minimum unsigned distance from `pt` to any segment in `segs`, capped at
/// `cap`. Segments whose AABB is farther than `cap` from `pt` are skipped
/// without any sqrt. Returns `cap` when nothing is within range.
///
/// `cap` is compared in **Fixed space** (not squared) to avoid `Fixed * Fixed`
/// overflow when callers pass `Fixed::MAX` or similar large bounds.
#[allow(dead_code)] // Retained for stroke width hit-testing and future use
pub(crate) fn min_dist_to_segments_capped(pt: Point, segs: &[LineSeg], cap: Fixed) -> Fixed {
    let mut best = cap;
    let mut found = false;
    for s in segs {
        let (xlo, xhi) = if s.p1.x <= s.p2.x {
            (s.p1.x, s.p2.x)
        } else {
            (s.p2.x, s.p1.x)
        };
        let (ylo, yhi) = if s.p1.y <= s.p2.y {
            (s.p1.y, s.p2.y)
        } else {
            (s.p2.y, s.p1.y)
        };
        // Quick Chebyshev reject: if the segment's bbox is strictly farther
        // than `best` on either axis, the true distance is ≥ best too.
        let dx_box = if pt.x < xlo {
            xlo - pt.x
        } else if pt.x > xhi {
            pt.x - xhi
        } else {
            Fixed::ZERO
        };
        let dy_box = if pt.y < ylo {
            ylo - pt.y
        } else if pt.y > yhi {
            pt.y - yhi
        } else {
            Fixed::ZERO
        };
        if dx_box >= best || dy_box >= best {
            continue;
        }
        let d = dist_sq_point_to_segment(pt, s.p1, s.p2).sqrt();
        if d < best {
            best = d;
            found = true;
        }
    }
    if found { best } else { cap }
}

/// Squared distance from point `p` to segment `ab`. Caller decides when to sqrt.
fn dist_sq_point_to_segment(p: Point, a: Point, b: Point) -> Fixed {
    let abx = b.x - a.x;
    let aby = b.y - a.y;
    let len_sq = abx * abx + aby * aby;
    if len_sq == Fixed::ZERO {
        let dx = p.x - a.x;
        let dy = p.y - a.y;
        return dx * dx + dy * dy;
    }
    let apx = p.x - a.x;
    let apy = p.y - a.y;
    let t_raw = (apx * abx + apy * aby) / len_sq;
    let t = t_raw.max(Fixed::ZERO).min(Fixed::ONE);
    let cx = a.x + abx * t;
    let cy = a.y + aby * t;
    let dx = p.x - cx;
    let dy = p.y - cy;
    dx * dx + dy * dy
}

/// Miter limit ratio (Fixed). If the miter extension exceeds this multiple of
/// half_width the join degrades to bevel. 4 is the SVG default.
const MITER_LIMIT: Fixed = Fixed::from_int(4);

/// Build a closed-ring offset polygon around `path` as a new Path.
/// Each subpath becomes one (open) or two (closed) sub-polygons in the output,
/// which `fill_path` evaluates with the even-odd rule to produce the stroke.
pub(crate) fn offset_polygon(path: &Path, width: Fixed) -> Path {
    let mut out = Path::new();
    if width <= Fixed::ZERO {
        return out;
    }
    let half = width / 2;

    for sub in flatten_subpaths(path) {
        if sub.segs.is_empty() {
            continue;
        }

        let n = compute_normals(&sub.segs, half);
        if sub.closed {
            let left = build_ring(&sub.segs, &n, half, /*left=*/ true);
            let mut right = build_ring(&sub.segs, &n, half, /*left=*/ false);
            // Flip winding on the right ring so even-odd carves
            // (outer_area ∖ inner_area) instead of cancelling both regions.
            right.reverse();
            append_closed_polyline(&mut out, &left);
            append_closed_polyline(&mut out, &right);
        } else {
            let left = build_open_rail(&sub.segs, &n, half, /*left=*/ true);
            let right = build_open_rail(&sub.segs, &n, half, /*left=*/ false);
            append_open_ribbon(&mut out, &left, &right);
        }
    }
    out
}

/// Per-segment outward unit normal scaled by `half`. `n[i]` corresponds to
/// `segs[i]`; it's the "left" normal when walking p1→p2 (perp rotated -90°).
fn compute_normals(segs: &[LineSeg], half: Fixed) -> Vec<Point> {
    let mut out = Vec::with_capacity(segs.len());
    for s in segs {
        let dx = s.p2.x - s.p1.x;
        let dy = s.p2.y - s.p1.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq == Fixed::ZERO {
            out.push(Point::ZERO);
            continue;
        }
        let len = len_sq.sqrt();
        let nx = -dy / len * half;
        let ny = dx / len * half;
        out.push(Point { x: nx, y: ny });
    }
    out
}

/// Walk the subpath generating one offset rail. For a closed subpath the rail
/// is itself closed (first point equals last point). `left=true` uses +normal,
/// false uses -normal. Joins use miter with bevel fallback beyond MITER_LIMIT.
fn build_ring(segs: &[LineSeg], n: &[Point], half: Fixed, left: bool) -> Vec<Point> {
    let sign = if left { Fixed::ONE } else { -Fixed::ONE };
    let count = segs.len();
    let mut out = Vec::with_capacity(count + 1);

    for i in 0..count {
        let prev = if i == 0 { count - 1 } else { i - 1 };
        let p_prev = segs[prev];
        let p_curr = segs[i];
        let n_prev = scaled(n[prev], sign);
        let n_curr = scaled(n[i], sign);

        let joint = compute_join(
            p_prev.p2, p_prev.p1, n_prev, p_curr.p1, p_curr.p2, n_curr, half,
        );
        out.push(joint);
    }
    if let Some(&first) = out.first() {
        out.push(first);
    }
    out
}

/// For open subpath: per-segment offset with joins between adjacent pairs, but
/// endpoints keep the raw p1+/-n and p2+/-n (no join, since there's no partner).
fn build_open_rail(segs: &[LineSeg], n: &[Point], half: Fixed, left: bool) -> Vec<Point> {
    let sign = if left { Fixed::ONE } else { -Fixed::ONE };
    let count = segs.len();
    let mut out = Vec::with_capacity(count + 1);

    // Starting endpoint: no join
    let n0 = scaled(n[0], sign);
    out.push(offset(segs[0].p1, n0));

    for i in 1..count {
        let p_prev = segs[i - 1];
        let p_curr = segs[i];
        let n_prev = scaled(n[i - 1], sign);
        let n_curr = scaled(n[i], sign);

        let joint = compute_join(
            p_prev.p2, p_prev.p1, n_prev, p_curr.p1, p_curr.p2, n_curr, half,
        );
        out.push(joint);
    }

    // Trailing endpoint: no join
    let n_last = scaled(n[count - 1], sign);
    out.push(offset(segs[count - 1].p2, n_last));

    out
}

/// Miter join: intersect the two offset lines. If the intersection is beyond
/// MITER_LIMIT * half from the shared corner, fall back to bevel (average of
/// the two offset corner points).
fn compute_join(
    // a->b is the incoming segment, c->d is the outgoing. b and c are the
    // shared corner in original path space (normally b == c but kept separate
    // for generality).
    _a: Point,
    b: Point,
    n_prev: Point,
    c: Point,
    _d: Point,
    n_curr: Point,
    half: Fixed,
) -> Point {
    let p_in = offset(b, n_prev);
    let p_out = offset(c, n_curr);

    // If adjacent offset points nearly coincide, no real corner — use one.
    if approx_eq(p_in, p_out) {
        return p_in;
    }

    // Miter = intersection of line(p_in, direction of incoming) with
    // line(p_out, direction of outgoing). Build direction from original segs.
    let dir_prev = Point {
        x: b.x - _a.x,
        y: b.y - _a.y,
    };
    let dir_curr = Point {
        x: _d.x - c.x,
        y: _d.y - c.y,
    };

    if let Some(miter) = line_intersect(p_in, dir_prev, p_out, dir_curr) {
        let corner = Point {
            x: (b.x + c.x) / 2,
            y: (b.y + c.y) / 2,
        };
        let dx = miter.x - corner.x;
        let dy = miter.y - corner.y;
        let dist_sq = dx * dx + dy * dy;
        let limit = half * MITER_LIMIT;
        if dist_sq <= limit * limit {
            return miter;
        }
    }
    // Bevel fallback: midpoint of the two offset corner points.
    Point {
        x: (p_in.x + p_out.x) / 2,
        y: (p_in.y + p_out.y) / 2,
    }
}

/// Solve p1 + t*d1 = p2 + s*d2 for t. Returns the intersection point, or None
/// when the directions are parallel.
fn line_intersect(p1: Point, d1: Point, p2: Point, d2: Point) -> Option<Point> {
    let denom = d1.x * d2.y - d1.y * d2.x;
    if denom == Fixed::ZERO {
        return None;
    }
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let t = (dx * d2.y - dy * d2.x) / denom;
    Some(Point {
        x: p1.x + d1.x * t,
        y: p1.y + d1.y * t,
    })
}

fn append_closed_polyline(out: &mut Path, pts: &[Point]) {
    if pts.len() < 2 {
        return;
    }
    out.move_to(pts[0]);
    for p in &pts[1..] {
        out.line_to(*p);
    }
    out.close();
}

fn append_open_ribbon(out: &mut Path, left: &[Point], right: &[Point]) {
    if left.is_empty() || right.is_empty() {
        return;
    }
    out.move_to(left[0]);
    for p in &left[1..] {
        out.line_to(*p);
    }
    // Walk right side in reverse so the ribbon stays a simple closed polygon.
    for p in right.iter().rev() {
        out.line_to(*p);
    }
    out.close();
}

fn scaled(p: Point, s: Fixed) -> Point {
    Point {
        x: p.x * s,
        y: p.y * s,
    }
}

fn offset(p: Point, n: Point) -> Point {
    Point {
        x: p.x + n.x,
        y: p.y + n.y,
    }
}

fn approx_eq(a: Point, b: Point) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    // Within ~1/256 pixel in Manhattan distance.
    dx.abs() + dy.abs() < Fixed::ONE / 128
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

    fn big_cap() -> Fixed {
        // Large enough to not affect any test case, small enough that
        // dx_box/dy_box comparisons stay well within i32 Fixed range.
        Fixed::from_int(1000)
    }

    #[test]
    fn dist_on_segment_is_zero() {
        let segs = square_segs();
        let d = min_dist_to_segments_capped(pt(5, 0), &segs, big_cap());
        assert_eq!(d, Fixed::ZERO);
    }

    #[test]
    fn dist_center_to_square_is_half_width() {
        let segs = square_segs();
        let d = min_dist_to_segments_capped(pt(5, 5), &segs, big_cap()).to_f32();
        assert!((d - 5.0).abs() < 0.01, "d = {d}");
    }

    #[test]
    fn dist_beyond_endpoint_uses_endpoint() {
        let segs = vec![LineSeg {
            p1: pt(0, 0),
            p2: pt(10, 0),
        }];
        let d = min_dist_to_segments_capped(pt(15, 0), &segs, big_cap()).to_f32();
        assert!((d - 5.0).abs() < 0.01);
    }

    #[test]
    fn dist_to_degenerate_segment() {
        let segs = vec![LineSeg {
            p1: pt(3, 4),
            p2: pt(3, 4),
        }];
        let d = min_dist_to_segments_capped(pt(0, 0), &segs, big_cap()).to_f32();
        assert!((d - 5.0).abs() < 0.01);
    }

    #[test]
    fn dist_capped_returns_cap_when_far() {
        let segs = vec![LineSeg {
            p1: pt(100, 100),
            p2: pt(110, 100),
        }];
        let cap = Fixed::from_int(5);
        let d = min_dist_to_segments_capped(pt(0, 0), &segs, cap);
        assert_eq!(d, cap);
    }

    #[test]
    fn flatten_subpaths_marks_closed_and_open() {
        // subpath A ends with Close (closed), subpath B is a dangling LineTo (open).
        let mut p = Path::new();
        p.move_to(pt(0, 0))
            .line_to(pt(10, 0))
            .line_to(pt(0, 10))
            .close();
        p.move_to(pt(50, 50)).line_to(pt(60, 50));
        let subs = flatten_subpaths(&p);
        assert_eq!(subs.len(), 2);
        assert!(subs[0].closed);
        assert!(!subs[1].closed);
    }

    #[test]
    fn offset_polygon_rectangle_closed_produces_two_rings() {
        // 10×10 rectangle stroked with width=2. The outline path should have
        // exactly 2 subpaths: outer ring + inner ring.
        let path = Path::rect(
            Fixed::from_int(0),
            Fixed::from_int(0),
            Fixed::from_int(10),
            Fixed::from_int(10),
        );
        let outline = offset_polygon(&path, Fixed::from_int(2));
        let closes = outline
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::Close))
            .count();
        assert_eq!(closes, 2);
    }

    #[test]
    fn offset_polygon_open_line_makes_one_ribbon() {
        // A single open LineTo should yield a single closed ribbon (butt caps).
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        let outline = offset_polygon(&path, Fixed::from_int(2));
        let closes = outline
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::Close))
            .count();
        assert_eq!(closes, 1);
    }

    #[test]
    fn offset_polygon_zero_width_is_empty() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        let outline = offset_polygon(&path, Fixed::ZERO);
        assert!(outline.cmds.is_empty());
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

    fn rect_segs(x0: i32, y0: i32, x1: i32, y1: i32, ccw: bool) -> Vec<LineSeg> {
        // Outer ring CW (default) or CCW. Order matters because winding
        // direction is derived from p1.y vs p2.y in the rasterizer.
        if ccw {
            alloc::vec![
                LineSeg {
                    p1: pt(x0, y0),
                    p2: pt(x0, y1)
                },
                LineSeg {
                    p1: pt(x0, y1),
                    p2: pt(x1, y1)
                },
                LineSeg {
                    p1: pt(x1, y1),
                    p2: pt(x1, y0)
                },
                LineSeg {
                    p1: pt(x1, y0),
                    p2: pt(x0, y0)
                },
            ]
        } else {
            alloc::vec![
                LineSeg {
                    p1: pt(x0, y0),
                    p2: pt(x1, y0)
                },
                LineSeg {
                    p1: pt(x1, y0),
                    p2: pt(x1, y1)
                },
                LineSeg {
                    p1: pt(x1, y1),
                    p2: pt(x0, y1)
                },
                LineSeg {
                    p1: pt(x0, y1),
                    p2: pt(x0, y0)
                },
            ]
        }
    }

    fn count_filled(segs: &[LineSeg], rule: FillRule) -> i32 {
        let mut n = 0;
        scanline_fill(segs, 0, 0, 10, 10, rule, |_, _, cov| {
            if cov > Fixed::from_int(0) {
                n += 1;
            }
        });
        n
    }

    #[test]
    fn fill_rule_even_odd_carves_inner_ring_to_zero() {
        // Outer 0..10 + inner 2..8 (same direction). Even-odd makes the
        // inner ring punch a hole; non-zero would fill solid.
        let mut segs = rect_segs(0, 0, 10, 10, false);
        segs.extend(rect_segs(2, 2, 8, 8, false));
        let lit = count_filled(&segs, FillRule::EvenOdd);
        assert!(lit > 0, "outer ring still fills with hole");
        // Inside hole should be 0; only outer band lit.
        // 10x10 = 100 total, hole 6x6 = 36, ring = 64.
        assert!(lit < 70, "expected hole, got lit={}", lit);
    }

    #[test]
    fn fill_rule_non_zero_fills_overlapping_same_direction_solid() {
        // Outer + inner same winding. Non-zero stays inside (count = 2),
        // entire 10x10 should be lit. Even-odd would punch a hole.
        let mut segs = rect_segs(0, 0, 10, 10, false);
        segs.extend(rect_segs(2, 2, 8, 8, false));
        let lit = count_filled(&segs, FillRule::NonZero);
        assert_eq!(lit, 100, "non-zero with same-winding should fill solid");
    }

    #[test]
    fn fill_rule_non_zero_carves_hole_when_inner_reversed() {
        // Outer CW, inner CCW (opposite winding). Non-zero count cancels
        // inside the inner — TrueType "even-odd-like via path direction".
        let mut segs = rect_segs(0, 0, 10, 10, false);
        segs.extend(rect_segs(2, 2, 8, 8, true));
        let lit = count_filled(&segs, FillRule::NonZero);
        // 10x10 = 100, hole 6x6 = 36, ring = 64.
        assert!(
            (60..70).contains(&lit),
            "non-zero with opposite winding should carve a hole, got lit={}",
            lit
        );
    }
}
