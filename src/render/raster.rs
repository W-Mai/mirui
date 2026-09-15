use alloc::vec::Vec;

use crate::types::{Fixed, Point, Transform, Transform3D};

use super::path::{Path, PathCmd};

pub use mirx::scene::{LineCap, LineJoin};

/// Cap so a pathological path can't pin tens of KB on a 200 KB MCU heap.
pub const MAX_SEG_CAP: usize = 2048;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineSeg {
    pub p1: Point,
    pub p2: Point,
}

/// Polygon fill rule.
#[allow(dead_code)] // NonZero is used by the ttf-parser path; tests verify both rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    /// Pixel is inside if a ray from it crosses an odd number of edges.
    /// Default for SVG / rectangles / shapes that don't self-overlap.
    EvenOdd,
    /// Pixel is inside if the signed winding count (downward-going edges
    /// add 1, upward subtract 1) is non-zero. Required for TrueType /
    /// CFF outlines where the same contour can wrap around itself.
    NonZero,
}

/// A range in the caller-owned flattened segment buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubPath {
    pub start: usize,
    pub end: usize,
    pub closed: bool,
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
pub(crate) struct StrokeSpec<'a> {
    pub width: Fixed,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: Fixed,
    pub dash: &'a [Fixed],
    pub dash_scale: Fixed,
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
pub(crate) struct StrokeScratch {
    outline: Path,
    flattened: Vec<LineSeg>,
    subpaths: Vec<SubPath>,
    normals: Vec<Point>,
    rail: Vec<Point>,
    left_rail: Vec<Point>,
    arc: Vec<Point>,
    dashed: Vec<LineSeg>,
    dash_subpaths: Vec<SubPath>,
    dash_lengths: Vec<Fixed>,
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
impl StrokeScratch {
    pub fn new() -> Self {
        Self {
            outline: Path::new(),
            flattened: Vec::new(),
            subpaths: Vec::new(),
            normals: Vec::new(),
            rail: Vec::new(),
            left_rail: Vec::new(),
            arc: Vec::new(),
            dashed: Vec::new(),
            dash_subpaths: Vec::new(),
            dash_lengths: Vec::new(),
        }
    }

    pub fn outline(
        &mut self,
        path: &Path,
        transform: Option<&Transform>,
        spec: StrokeSpec<'_>,
    ) -> &Path {
        self.dash_lengths.clear();
        let dash = if spec.dash_scale == Fixed::ONE {
            spec.dash
        } else {
            self.dash_lengths
                .extend(spec.dash.iter().map(|length| *length * spec.dash_scale));
            &self.dash_lengths
        };
        offset_polygon_into(
            &path.cmds,
            transform,
            spec.width,
            spec.cap,
            spec.join,
            spec.miter_limit,
            (!dash.is_empty()).then_some(dash),
            &mut self.outline,
            &mut self.flattened,
            &mut self.subpaths,
            &mut self.normals,
            &mut self.rail,
            &mut self.left_rail,
            &mut self.arc,
            &mut self.dashed,
            &mut self.dash_subpaths,
        );
        &self.outline
    }
}

/// Subdivision step counts. Chosen to keep a single control-point radius
/// visually smooth on 128×128 screens; larger paths may show facets but UI
/// radii are small.
const QUAD_STEPS: i32 = 8;
const CUBIC_STEPS: i32 = 16;

/// `transform` folds in at emit time so backends can compose
/// `viewport × cmd` once and avoid pre-rebuilding the PathCmd stream.
pub fn flatten_into(cmds: &[PathCmd], transform: Option<&Transform>, out: &mut Vec<LineSeg>) {
    out.clear();
    let mut subpath_start = Point::ZERO;
    let mut current = Point::ZERO;

    let apply = |p: Point| -> Point {
        match transform {
            Some(tf) => tf.apply_point(p),
            None => p,
        }
    };

    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                let p = apply(*p);
                subpath_start = p;
                current = p;
            }
            PathCmd::LineTo(p) => {
                let p = apply(*p);
                out.push(LineSeg { p1: current, p2: p });
                current = p;
            }
            PathCmd::QuadTo { ctrl, end } => {
                let ctrl = apply(*ctrl);
                let end = apply(*end);
                let p0 = current;
                for i in 1..=QUAD_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(QUAD_STEPS);
                    let next = quad_at(p0, ctrl, end, t);
                    out.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                let ctrl1 = apply(*ctrl1);
                let ctrl2 = apply(*ctrl2);
                let end = apply(*end);
                let p0 = current;
                for i in 1..=CUBIC_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(CUBIC_STEPS);
                    let next = cubic_at(p0, ctrl1, ctrl2, end, t);
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

    if out.capacity() > MAX_SEG_CAP {
        out.shrink_to(MAX_SEG_CAP);
    }
}

/// Flatten a path after homographic projection into caller-owned segments.
/// Curve samples are projected individually so perspective does not get
/// approximated by an affine transform of the control polygon.
pub(crate) fn flatten_projective_into(
    cmds: &[PathCmd],
    transform: &Transform3D,
    out: &mut Vec<LineSeg>,
) -> Result<(), ()> {
    out.clear();
    let mut subpath_start = Point::ZERO;
    let mut current = Point::ZERO;
    let mut projected_start = Point::ZERO;
    let mut projected_current = Point::ZERO;
    let mut has_current = false;

    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(point) => {
                let projected = transform.apply_point(*point).ok_or(())?;
                subpath_start = *point;
                current = *point;
                projected_start = projected;
                projected_current = projected;
                has_current = true;
            }
            PathCmd::LineTo(point) if has_current => {
                let projected = transform.apply_point(*point).ok_or(())?;
                out.push(LineSeg {
                    p1: projected_current,
                    p2: projected,
                });
                current = *point;
                projected_current = projected;
            }
            PathCmd::QuadTo { ctrl, end } if has_current => {
                let p0 = current;
                for i in 1..=QUAD_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(QUAD_STEPS);
                    let point = quad_at(p0, *ctrl, *end, t);
                    let projected = transform.apply_point(point).ok_or(())?;
                    out.push(LineSeg {
                        p1: projected_current,
                        p2: projected,
                    });
                    projected_current = projected;
                }
                current = *end;
            }
            PathCmd::CubicTo { ctrl1, ctrl2, end } if has_current => {
                let p0 = current;
                for i in 1..=CUBIC_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(CUBIC_STEPS);
                    let point = cubic_at(p0, *ctrl1, *ctrl2, *end, t);
                    let projected = transform.apply_point(point).ok_or(())?;
                    out.push(LineSeg {
                        p1: projected_current,
                        p2: projected,
                    });
                    projected_current = projected;
                }
                current = *end;
            }
            PathCmd::Close if has_current => {
                if projected_current != projected_start {
                    out.push(LineSeg {
                        p1: projected_current,
                        p2: projected_start,
                    });
                }
                current = subpath_start;
                projected_current = projected_start;
            }
            _ => {}
        }
    }

    if out.capacity() > MAX_SEG_CAP {
        out.shrink_to(MAX_SEG_CAP);
    }
    Ok(())
}

/// Flatten path into per-subpath groups, tracking whether each ended with
/// Close. stroke_path needs this to decide between offset-ring (closed) and
/// butt-capped strip (open) handling.
pub fn flatten_subpaths_into(
    cmds: &[PathCmd],
    transform: Option<&Transform>,
    segments: &mut Vec<LineSeg>,
    out: &mut Vec<SubPath>,
) {
    segments.clear();
    out.clear();
    let mut subpath_start = Point::ZERO;
    let mut current = Point::ZERO;
    let mut start = 0;
    let mut has_moveto = false;

    let apply = |p: Point| -> Point {
        match transform {
            Some(tf) => tf.apply_point(p),
            None => p,
        }
    };

    let flush = |segments: &[LineSeg], out: &mut Vec<SubPath>, start: usize, closed: bool| {
        if segments.len() > start {
            out.push(SubPath {
                start,
                end: segments.len(),
                closed,
            });
        }
    };

    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(p) => {
                flush(segments, out, start, false);
                start = segments.len();
                let p = apply(*p);
                subpath_start = p;
                current = p;
                has_moveto = true;
            }
            PathCmd::LineTo(p) => {
                if !has_moveto {
                    continue;
                }
                let p = apply(*p);
                segments.push(LineSeg { p1: current, p2: p });
                current = p;
            }
            PathCmd::QuadTo { ctrl, end } => {
                if !has_moveto {
                    continue;
                }
                let ctrl = apply(*ctrl);
                let end = apply(*end);
                let p0 = current;
                for i in 1..=QUAD_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(QUAD_STEPS);
                    let next = quad_at(p0, ctrl, end, t);
                    segments.push(LineSeg {
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
                let ctrl1 = apply(*ctrl1);
                let ctrl2 = apply(*ctrl2);
                let end = apply(*end);
                let p0 = current;
                for i in 1..=CUBIC_STEPS {
                    let t = Fixed::from_int(i) / Fixed::from_int(CUBIC_STEPS);
                    let next = cubic_at(p0, ctrl1, ctrl2, end, t);
                    segments.push(LineSeg {
                        p1: current,
                        p2: next,
                    });
                    current = next;
                }
            }
            PathCmd::Close => {
                if current != subpath_start {
                    segments.push(LineSeg {
                        p1: current,
                        p2: subpath_start,
                    });
                }
                current = subpath_start;
                flush(segments, out, start, true);
                start = segments.len();
                has_moveto = false;
            }
        }
    }
    flush(segments, out, start, false);
}

/// Vertical supersampling count per pixel row. 4 sub-scanlines gives 5-level
/// (0/4, 1/4, 2/4, 3/4, 4/4) coverage — enough to hide the worst jaggies
/// without making the rasterizer too heavy on ESP32.
const SUB_SCANLINES: i32 = 4;

/// Coverage-based fill rasterizer with 4 sub-scanlines per pixel row.
/// Emits `cov ∈ [0, 1]` per pixel under the chosen [`FillRule`].
/// `acc` and `crossings` are caller-owned scratch buffers, reused
/// across calls so a tight fill loop doesn't allocate per draw.
#[allow(clippy::too_many_arguments)]
pub fn scanline_fill(
    segs: &[LineSeg],
    px_x0: i32,
    py_y0: i32,
    px_x1: i32,
    py_y1: i32,
    rule: FillRule,
    acc: &mut Vec<Fixed>,
    crossings: &mut Vec<(Fixed, i8)>,
    mut emit: impl FnMut(i32, i32, Fixed),
) {
    if segs.is_empty() || px_x1 <= px_x0 || py_y1 <= py_y0 {
        return;
    }
    let row_w = (px_x1 - px_x0) as usize;
    acc.clear();
    acc.resize(row_w, Fixed::ZERO);
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
                        accumulate_interval(acc, px_x0, px_x1, xa, xb, sub_weight);
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
                                    accumulate_interval(acc, px_x0, px_x1, start, *x, sub_weight);
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

fn apply_dash_pattern(
    segs: &[LineSeg],
    closed: bool,
    pattern: &[Fixed],
    segments: &mut Vec<LineSeg>,
    out: &mut Vec<SubPath>,
) {
    let first_output = out.len();
    let first_segment = segments.len();
    if pattern.is_empty() || pattern.iter().any(|length| *length <= Fixed::ZERO) {
        segments.extend_from_slice(segs);
        if !segs.is_empty() {
            out.push(SubPath {
                start: first_segment,
                end: segments.len(),
                closed,
            });
        }
        return;
    }

    let mut remaining = pattern[0];
    let mut pattern_idx = 0;
    let mut on = true;
    let mut start = segments.len();

    for &seg in segs {
        let dx = seg.p2.x - seg.p1.x;
        let dy = seg.p2.y - seg.p1.y;
        let seg_len = (dx * dx + dy * dy).sqrt();

        if seg_len == Fixed::ZERO {
            continue;
        }

        let ux = dx / seg_len;
        let uy = dy / seg_len;
        let mut walked = Fixed::ZERO;
        let p_start = seg.p1;

        while walked < seg_len {
            let step = (seg_len - walked).min(remaining);
            let p_end = Point {
                x: p_start.x + ux * (walked + step),
                y: p_start.y + uy * (walked + step),
            };
            if on {
                segments.push(LineSeg {
                    p1: Point {
                        x: p_start.x + ux * walked,
                        y: p_start.y + uy * walked,
                    },
                    p2: p_end,
                });
            }
            walked += step;
            remaining -= step;
            if remaining <= Fixed::ZERO {
                if on && segments.len() > start {
                    out.push(SubPath {
                        start,
                        end: segments.len(),
                        closed: false,
                    });
                }
                start = segments.len();
                pattern_idx = (pattern_idx + 1) % pattern.len();
                on = !on;
                remaining = pattern[pattern_idx];
            }
        }
    }
    if segments.len() > start {
        out.push(SubPath {
            start,
            end: segments.len(),
            closed: false,
        });
    }
    if closed && out.len() == first_output + 1 && segments[first_segment..] == *segs {
        out[first_output].closed = true;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn offset_polygon_into(
    cmds: &[PathCmd],
    transform: Option<&Transform>,
    width: Fixed,
    cap: LineCap,
    join: LineJoin,
    miter_limit: Fixed,
    dash: Option<&[Fixed]>,
    out: &mut Path,
    flattened: &mut Vec<LineSeg>,
    subpath_scratch: &mut Vec<SubPath>,
    normals_scratch: &mut Vec<Point>,
    rail_scratch: &mut Vec<Point>,
    left_rail_scratch: &mut Vec<Point>,
    arc_scratch: &mut Vec<Point>,
    dashed: &mut Vec<LineSeg>,
    dash_scratch: &mut Vec<SubPath>,
) {
    out.cmds.to_mut().clear();
    if width <= Fixed::ZERO {
        return;
    }
    let half = width / 2;

    flatten_subpaths_into(cmds, transform, flattened, subpath_scratch);

    let (segments, subpaths): (&[LineSeg], &[SubPath]) = if let Some(pattern) = dash {
        if pattern.is_empty() {
            (flattened, subpath_scratch)
        } else {
            dashed.clear();
            dash_scratch.clear();
            for sub in subpath_scratch.iter() {
                apply_dash_pattern(
                    &flattened[sub.start..sub.end],
                    sub.closed,
                    pattern,
                    dashed,
                    dash_scratch,
                );
            }
            (dashed, dash_scratch)
        }
    } else {
        (flattened, subpath_scratch)
    };

    for sub in subpaths {
        let segs = &segments[sub.start..sub.end];
        compute_normals_into(segs, half, normals_scratch);
        if sub.closed {
            build_ring_into(
                segs,
                normals_scratch,
                half,
                join,
                miter_limit,
                /*left=*/ true,
                rail_scratch,
                arc_scratch,
            );
            append_closed_polyline(out, rail_scratch);
            build_ring_into(
                segs,
                normals_scratch,
                half,
                join,
                miter_limit,
                /*left=*/ false,
                rail_scratch,
                arc_scratch,
            );
            rail_scratch.reverse();
            append_closed_polyline(out, rail_scratch);
        } else {
            build_open_rail_into(
                segs,
                normals_scratch,
                half,
                join,
                miter_limit,
                /*left=*/ true,
                left_rail_scratch,
                arc_scratch,
            );
            build_open_rail_into(
                segs,
                normals_scratch,
                half,
                join,
                miter_limit,
                /*left=*/ false,
                rail_scratch,
                arc_scratch,
            );
            append_open_ribbon(out, segs, left_rail_scratch, rail_scratch, cap, half);
        }
    }
}

fn compute_normals_into(segs: &[LineSeg], half: Fixed, out: &mut Vec<Point>) {
    out.clear();
    out.reserve(segs.len());
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
}

#[allow(clippy::too_many_arguments)]
fn build_ring_into(
    segs: &[LineSeg],
    n: &[Point],
    half: Fixed,
    join: LineJoin,
    miter_limit: Fixed,
    left: bool,
    out: &mut Vec<Point>,
    arc_scratch: &mut Vec<Point>,
) {
    out.clear();
    let sign = if left { Fixed::ONE } else { -Fixed::ONE };
    let count = segs.len();
    out.reserve(count + 1);

    for i in 0..count {
        let prev = if i == 0 { count - 1 } else { i - 1 };
        let p_prev = segs[prev];
        let p_curr = segs[i];
        let n_prev = scaled(n[prev], sign);
        let n_curr = scaled(n[i], sign);

        compute_join_points(
            p_prev.p2,
            p_prev.p1,
            n_prev,
            p_curr.p1,
            p_curr.p2,
            n_curr,
            half,
            join,
            miter_limit,
            arc_scratch,
        );
        out.extend_from_slice(arc_scratch);
    }
    if let Some(&first) = out.first() {
        out.push(first);
    }
}

#[allow(clippy::too_many_arguments)]
fn build_open_rail_into(
    segs: &[LineSeg],
    n: &[Point],
    half: Fixed,
    join: LineJoin,
    miter_limit: Fixed,
    left: bool,
    out: &mut Vec<Point>,
    arc_scratch: &mut Vec<Point>,
) {
    out.clear();
    let sign = if left { Fixed::ONE } else { -Fixed::ONE };
    let count = segs.len();
    out.reserve(count + 1);

    let n0 = scaled(n[0], sign);
    out.push(offset(segs[0].p1, n0));

    for i in 1..count {
        let p_prev = segs[i - 1];
        let p_curr = segs[i];
        let n_prev = scaled(n[i - 1], sign);
        let n_curr = scaled(n[i], sign);

        compute_join_points(
            p_prev.p2,
            p_prev.p1,
            n_prev,
            p_curr.p1,
            p_curr.p2,
            n_curr,
            half,
            join,
            miter_limit,
            arc_scratch,
        );
        out.extend_from_slice(arc_scratch);
    }

    let n_last = scaled(n[count - 1], sign);
    out.push(offset(segs[count - 1].p2, n_last));
}

#[allow(clippy::too_many_arguments)]
fn compute_join_points(
    a: Point,
    b: Point,
    n_prev: Point,
    c: Point,
    d: Point,
    n_curr: Point,
    half: Fixed,
    join: LineJoin,
    miter_limit: Fixed,
    out: &mut Vec<Point>,
) {
    out.clear();
    let p_in = offset(b, n_prev);
    let p_out = offset(c, n_curr);

    if approx_eq(p_in, p_out) {
        out.push(p_in);
        return;
    }

    match join {
        LineJoin::Round => {
            out.push(p_in);
            let center = Point {
                x: (b.x + c.x) / 2,
                y: (b.y + c.y) / 2,
            };
            let steps = arc_steps(half);
            for i in 1..steps {
                let t = Fixed::from_int(i as i32) / Fixed::from_int(steps as i32);
                let p = Point {
                    x: p_in.x + (p_out.x - p_in.x) * t,
                    y: p_in.y + (p_out.y - p_in.y) * t,
                };
                let dx = p.x - center.x;
                let dy = p.y - center.y;
                let len_sq = dx * dx + dy * dy;
                if len_sq > Fixed::ZERO {
                    let len = len_sq.sqrt();
                    let scale = half / len;
                    out.push(Point {
                        x: center.x + dx * scale,
                        y: center.y + dy * scale,
                    });
                } else {
                    out.push(p);
                }
            }
            out.push(p_out);
        }
        LineJoin::Miter | LineJoin::Bevel => {
            if join == LineJoin::Miter {
                let dir_prev = Point {
                    x: b.x - a.x,
                    y: b.y - a.y,
                };
                let dir_curr = Point {
                    x: d.x - c.x,
                    y: d.y - c.y,
                };
                if let Some(miter) = line_intersect(p_in, dir_prev, p_out, dir_curr) {
                    let corner = Point {
                        x: (b.x + c.x) / 2,
                        y: (b.y + c.y) / 2,
                    };
                    let dx = miter.x - corner.x;
                    let dy = miter.y - corner.y;
                    let dist_sq = dx * dx + dy * dy;
                    let limit = half * miter_limit;
                    if dist_sq <= limit * limit {
                        out.push(miter);
                        return;
                    }
                }
            }
            out.push(p_in);
            out.push(p_out);
        }
    }
}

fn arc_steps(half: Fixed) -> usize {
    let r = half.to_int().max(2) as usize;
    (r * 3).clamp(4, 32)
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

fn append_open_ribbon(
    out: &mut Path,
    segs: &[LineSeg],
    left: &[Point],
    right: &[Point],
    cap: LineCap,
    half: Fixed,
) {
    if left.is_empty() || right.is_empty() {
        return;
    }

    let (start_left, start_right) = (left[0], right[0]);
    let (end_left, end_right) = (
        left.last().copied().unwrap_or(left[0]),
        right.last().copied().unwrap_or(right[0]),
    );

    match cap {
        LineCap::Butt => {
            out.move_to(start_left);
            for p in &left[1..] {
                out.line_to(*p);
            }
            for p in right.iter().rev() {
                out.line_to(*p);
            }
            out.close();
        }
        LineCap::Square => {
            let start_dir = scaled(direction(segs[0].p2, segs[0].p1), half);
            let last = segs[segs.len() - 1];
            let end_dir = scaled(direction(last.p1, last.p2), half);

            let sl_ext = offset(start_left, start_dir);
            let sr_ext = offset(start_right, start_dir);
            let el_ext = offset(end_left, end_dir);
            let er_ext = offset(end_right, end_dir);

            out.move_to(sl_ext);
            out.line_to(start_left);
            for p in &left[1..] {
                out.line_to(*p);
            }
            out.line_to(el_ext);
            out.line_to(er_ext);
            for p in right.iter().rev() {
                out.line_to(*p);
            }
            out.line_to(sr_ext);
            out.close();
        }
        LineCap::Round => {
            out.move_to(start_left);
            for p in &left[1..] {
                out.line_to(*p);
            }
            append_arc_cap(out, end_left, end_right, half);
            for p in right.iter().rev().skip(1) {
                out.line_to(*p);
            }
            append_arc_cap(out, start_right, start_left, half);
            out.close();
        }
    }
}

fn direction(from: Point, to: Point) -> Point {
    let dx = to.x - from.x;
    let dy = to.y - from.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq == Fixed::ZERO {
        return Point::ZERO;
    }
    let len = len_sq.sqrt();
    Point {
        x: dx / len * (Fixed::ONE / Fixed::from_int(1)),
        y: dy / len * (Fixed::ONE / Fixed::from_int(1)),
    }
}

fn append_arc_cap(out: &mut Path, p1: Point, p2: Point, half: Fixed) {
    let center = Point {
        x: (p1.x + p2.x) / 2,
        y: (p1.y + p2.y) / 2,
    };
    let steps = arc_steps(half);
    for i in 1..steps {
        let angle =
            -Fixed::from_int(i as i32) * Fixed::from_int(314) / Fixed::from_int(steps as i32 * 100);
        let (sin_v, cos_v) = sin_cos_approx(angle);
        let dx = p1.x - center.x;
        let dy = p1.y - center.y;
        let nx = dx * cos_v - dy * sin_v;
        let ny = dx * sin_v + dy * cos_v;
        out.line_to(Point {
            x: center.x + nx,
            y: center.y + ny,
        });
    }
    out.line_to(p2);
}

fn sin_cos_approx(rad: Fixed) -> (Fixed, Fixed) {
    #[cfg(feature = "std")]
    {
        let f = rad.to_f32();
        (Fixed::from_f32(f.sin()), Fixed::from_f32(f.cos()))
    }
    #[cfg(not(feature = "std"))]
    {
        let x = rad.to_f32();
        let x2 = x * x;
        let s = x - x * x2 / 6.0 + x * x2 * x2 / 120.0;
        let c = 1.0 - x2 / 2.0 + x2 * x2 / 24.0;
        (Fixed::from_f32(s), Fixed::from_f32(c))
    }
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

    fn flatten_path(p: &Path) -> Vec<LineSeg> {
        let mut out = Vec::new();
        flatten_into(&p.cmds, None, &mut out);
        out
    }

    fn flatten_subpaths_path(p: &Path) -> (Vec<LineSeg>, Vec<SubPath>) {
        let mut segments = Vec::new();
        let mut out = Vec::new();
        flatten_subpaths_into(&p.cmds, None, &mut segments, &mut out);
        (segments, out)
    }

    fn offset_polygon_path(p: &Path, width: Fixed) -> Path {
        offset_polygon_path_with_cap(p, width, LineCap::Butt)
    }

    fn offset_polygon_path_with_cap(p: &Path, width: Fixed, cap: LineCap) -> Path {
        let mut out = Path::new();
        let mut flattened = Vec::new();
        let mut scratch = Vec::new();
        let mut normals = Vec::new();
        let mut rail = Vec::new();
        let mut left_rail = Vec::new();
        let mut arc = Vec::new();
        let mut dashed = Vec::new();
        let mut dash_scratch = Vec::new();
        offset_polygon_into(
            &p.cmds,
            None,
            width,
            cap,
            LineJoin::Miter,
            Fixed::from_int(4),
            None,
            &mut out,
            &mut flattened,
            &mut scratch,
            &mut normals,
            &mut rail,
            &mut left_rail,
            &mut arc,
            &mut dashed,
            &mut dash_scratch,
        );
        out
    }

    #[test]
    fn flatten_empty_path() {
        assert!(flatten_path(&Path::new()).is_empty());
    }

    #[test]
    fn flatten_single_line() {
        let mut p = Path::new();
        p.move_to(pt(0, 0)).line_to(pt(10, 0));
        let segs = flatten_path(&p);
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
        let segs = flatten_path(&p);
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
        let segs = flatten_path(&p);
        // close() sees current == subpath_start, so it must not add a redundant edge
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn flatten_quad_emits_n_segments_connected() {
        let mut p = Path::new();
        p.move_to(pt(0, 0)).quad_to(pt(10, 10), pt(20, 0));
        let segs = flatten_path(&p);
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
        let segs = flatten_path(&p);
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
        let segs = flatten_path(&p);
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
        let (_, subs) = flatten_subpaths_path(&p);
        assert_eq!(subs.len(), 2);
        assert!(subs[0].closed);
        assert!(!subs[1].closed);
    }

    #[test]
    fn shared_stroke_scratch_scales_dash_lengths_with_output_pixels() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(40, 0));
        let mut scratch = StrokeScratch::new();
        let dash = [Fixed::from_int(5), Fixed::from_int(5)];
        let spec = |dash_scale| StrokeSpec {
            width: Fixed::from_int(2),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: Fixed::from_int(4),
            dash: &dash,
            dash_scale,
        };
        let contours = |path: &Path| {
            path.cmds
                .iter()
                .filter(|cmd| matches!(cmd, PathCmd::MoveTo(_)))
                .count()
        };
        assert_eq!(contours(scratch.outline(&path, None, spec(Fixed::ONE))), 4);
        assert_eq!(
            contours(scratch.outline(&path, None, spec(Fixed::from_int(2)))),
            2
        );
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
        let outline = offset_polygon_path(&path, Fixed::from_int(2));
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
        let outline = offset_polygon_path(&path, Fixed::from_int(2));
        let closes = outline
            .cmds
            .iter()
            .filter(|c| matches!(c, PathCmd::Close))
            .count();
        assert_eq!(closes, 1);
    }

    #[test]
    fn open_butt_stroke_connects_matching_rail_ends() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        let outline = offset_polygon_path(&path, Fixed::from_int(2));
        assert_eq!(
            &*outline.cmds,
            &[
                PathCmd::MoveTo(pt(0, 1)),
                PathCmd::LineTo(pt(10, 1)),
                PathCmd::LineTo(pt(10, -1)),
                PathCmd::LineTo(pt(0, -1)),
                PathCmd::Close,
            ]
        );
        let edges = flatten_path(&outline);
        for x in [1, 5, 9] {
            assert!(point_in_segments(pt(x, 0), &edges));
        }
        assert!(!point_in_segments(pt(5, 2), &edges));
    }

    #[test]
    fn open_curve_stroke_has_no_long_cross_rail_edge() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).quad_to(pt(12, 16), pt(24, 0));
        let outline = offset_polygon_path(&path, Fixed::from_int(4));
        let edges = flatten_path(&outline);
        assert!(edges.iter().all(|edge| {
            let dx = (edge.p2.x - edge.p1.x).to_f32();
            let dy = (edge.p2.y - edge.p1.y).to_f32();
            dx * dx + dy * dy <= 64.0
        }));
    }

    #[test]
    fn open_square_and_round_caps_extend_outward() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        for cap in [LineCap::Square, LineCap::Round] {
            let outline = offset_polygon_path_with_cap(&path, Fixed::from_int(4), cap);
            let xs = outline.cmds.iter().filter_map(|command| match command {
                PathCmd::MoveTo(point) | PathCmd::LineTo(point) => Some(point.x),
                _ => None,
            });
            assert!(xs.clone().min().unwrap() <= Fixed::from_int(-1), "{cap:?}");
            assert!(xs.max().unwrap() >= Fixed::from_int(11), "{cap:?}");
            assert!(flatten_path(&outline).iter().all(|edge| {
                let dx = (edge.p2.x - edge.p1.x).to_f32();
                let dy = (edge.p2.y - edge.p1.y).to_f32();
                dx * dx + dy * dy <= 100.0
            }));
        }
    }

    #[test]
    fn closed_dash_does_not_repeat_the_closing_edge() {
        let mut path = Path::new();
        path.move_to(pt(0, 0))
            .line_to(pt(10, 0))
            .line_to(pt(10, 10))
            .line_to(pt(0, 10))
            .close();
        let (segments, subpaths) = flatten_subpaths_path(&path);
        let mut dashed_segments = Vec::new();
        let mut dashed = Vec::new();
        apply_dash_pattern(
            &segments[subpaths[0].start..subpaths[0].end],
            true,
            &[Fixed::from_int(100), Fixed::from_int(100)],
            &mut dashed_segments,
            &mut dashed,
        );
        assert_eq!(dashed.len(), 1);
        assert_eq!(dashed_segments, segments);
        assert!(dashed[0].closed);
    }

    #[test]
    fn open_stroke_reuses_both_rail_buffers() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        let mut outline = Path::new();
        let mut flattened = Vec::new();
        let mut subpaths = Vec::new();
        let mut normals = Vec::new();
        let mut right = Vec::new();
        let mut left = Vec::new();
        let mut arc = Vec::new();
        let mut dashed_segments = Vec::new();
        let mut dashed = Vec::new();
        let mut first = None;

        for _ in 0..2 {
            offset_polygon_into(
                &path.cmds,
                None,
                Fixed::from_int(2),
                LineCap::Butt,
                LineJoin::Miter,
                Fixed::from_int(4),
                None,
                &mut outline,
                &mut flattened,
                &mut subpaths,
                &mut normals,
                &mut right,
                &mut left,
                &mut arc,
                &mut dashed_segments,
                &mut dashed,
            );
            let buffers = [
                (left.as_ptr(), left.capacity()),
                (right.as_ptr(), right.capacity()),
            ];
            assert!(buffers.iter().all(|(_, capacity)| *capacity > 0));
            if let Some(previous) = first {
                assert_eq!(buffers, previous);
            }
            first = Some(buffers);
        }
    }

    #[test]
    fn dashed_subpaths_reuse_flat_segment_and_range_buffers() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).quad_to(pt(10, 12), pt(20, 0));
        path.move_to(pt(0, 20)).line_to(pt(20, 20));
        let mut outline = Path::new();
        let mut flattened = Vec::new();
        let mut subpaths = Vec::new();
        let mut normals = Vec::new();
        let mut right = Vec::new();
        let mut left = Vec::new();
        let mut arc = Vec::new();
        let mut dashed_segments = Vec::new();
        let mut dashed_subpaths = Vec::new();
        let mut first = None;

        for _ in 0..2 {
            offset_polygon_into(
                &path.cmds,
                None,
                Fixed::from_int(2),
                LineCap::Butt,
                LineJoin::Miter,
                Fixed::from_int(4),
                Some(&[Fixed::from_int(3), Fixed::from_int(2)]),
                &mut outline,
                &mut flattened,
                &mut subpaths,
                &mut normals,
                &mut right,
                &mut left,
                &mut arc,
                &mut dashed_segments,
                &mut dashed_subpaths,
            );
            let buffers = [
                (flattened.as_ptr().cast::<()>(), flattened.capacity()),
                (subpaths.as_ptr().cast::<()>(), subpaths.capacity()),
                (
                    dashed_segments.as_ptr().cast::<()>(),
                    dashed_segments.capacity(),
                ),
                (
                    dashed_subpaths.as_ptr().cast::<()>(),
                    dashed_subpaths.capacity(),
                ),
            ];
            assert!(buffers.iter().all(|(_, capacity)| *capacity > 0));
            if let Some(previous) = first {
                assert_eq!(buffers, previous);
            }
            first = Some(buffers);
        }
    }

    #[test]
    fn offset_polygon_zero_width_is_empty() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).line_to(pt(10, 0));
        let outline = offset_polygon_path(&path, Fixed::ZERO);
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
        let segs = flatten_path(&p);
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

    #[test]
    fn projective_flatten_projects_curve_samples_in_path_space() {
        let mut path = Path::new();
        path.move_to(pt(0, 0)).quad_to(pt(8, 12), pt(16, 0));
        let projection =
            Transform3D::rotate_y_perspective(Fixed::from_int(25), Fixed::from_int(120));
        let mut segments = Vec::new();

        flatten_projective_into(&path.cmds, &projection, &mut segments).unwrap();

        let midpoint = quad_at(pt(0, 0), pt(8, 12), pt(16, 0), Fixed::from_ratio(1, 2));
        assert_eq!(segments[3].p2, projection.apply_point(midpoint).unwrap());
        assert_eq!(segments.len(), QUAD_STEPS as usize);
    }

    #[test]
    fn projective_flatten_rejects_points_behind_the_camera() {
        let path = Path::rect(
            Fixed::ZERO,
            Fixed::ZERO,
            Fixed::from_int(8),
            Fixed::from_int(8),
        );
        let projection = Transform3D {
            m22: crate::types::Fixed64::ZERO,
            ..Transform3D::IDENTITY
        };
        let mut segments = Vec::new();

        assert_eq!(
            flatten_projective_into(&path.cmds, &projection, &mut segments),
            Err(())
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
        let mut acc = Vec::new();
        let mut crossings = Vec::new();
        scanline_fill(
            segs,
            0,
            0,
            10,
            10,
            rule,
            &mut acc,
            &mut crossings,
            |_, _, cov| {
                if cov > Fixed::from_int(0) {
                    n += 1;
                }
            },
        );
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
