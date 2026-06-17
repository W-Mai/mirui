//! Quad coverage with three implementations selected by feature.
//!
//! - **`quad-aa` + `std`** (desktop SDL / SDL GPU): Fixed64 signed-distance
//!   field, smooth 256-step coverage.
//! - **`quad-aa` without `std`** (MCU): 2×2 supersample, coverage
//!   quantised to `{0, 0.25, 0.5, 0.75, 1}`. Each sample test is four
//!   integer adds + sign check per edge, no divide / no sqrt.
//! - **Neither** (`default-features = false` without `quad-aa`): binary
//!   point-in-quad, the v0.9.x fill behaviour — hard-edged but cheapest.
//!   Pick this when an MCU can't spare the supersample cycles.
//!
//! `PreparedEdge` / `EdgeRowState` are shared; only the per-pixel
//! coverage function switches. Caller code uses
//! `quad_pixel_coverage_row`, a cfg alias for the active implementation.
//!
//! Winding: `prepare_quad_edges` normalises to left-hand-inside so
//! positive signed distance means inside regardless of the caller's
//! vertex winding.

#[cfg(all(feature = "quad-aa", feature = "std"))]
use crate::types::Fixed64;
use crate::types::{Fixed, Point};

/// Half the pixel diagonal extent along any unit normal: √2/2 ≈ 0.707.
/// AA band width is conservatively taken as one full pixel (±0.5) so any
/// pixel straddling an edge gets a linear falloff rather than a step.

#[derive(Clone, Copy)]
pub(super) struct PreparedEdge {
    /// One endpoint of the edge; the edge direction is `rot90_cw(normal)`.
    pub base: Point,
    /// Inward (left-hand) normal `rot90_ccw(edge)`. Length = edge length.
    pub nx: Fixed,
    pub ny: Fixed,
    /// `1 / |normal|` as Fixed64, used by the std (SDF) path to turn
    /// raw signed distance into pixel units.
    #[cfg(all(feature = "quad-aa", feature = "std"))]
    pub inv_len: Fixed64,
    /// `(|n| / 2)²` as Fixed64 — the std (SDF) band cut-off. Raw²
    /// compared against this tells the hot path whether to skip the
    /// normalise.
    #[cfg(all(feature = "quad-aa", feature = "std"))]
    pub half_len_sq: Fixed64,
    /// Quarter-normal increments used by the no_std 2×2 supersample
    /// path. A sample offset of ±0.25 along x or y shifts raw by
    /// ±(nx / 4) and ±(ny / 4); caching both keeps the inner loop to
    /// a single integer add + sign bit read per sample per edge.
    #[cfg(all(feature = "quad-aa", not(feature = "std")))]
    pub qx: Fixed,
    #[cfg(all(feature = "quad-aa", not(feature = "std")))]
    pub qy: Fixed,
}

/// Prepare the quad's edges into inside-on-the-left orientation. Input
/// `q` may be either winding; pass `cw = shoelace_is_cw(q)` to decide.
pub(super) fn prepare_quad_edges(q: &[Point; 4], cw: bool) -> [PreparedEdge; 4] {
    core::array::from_fn(|i| {
        let (a, b) = if cw {
            (q[i], q[(i + 1) & 3])
        } else {
            (q[(i + 1) & 3], q[i])
        };
        let edge_dx = b.x - a.x;
        let edge_dy = b.y - a.y;
        let nx = -edge_dy;
        let ny = edge_dx;
        #[cfg(all(feature = "quad-aa", feature = "std"))]
        let (inv_len, half_len_sq) = {
            // |n|² = |edge|² since rot90 preserves length. Fixed64
            // because the square of a screen-scale edge can overflow
            // Fixed (Q24.8). Only paid once per quad, so no hot-path cost.
            let len_sq = Fixed64::from_fixed(edge_dx) * Fixed64::from_fixed(edge_dx)
                + Fixed64::from_fixed(edge_dy) * Fixed64::from_fixed(edge_dy);
            let len = len_sq.sqrt();
            let inv_len = if len > Fixed64::ZERO {
                Fixed64::ONE / len
            } else {
                Fixed64::ZERO
            };
            (inv_len, Fixed64::from_raw(len_sq.raw() / 4))
        };
        #[cfg(all(feature = "quad-aa", not(feature = "std")))]
        let (qx, qy) = {
            // Sample offset ±0.25 pixel along either axis shifts raw by
            // ±(n / 4). Precompute once per quad — the hot path reads
            // these instead of scaling nx / ny each sample.
            (Fixed::from_raw(nx.raw() / 4), Fixed::from_raw(ny.raw() / 4))
        };
        PreparedEdge {
            base: a,
            nx,
            ny,
            #[cfg(all(feature = "quad-aa", feature = "std"))]
            inv_len,
            #[cfg(all(feature = "quad-aa", feature = "std"))]
            half_len_sq,
            #[cfg(all(feature = "quad-aa", not(feature = "std")))]
            qx,
            #[cfg(all(feature = "quad-aa", not(feature = "std")))]
            qy,
        }
    })
}

/// Pre-computed corner data for quad rounding. `center` is the circle
/// center (sits `radius` inward from the vertex along both incident
/// edges); `ua` / `ub` are the two inward unit vectors that define the
/// wedge; `radius` is cached raw for the inner loop.
#[derive(Clone, Copy)]
pub(super) struct PreparedCorner {
    pub center: Point,
    pub ua: Point,
    pub ub: Point,
    pub radius: Fixed,
}

/// Per-row state for incremental SDF: `raw` at the first pixel of the
/// row. Stepping right by one pixel is a single `raw += nx` add. One
/// instance per edge, built once per row.
pub(super) struct EdgeRowState {
    pub raw: [Fixed; 4],
}

impl EdgeRowState {
    pub(super) fn new(edges: &[PreparedEdge; 4], cx_start: Fixed, cy: Fixed) -> Self {
        let mut raw = [Fixed::ZERO; 4];
        for i in 0..4 {
            let e = &edges[i];
            let dx = cx_start - e.base.x;
            let dy = cy - e.base.y;
            raw[i] = dx * e.nx + dy * e.ny;
        }
        Self { raw }
    }

    #[inline]
    pub(super) fn step(&mut self, edges: &[PreparedEdge; 4]) {
        for (raw, e) in self.raw.iter_mut().zip(edges.iter()) {
            *raw += e.nx;
        }
    }
}

#[cfg(not(feature = "quad-aa"))]
pub(super) use quad_pixel_coverage_row_binary as quad_pixel_coverage_row;
/// Cfg alias: picks the SDF coverage with `quad-aa + std`, the
/// supersample coverage with `quad-aa` on `no_std`, or the binary
/// point-in-quad test when `quad-aa` is disabled. Callers never select
/// directly.
#[cfg(all(feature = "quad-aa", feature = "std"))]
pub(super) use quad_pixel_coverage_row_sdf as quad_pixel_coverage_row;
#[cfg(all(feature = "quad-aa", not(feature = "std")))]
pub(super) use quad_pixel_coverage_row_supersample as quad_pixel_coverage_row;

/// Binary coverage — the v0.9.x hard-edge fill. Returns `Fixed::ONE` if
/// the pixel center is inside all four edges, else `Fixed::ZERO`. No
/// rounded-corner support here either; the corner disk still takes a
/// sample, just a binary one.
#[cfg(not(feature = "quad-aa"))]
#[inline]
pub(super) fn quad_pixel_coverage_row_binary(
    _edges: &[PreparedEdge; 4],
    corners: Option<&[PreparedCorner; 4]>,
    cx: Fixed,
    cy: Fixed,
    row: &EdgeRowState,
) -> Fixed {
    // Any edge with a negative raw signed distance at the pixel center
    // excludes this pixel.
    for raw in row.raw.iter() {
        if raw.raw() < 0 {
            return Fixed::ZERO;
        }
    }
    // Rounded corner: pixel center inside the outward wedge AND outside
    // the disk means it's clipped off.
    if let Some(corner_arr) = corners {
        for c in corner_arr {
            let dx = cx - c.center.x;
            let dy = cy - c.center.y;
            let proj_a = dx * c.ua.x + dy * c.ua.y;
            let proj_b = dx * c.ub.x + dy * c.ub.y;
            if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
                let dx_raw = dx.raw() as i64;
                let dy_raw = dy.raw() as i64;
                let dist_sq = dx_raw * dx_raw + dy_raw * dy_raw;
                let r_raw = c.radius.raw() as i64;
                if dist_sq > r_raw * r_raw {
                    return Fixed::ZERO;
                }
                break;
            }
        }
    }
    Fixed::ONE
}

/// 2×2 supersample coverage for the pixel whose row state is `row`.
/// Returns a `Fixed` in `{0, 0.25, 0.5, 0.75, 1}`. Hot path is four
/// add-and-sign-test per sample per edge, no divides — used on targets
/// where Fixed64 multiply is a software i64 shim.
#[cfg(all(feature = "quad-aa", not(feature = "std")))]
#[inline]
pub(super) fn quad_pixel_coverage_row_supersample(
    edges: &[PreparedEdge; 4],
    corners: Option<&[PreparedCorner; 4]>,
    cx: Fixed,
    cy: Fixed,
    row: &EdgeRowState,
) -> Fixed {
    // Per sample, test all four edges (left-hand signed distance > 0
    // means inside that edge; inside all four = inside the quad).
    // Early-abort on the first edge that excludes the sample: most
    // samples bail after one or two edge checks on the hot path.
    let s00 = sample_inside(edges, row, -1, -1);
    let s10 = sample_inside(edges, row, 1, -1);
    let s01 = sample_inside(edges, row, -1, 1);
    let s11 = sample_inside(edges, row, 1, 1);
    let mut hit = s00 as u32 + s10 as u32 + s01 as u32 + s11 as u32;

    // Rounded corners: inside the outward wedge, override with a disk
    // hit test (sub-sample distance² < r²). `break` keeps the loop at
    // O(1) — wedges don't overlap.
    if let Some(corner_arr) = corners {
        for c in corner_arr {
            let dx = cx - c.center.x;
            let dy = cy - c.center.y;
            let proj_a = dx * c.ua.x + dy * c.ua.y;
            let proj_b = dx * c.ub.x + dy * c.ub.y;
            if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
                hit = corner_sample_hit(c, cx, cy);
                break;
            }
        }
    }

    match hit {
        0 => Fixed::ZERO,
        1 => Fixed::from_raw(64),  // 0.25 in Q24.8
        2 => Fixed::HALF,          // 0.5
        3 => Fixed::from_raw(192), // 0.75
        _ => Fixed::ONE,
    }
}

/// Sample at offset (sx, sy) × 0.25 pixel from the row anchor. `sx` /
/// `sy` are `-1` or `+1`. Returns `true` if the sample is inside all
/// four edges.
#[cfg(all(feature = "quad-aa", not(feature = "std")))]
#[inline(always)]
fn sample_inside(edges: &[PreparedEdge; 4], row: &EdgeRowState, sx: i32, sy: i32) -> bool {
    for (e, raw_center) in edges.iter().zip(row.raw.iter()) {
        let qx = Fixed::from_raw(sx * e.qx.raw());
        let qy = Fixed::from_raw(sy * e.qy.raw());
        let raw = *raw_center + qx + qy;
        if raw.raw() < 0 {
            return false;
        }
    }
    true
}

/// Count how many of a corner's four sub-samples sit inside the disk.
/// Used only for pixels that fall in the corner's outward wedge.
#[cfg(all(feature = "quad-aa", not(feature = "std")))]
#[inline]
fn corner_sample_hit(c: &PreparedCorner, cx: Fixed, cy: Fixed) -> u32 {
    let r = c.radius;
    let r_sq_raw = r.raw() as i64 * r.raw() as i64;
    let quarter = Fixed::from_raw(64); // 0.25
    let offsets = [
        (-quarter, -quarter),
        (quarter, -quarter),
        (-quarter, quarter),
        (quarter, quarter),
    ];
    let mut hit = 0u32;
    for (dx, dy) in offsets {
        let sx = (cx + dx) - c.center.x;
        let sy = (cy + dy) - c.center.y;
        // |sample − center|² < r²: done as i64 to avoid Fixed overflow
        // on big corners; each multiply is a single RV32M `mulh` pair,
        // roughly the cost of a Fixed split multiply.
        let sx_raw = sx.raw() as i64;
        let sy_raw = sy.raw() as i64;
        if sx_raw * sx_raw + sy_raw * sy_raw < r_sq_raw {
            hit += 1;
        }
    }
    hit
}

/// SDF coverage for the pixel whose row state is `row`. Returns a
/// continuous `Fixed` in `[0, 1]`. Uses Fixed64 to normalise raw signed
/// distance to pixel units, which gives smooth 256-step coverage but
/// pays an i64 shim on RV32 targets — only enabled on `std` builds.
#[cfg(all(feature = "quad-aa", feature = "std"))]
#[inline]
pub(super) fn quad_pixel_coverage_row_sdf(
    edges: &[PreparedEdge; 4],
    corners: Option<&[PreparedCorner; 4]>,
    cx: Fixed,
    cy: Fixed,
    row: &EdgeRowState,
) -> Fixed {
    let mut min_sdf = Fixed::MAX;
    for (e, raw_fixed) in edges.iter().zip(row.raw.iter()) {
        let raw = Fixed64::from_fixed(*raw_fixed);
        let raw_sq = raw * raw;
        if raw_sq >= e.half_len_sq {
            // Safely inside or outside the ±0.5 band.
            if raw.raw() < 0 {
                return Fixed::ZERO;
            }
            continue;
        }
        let d = (raw * e.inv_len).to_fixed();
        if d < min_sdf {
            min_sdf = d;
        }
    }

    if let Some(corner_arr) = corners {
        for c in corner_arr {
            let dx = cx - c.center.x;
            let dy = cy - c.center.y;
            let proj_a = dx * c.ua.x + dy * c.ua.y;
            let proj_b = dx * c.ub.x + dy * c.ub.y;
            if proj_a < Fixed::ZERO && proj_b < Fixed::ZERO {
                let dist_sq = Fixed64::from_fixed(dx) * Fixed64::from_fixed(dx)
                    + Fixed64::from_fixed(dy) * Fixed64::from_fixed(dy);
                let dist = dist_sq.sqrt().to_fixed();
                let corner_sdf = c.radius - dist;
                if corner_sdf < min_sdf {
                    min_sdf = corner_sdf;
                }
                break;
            }
        }
    }

    if min_sdf >= Fixed::HALF {
        Fixed::ONE
    } else if min_sdf <= -Fixed::HALF {
        Fixed::ZERO
    } else {
        min_sdf + Fixed::HALF
    }
}

/// Positive shoelace = clockwise in screen (y-down) coordinates.
#[inline]
pub(super) fn shoelace_is_cw(q: &[Point; 4]) -> bool {
    let mut sum = Fixed::ZERO;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) & 3];
        sum += a.x * b.y - b.x * a.y;
    }
    sum > Fixed::ZERO
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> Point {
        Point {
            x: Fixed::from_f32(x),
            y: Fixed::from_f32(y),
        }
    }

    fn square_cw() -> [Point; 4] {
        [pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)]
    }

    fn cov_at(edges: &[PreparedEdge; 4], px: i32, py: i32) -> Fixed {
        let cx = Fixed::from_int(px) + Fixed::HALF;
        let cy = Fixed::from_int(py) + Fixed::HALF;
        let row = EdgeRowState::new(edges, cx, cy);
        quad_pixel_coverage_row(edges, None, cx, cy, &row)
    }

    #[test]
    fn straight_quad_center_cov_is_full() {
        let q = square_cw();
        let edges = prepare_quad_edges(&q, true);
        assert_eq!(cov_at(&edges, 5, 5), Fixed::ONE);
    }

    #[test]
    fn straight_quad_outside_cov_is_zero() {
        let q = square_cw();
        let edges = prepare_quad_edges(&q, true);
        assert_eq!(cov_at(&edges, 20, 20), Fixed::ZERO);
    }

    // Partial coverage only makes sense under `quad-aa`; the binary
    // fallback returns 0 or 1, never anything in between.
    #[cfg(feature = "quad-aa")]
    #[test]
    fn straight_quad_edge_half_cov() {
        // Shift right edge so it falls through pixel (9, 5) center.
        let q = [pt(0.0, 0.0), pt(9.5, 0.0), pt(9.5, 10.0), pt(0.0, 10.0)];
        let edges = prepare_quad_edges(&q, true);
        let cov = cov_at(&edges, 9, 5);
        assert!((cov.to_f32() - 0.5).abs() < 0.05, "cov = {}", cov.to_f32());
    }

    #[test]
    fn straight_quad_ccw_still_inside() {
        let q = [pt(0.0, 0.0), pt(0.0, 10.0), pt(10.0, 10.0), pt(10.0, 0.0)];
        let cw = shoelace_is_cw(&q);
        assert!(!cw);
        let edges = prepare_quad_edges(&q, cw);
        assert_eq!(cov_at(&edges, 5, 5), Fixed::ONE);
    }

    #[test]
    fn shoelace_cw_positive() {
        assert!(shoelace_is_cw(&square_cw()));
    }
}
