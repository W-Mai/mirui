//! Signed-distance pixel coverage for the quad rasterizer.
//!
//! Each pixel's coverage is derived from its **signed distance to the
//! nearest quad edge**, clamped to a ±0.5 pixel band and mapped linearly
//! to `[0, 1]`. Rounded corners are expressed in the same SDF by taking
//! the distance to the corner circle instead of to the meeting edges
//! whenever the pixel sits in that corner's outward wedge.
//!
//! Why signed distance rather than analytic area clip: the SDF stays
//! continuous under sub-pixel translation of the quad, which is what
//! keeps the edge from shimmering during scroll; using Fixed64 inside
//! the normalisation removes the Q24.8 precision wobble that killed an
//! earlier attempt to do the same trick.
//!
//! Caller is expected to cache `PreparedEdge` and `CornerShape` once
//! per quad (shared across all pixels) so the inner loop stays light.
//!
//! Winding: `prepare_quad_edges` normalises to left-hand-inside so
//! `signed_dist` returns positive on the inside regardless of input
//! winding order.

use crate::types::{Fixed, Fixed64, Point};

/// Half the pixel diagonal extent along any unit normal: √2/2 ≈ 0.707.
/// AA band width is conservatively taken as one full pixel (±0.5) so any
/// pixel straddling an edge gets a linear falloff rather than a step.
const HALF: Fixed = Fixed::from_raw(128); // 0.5

#[derive(Clone, Copy)]
pub(super) struct PreparedEdge {
    /// One endpoint of the edge; the edge direction is `rot90_cw(normal)`.
    pub base: Point,
    /// Inward (left-hand) normal `rot90_ccw(edge)`. Length = edge length.
    pub nx: Fixed,
    pub ny: Fixed,
    /// Precomputed `1 / |normal|` as Fixed64 so per-pixel work is a
    /// cheap mul + cast, not a divide.
    pub inv_len: Fixed64,
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
        // |n|² = |edge|² since rot90 preserves length.
        let len_sq = Fixed64::from_fixed(edge_dx) * Fixed64::from_fixed(edge_dx)
            + Fixed64::from_fixed(edge_dy) * Fixed64::from_fixed(edge_dy);
        let len = len_sq.sqrt();
        let inv_len = if len > Fixed64::ZERO {
            Fixed64::ONE / len
        } else {
            Fixed64::ZERO
        };
        PreparedEdge {
            base: a,
            nx: -edge_dy,
            ny: edge_dx,
            inv_len,
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

/// Pixel coverage for a (possibly rounded) quad.
///
/// Interior pixels return 1, exterior pixels return 0, and pixels within
/// ±0.5 of an edge or the corner arc get linear falloff.
///
/// `corners = None` skips the rounding path entirely (straight quad).
pub(super) fn quad_pixel_coverage_sdf(
    edges: &[PreparedEdge; 4],
    corners: Option<&[PreparedCorner; 4]>,
    px: i32,
    py: i32,
) -> Fixed {
    // Pixel center.
    let cx = Fixed::from_int(px) + HALF;
    let cy = Fixed::from_int(py) + HALF;

    // Start with distance to the closest straight edge (min over 4 edges).
    // Positive = inside, negative = outside.
    let mut sdf = Fixed::MAX;
    for e in edges {
        let dx = cx - e.base.x;
        let dy = cy - e.base.y;
        // Raw dot = sd * |n|. Normalise so sd is in pixel units.
        let raw = Fixed64::from_fixed(dx) * Fixed64::from_fixed(e.nx)
            + Fixed64::from_fixed(dy) * Fixed64::from_fixed(e.ny);
        let d = (raw * e.inv_len).to_fixed();
        if d < sdf {
            sdf = d;
        }
    }

    // If rounded, corner circles bite into the interior: for any pixel
    // sitting in a corner's outward wedge, its signed distance to the
    // circle (= radius − |p − center|) overrides the two meeting edges.
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
                if corner_sdf < sdf {
                    sdf = corner_sdf;
                }
                break; // wedges are disjoint
            }
        }
    }

    // Map SDF ∈ [−0.5, +0.5] → coverage ∈ [0, 1].
    if sdf >= HALF {
        Fixed::ONE
    } else if sdf <= -HALF {
        Fixed::ZERO
    } else {
        sdf + HALF
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

    #[test]
    fn straight_quad_center_cov_is_full() {
        let q = square_cw();
        let edges = prepare_quad_edges(&q, true);
        let cov = quad_pixel_coverage_sdf(&edges, None, 5, 5);
        assert_eq!(cov, Fixed::ONE);
    }

    #[test]
    fn straight_quad_outside_cov_is_zero() {
        let q = square_cw();
        let edges = prepare_quad_edges(&q, true);
        let cov = quad_pixel_coverage_sdf(&edges, None, 20, 20);
        assert_eq!(cov, Fixed::ZERO);
    }

    #[test]
    fn straight_quad_edge_half_cov() {
        // Shift right edge so it falls through pixel (9, 5) center.
        let q = [pt(0.0, 0.0), pt(9.5, 0.0), pt(9.5, 10.0), pt(0.0, 10.0)];
        let edges = prepare_quad_edges(&q, true);
        let cov = quad_pixel_coverage_sdf(&edges, None, 9, 5);
        assert!((cov.to_f32() - 0.5).abs() < 0.05, "cov = {}", cov.to_f32());
    }

    #[test]
    fn straight_quad_ccw_still_inside() {
        let q = [pt(0.0, 0.0), pt(0.0, 10.0), pt(10.0, 10.0), pt(10.0, 0.0)];
        let cw = shoelace_is_cw(&q);
        assert!(!cw);
        let edges = prepare_quad_edges(&q, cw);
        let cov = quad_pixel_coverage_sdf(&edges, None, 5, 5);
        assert_eq!(cov, Fixed::ONE);
    }

    #[test]
    fn shoelace_cw_positive() {
        assert!(shoelace_is_cw(&square_cw()));
    }
}
