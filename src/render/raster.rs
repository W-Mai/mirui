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
#[derive(Clone, Copy)]
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
    semantic_joins: Vec<usize>,
    normals: Vec<Point>,
    rail: Vec<Point>,
    left_rail: Vec<Point>,
    arc: Vec<Point>,
    dashed: Vec<LineSeg>,
    dash_subpaths: Vec<SubPath>,
    dashed_semantic_joins: Vec<usize>,
    dash_lengths: Vec<Fixed>,
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu", test))]
impl StrokeScratch {
    pub fn new() -> Self {
        Self {
            outline: Path::new(),
            flattened: Vec::new(),
            subpaths: Vec::new(),
            semantic_joins: Vec::new(),
            normals: Vec::new(),
            rail: Vec::new(),
            left_rail: Vec::new(),
            arc: Vec::new(),
            dashed: Vec::new(),
            dash_subpaths: Vec::new(),
            dashed_semantic_joins: Vec::new(),
            dash_lengths: Vec::new(),
        }
    }

    pub fn outline(
        &mut self,
        path: &[PathCmd],
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
            path,
            transform,
            spec.width,
            spec.cap,
            spec.join,
            spec.miter_limit,
            (!dash.is_empty()).then_some(dash),
            &mut self.outline,
            &mut self.flattened,
            &mut self.subpaths,
            &mut self.semantic_joins,
            &mut self.normals,
            &mut self.rail,
            &mut self.left_rail,
            &mut self.arc,
            &mut self.dashed,
            &mut self.dash_subpaths,
            &mut self.dashed_semantic_joins,
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
    flatten_subpaths_impl(cmds, transform, segments, out, None);
}

fn flatten_stroke_subpaths_into(
    cmds: &[PathCmd],
    transform: Option<&Transform>,
    segments: &mut Vec<LineSeg>,
    out: &mut Vec<SubPath>,
    semantic_joins: &mut Vec<usize>,
) {
    flatten_subpaths_impl(cmds, transform, segments, out, Some(semantic_joins));
}

fn flatten_subpaths_impl(
    cmds: &[PathCmd],
    transform: Option<&Transform>,
    segments: &mut Vec<LineSeg>,
    out: &mut Vec<SubPath>,
    mut semantic_joins: Option<&mut Vec<usize>>,
) {
    segments.clear();
    out.clear();
    if let Some(joins) = semantic_joins.as_deref_mut() {
        joins.clear();
    }
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
                if let Some(joins) = semantic_joins.as_deref_mut() {
                    joins.push(segments.len());
                }
                let p = apply(*p);
                segments.push(LineSeg { p1: current, p2: p });
                current = p;
            }
            PathCmd::QuadTo { ctrl, end } => {
                if !has_moveto {
                    continue;
                }
                if let Some(joins) = semantic_joins.as_deref_mut() {
                    joins.push(segments.len());
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
                if let Some(joins) = semantic_joins.as_deref_mut() {
                    joins.push(segments.len());
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
                    if let Some(joins) = semantic_joins.as_deref_mut() {
                        joins.push(segments.len());
                    }
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
    scanline_fill_rows(
        segs,
        px_x0,
        py_y0,
        px_x1,
        py_y1,
        rule,
        acc,
        crossings,
        |x, y, differences| {
            let mut coverage = Fixed::ZERO;
            for (i, difference) in differences.iter().enumerate() {
                coverage += *difference;
                if coverage > Fixed::ZERO {
                    emit(x + i as i32, y, coverage);
                }
            }
        },
    );
}

/// Emit borrowed coverage differences from the existing row scratch.
/// Consumers reconstruct coverage in order while retaining their row-local state.
#[allow(clippy::too_many_arguments)]
pub(crate) fn scanline_fill_rows(
    segs: &[LineSeg],
    px_x0: i32,
    py_y0: i32,
    px_x1: i32,
    py_y1: i32,
    rule: FillRule,
    acc: &mut Vec<Fixed>,
    crossings: &mut Vec<(Fixed, i8)>,
    mut emit: impl FnMut(i32, i32, &[Fixed]),
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

        emit(px_x0, py, acc);
    }
}

/// Visit exact positive-coverage runs without expanding or copying the row scratch.
pub(crate) fn coverage_spans(differences: &[Fixed], mut emit: impl FnMut(usize, usize, Fixed)) {
    let mut coverage = Fixed::ZERO;
    let mut start = 0;
    while start < differences.len() {
        coverage += differences[start];
        let mut end = start + 1;
        while end < differences.len() && differences[end] == Fixed::ZERO {
            end += 1;
        }
        if coverage > Fixed::ZERO {
            emit(start, end, coverage);
        }
        start = end;
    }
}

/// Add weighted interval coverage as differences in the existing row scratch.
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

    if lo_int >= hi_int {
        return;
    }
    let lo = (lo_int - px_x0) as usize;
    let hi = (hi_int - px_x0) as usize;
    let first = (xhi.min(Fixed::from_int(lo_int + 1)) - xlo.max(Fixed::from_int(lo_int))) * weight;
    acc[lo] += first;
    if hi == lo + 1 {
        if hi < acc.len() {
            acc[hi] -= first;
        }
        return;
    }
    let last = (xhi.min(Fixed::from_int(hi_int)) - xlo.max(Fixed::from_int(hi_int - 1))) * weight;
    acc[lo + 1] += weight - first;
    acc[hi - 1] += last - weight;
    if hi < acc.len() {
        acc[hi] -= last;
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

struct DashJoinMap<'a> {
    source_start: usize,
    source_semantic_joins: &'a [usize],
    output_semantic_joins: &'a mut Vec<usize>,
}

fn apply_dash_pattern(
    segs: &[LineSeg],
    closed: bool,
    pattern: &[Fixed],
    segments: &mut Vec<LineSeg>,
    out: &mut Vec<SubPath>,
    joins: DashJoinMap<'_>,
) {
    let first_output = out.len();
    let first_segment = segments.len();
    if pattern.is_empty() || pattern.iter().any(|length| *length <= Fixed::ZERO) {
        segments.extend_from_slice(segs);
        joins.output_semantic_joins.extend(
            joins
                .source_semantic_joins
                .iter()
                .copied()
                .filter(|index| {
                    *index >= joins.source_start && *index < joins.source_start + segs.len()
                })
                .map(|index| first_segment + index - joins.source_start),
        );
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

    for (source_index, &seg) in segs.iter().enumerate() {
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
                if walked == Fixed::ZERO
                    && joins
                        .source_semantic_joins
                        .binary_search(&(joins.source_start + source_index))
                        .is_ok()
                {
                    joins.output_semantic_joins.push(segments.len());
                }
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
    semantic_joins_scratch: &mut Vec<usize>,
    normals_scratch: &mut Vec<Point>,
    rail_scratch: &mut Vec<Point>,
    left_rail_scratch: &mut Vec<Point>,
    arc_scratch: &mut Vec<Point>,
    dashed: &mut Vec<LineSeg>,
    dash_scratch: &mut Vec<SubPath>,
    dashed_semantic_joins_scratch: &mut Vec<usize>,
) {
    out.cmds.to_mut().clear();
    if width <= Fixed::ZERO {
        return;
    }
    let half = width / 2;

    flatten_stroke_subpaths_into(
        cmds,
        transform,
        flattened,
        subpath_scratch,
        semantic_joins_scratch,
    );

    let (segments, subpaths, semantic_joins): (&[LineSeg], &[SubPath], &[usize]) =
        if let Some(pattern) = dash {
            if pattern.is_empty() {
                (flattened, subpath_scratch, semantic_joins_scratch)
            } else {
                dashed.clear();
                dash_scratch.clear();
                dashed_semantic_joins_scratch.clear();
                for sub in subpath_scratch.iter() {
                    apply_dash_pattern(
                        &flattened[sub.start..sub.end],
                        sub.closed,
                        pattern,
                        dashed,
                        dash_scratch,
                        DashJoinMap {
                            source_start: sub.start,
                            source_semantic_joins: semantic_joins_scratch,
                            output_semantic_joins: dashed_semantic_joins_scratch,
                        },
                    );
                }
                (dashed, dash_scratch, dashed_semantic_joins_scratch)
            }
        } else {
            (flattened, subpath_scratch, semantic_joins_scratch)
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
                semantic_joins,
                sub.start,
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
                semantic_joins,
                sub.start,
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
                semantic_joins,
                sub.start,
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
                semantic_joins,
                sub.start,
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
    semantic_joins: &[usize],
    segment_offset: usize,
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
        let point_join = if semantic_joins.binary_search(&(segment_offset + i)).is_ok() {
            join
        } else {
            LineJoin::Miter
        };

        compute_join_points(
            p_prev.p2,
            p_prev.p1,
            n_prev,
            p_curr.p1,
            p_curr.p2,
            n_curr,
            half,
            point_join,
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
    semantic_joins: &[usize],
    segment_offset: usize,
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
        let point_join = if semantic_joins.binary_search(&(segment_offset + i)).is_ok() {
            join
        } else {
            LineJoin::Miter
        };

        compute_join_points(
            p_prev.p2,
            p_prev.p1,
            n_prev,
            p_curr.p1,
            p_curr.p2,
            n_curr,
            half,
            point_join,
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
            let center = Point {
                x: (b.x + c.x) / 2,
                y: (b.y + c.y) / 2,
            };
            let dir_prev = Point {
                x: b.x - a.x,
                y: b.y - a.y,
            };
            let dir_curr = Point {
                x: d.x - c.x,
                y: d.y - c.y,
            };
            let turn = dir_prev.x * dir_curr.y - dir_prev.y * dir_curr.x;
            let side = dir_prev.x * n_prev.y - dir_prev.y * n_prev.x;
            let outer = (turn > Fixed::ZERO && side < Fixed::ZERO)
                || (turn < Fixed::ZERO && side > Fixed::ZERO);
            if !outer {
                if let Some(inner) = line_intersect(p_in, dir_prev, p_out, dir_curr) {
                    out.push(inner);
                } else {
                    out.push(Point {
                        x: (p_in.x + p_out.x) / 2,
                        y: (p_in.y + p_out.y) / 2,
                    });
                }
                return;
            }

            out.push(p_in);
            let chord_x = p_out.x - p_in.x;
            let chord_y = p_out.y - p_in.y;
            let chord = (chord_x * chord_x + chord_y * chord_y).sqrt();
            let target_step = (half / 4).max(Fixed::from_ratio(1, 4));
            let steps = (chord / target_step)
                .ceil()
                .to_int()
                .clamp(1, arc_steps(half) as i32) as usize;
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
#[path = "../../tests/unit/render/raster.rs"]
mod tests;
