//! Analytic pixel-vs-quad coverage for anti-aliased quad rasterization.
//!
//! A pixel box `[px, px+1] × [py, py+1]` (treated as a unit square in
//! physical coordinates) is clipped against each of the quad's 4 edges.
//! The remaining area ∈ `[0, 1]` is the pixel's coverage. Math is exact
//! (closed-form polygon-plane-clip), so sub-pixel translation of the
//! quad produces continuously varying coverage — no shimmer.
//!
//! Convexity of the quad + convexity of the pixel box guarantee that
//! `edge_clip_area`'s 4-corner sign pattern never falls into the
//! diagonal 2+2 case; it's either 0/1/2-adjacent/3/4 negative corners.

use crate::types::{Fixed, Point};

/// Coverage of pixel `(px, py)` inside quad `q[0..4]` (any winding).
/// Returns `cov ∈ [0, 1]`. Works by clipping the pixel box against each
/// of the 4 edges and starting from full coverage; orientation is
/// detected once (shoelace sign) so each edge gets the right sign.
#[allow(dead_code)]
pub(super) fn quad_pixel_coverage(q: &[Point; 4], px: i32, py: i32) -> Fixed {
    // Shoelace sign picks orientation: positive under screen (y-down)
    // conventions ⟹ clockwise, which is the wind order we treat as
    // canonical ("inside" sits on the edge's left-hand normal). A
    // counter-clockwise quad is flipped by reversing the edge direction
    // before each clip.
    let cw = shoelace_is_cw(q);
    let mut clipped = Fixed::ZERO;
    for i in 0..4 {
        let (a, b) = if cw {
            (q[i], q[(i + 1) & 3])
        } else {
            (q[(i + 1) & 3], q[i])
        };
        clipped += edge_clip_area(px, py, a, b);
    }
    let cov = Fixed::ONE - clipped;
    cov.max(Fixed::ZERO).min(Fixed::ONE)
}

/// Coverage of pixel `(px, py)` against a corner disk of radius `r`
/// centered at `c`. Returns 1 when the pixel center is well inside the
/// disk, 0 when well outside, and a 1-pixel band of linear falloff on
/// the boundary.
///
/// Not analytic area (that would need circle-box clip, several cases);
/// the 1-pixel signed-distance band matches what `rounded_rect_coverage`
/// does for axis-aligned rects and is visually indistinguishable.
#[allow(dead_code)]
pub(super) fn corner_pixel_coverage(px: i32, py: i32, c: Point, r: Fixed) -> Fixed {
    let pcx = Fixed::from_int(px) + Fixed::from_raw(128);
    let pcy = Fixed::from_int(py) + Fixed::from_raw(128);
    let dx = pcx - c.x;
    let dy = pcy - c.y;
    let dist_sq = dx * dx + dy * dy;
    let half = Fixed::ONE / 2;
    let r_in = r - half;
    let r_out = r + half;
    if r_in > Fixed::ZERO && dist_sq <= r_in * r_in {
        return Fixed::ONE;
    }
    if dist_sq >= r_out * r_out {
        return Fixed::ZERO;
    }
    let dist = dist_sq.sqrt();
    // Linear falloff on [r - 0.5, r + 0.5].
    (r_out - dist).max(Fixed::ZERO).min(Fixed::ONE)
}

/// Positive shoelace = clockwise in screen (y-down) coordinates.
#[inline]
#[allow(dead_code)]
fn shoelace_is_cw(q: &[Point; 4]) -> bool {
    let mut sum = Fixed::ZERO;
    for i in 0..4 {
        let a = q[i];
        let b = q[(i + 1) & 3];
        sum += a.x * b.y - b.x * a.y;
    }
    sum > Fixed::ZERO
}

/// Signed distance of `c` from edge (a → b) along the left-hand normal.
/// Positive = `c` is on the half-plane to the edge's left (in the usual
/// screen convention where y grows downward, that's the "inside" of a
/// clockwise-wound quad; a counter-clockwise quad would need the sign
/// flipped by its caller).
#[inline]
#[allow(dead_code)]
pub(super) fn edge_signed_dist(a: Point, b: Point, c: Point) -> Fixed {
    let edge_dx = b.x - a.x;
    let edge_dy = b.y - a.y;
    let nx = -edge_dy;
    let ny = edge_dx;
    let dx = c.x - a.x;
    let dy = c.y - a.y;
    dx * nx + dy * ny
}

/// Area of pixel box `[px, px+1] × [py, py+1]` **clipped away** by one
/// edge (a → b). Returns a value in `[0, 1]`; 0 = pixel fully inside,
/// 1 = pixel fully outside.
///
/// Implementation: the 4 pixel corners each have a signed distance
/// against the edge's inward-pointing normal. The sign pattern
/// (positive = inside, negative = outside) determines geometry:
///   - 0 neg:  fully inside  → 0
///   - 4 neg:  fully outside → 1
///   - 1 neg:  1 triangle clipped off
///   - 3 neg:  1 triangle kept (clip = 1 − triangle_area)
///   - 2 adj:  1 trapezoid clipped
///
/// Diagonal 2+2 is geometrically impossible for a convex quad plus a
/// convex pixel box (convexity of both sides of the edge intersected
/// with the pixel keeps same-sign corners contiguous).
#[allow(dead_code)]
pub(super) fn edge_clip_area(px: i32, py: i32, a: Point, b: Point) -> Fixed {
    let x0 = Fixed::from_int(px);
    let x1 = Fixed::from_int(px + 1);
    let y0 = Fixed::from_int(py);
    let y1 = Fixed::from_int(py + 1);
    let corners = [
        Point { x: x0, y: y0 }, // C0 top-left
        Point { x: x1, y: y0 }, // C1 top-right
        Point { x: x1, y: y1 }, // C2 bottom-right
        Point { x: x0, y: y1 }, // C3 bottom-left
    ];
    let d = [
        edge_signed_dist(a, b, corners[0]),
        edge_signed_dist(a, b, corners[1]),
        edge_signed_dist(a, b, corners[2]),
        edge_signed_dist(a, b, corners[3]),
    ];
    // Sign: treat d == 0 as inside (≥ 0).
    let inside = [
        d[0] >= Fixed::ZERO,
        d[1] >= Fixed::ZERO,
        d[2] >= Fixed::ZERO,
        d[3] >= Fixed::ZERO,
    ];
    let neg_count = 4 - (inside[0] as u8 + inside[1] as u8 + inside[2] as u8 + inside[3] as u8);

    match neg_count {
        0 => Fixed::ZERO,
        4 => Fixed::ONE,
        1 => {
            // One corner outside: clip is a triangle at that corner.
            let k = inside.iter().position(|&i| !i).unwrap();
            let prev = (k + 3) & 3;
            let next = (k + 1) & 3;
            let (t_prev, _) = edge_crossing_param(corners[prev], corners[k], a, b);
            let (t_next, _) = edge_crossing_param(corners[k], corners[next], a, b);
            // Triangle vertices: corner[k], crossing on (prev→k), crossing on (k→next).
            // In the pixel-box unit system, distances from corner[k] to the
            // two crossings are `t_prev` (from prev, so 1 - t_prev from k) and
            // `t_next` (from k itself). Legs are axis-aligned so area = 1/2 * leg_a * leg_b.
            let leg_a = Fixed::ONE - t_prev;
            let leg_b = t_next;
            leg_a * leg_b / 2
        }
        3 => {
            // Three corners outside: one corner kept, rest clipped.
            // Keep = triangle at the single inside corner. Clip = 1 - triangle.
            let k = inside.iter().position(|&i| i).unwrap();
            let prev = (k + 3) & 3;
            let next = (k + 1) & 3;
            let (t_prev, _) = edge_crossing_param(corners[prev], corners[k], a, b);
            let (t_next, _) = edge_crossing_param(corners[k], corners[next], a, b);
            // Triangle at inside corner k: legs from k to crossings.
            // `t_prev` is param on prev→k, so distance from k = 1 - t_prev.
            // `t_next` is param on k→next, so distance from k = t_next.
            let leg_a = Fixed::ONE - t_prev;
            let leg_b = t_next;
            let kept = leg_a * leg_b / 2;
            Fixed::ONE - kept
        }
        2 => {
            // Two adjacent corners outside: trapezoid clipped.
            // Find the two negative corners; they must be adjacent
            // (diagonal case impossible by convexity).
            let k0 = inside.iter().position(|&i| !i).unwrap();
            // Adjacent neg corner is at k0+1 or k0-1 (mod 4).
            let k1 = if !inside[(k0 + 1) & 3] {
                (k0 + 1) & 3
            } else {
                debug_assert!(!inside[(k0 + 3) & 3]);
                (k0 + 3) & 3
            };
            // Walk from the inside corner bordering k0 (the one on the
            // "k0 side" of the outside pair) around through k0, k1 to
            // the inside corner on the other side. Two crossings on
            // the two "entering" edges.
            let (enter_neg, exit_neg) = if (k0 + 1) & 3 == k1 {
                (k0, k1)
            } else {
                (k1, k0)
            };
            let inside_a = (enter_neg + 3) & 3;
            let inside_b = (exit_neg + 1) & 3;
            let (t_enter, _) = edge_crossing_param(corners[inside_a], corners[enter_neg], a, b);
            let (t_exit, _) = edge_crossing_param(corners[exit_neg], corners[inside_b], a, b);
            // Trapezoid parallel sides sit on the same axis: pixel is
            // a unit square, adjacent-pair clip forms a trapezoid whose
            // two parallel sides have lengths (1 - t_enter) and t_exit,
            // separated by the full pixel extent (= 1) along the
            // perpendicular axis. Area = (a + b) / 2 * h = (a + b) / 2.
            let side_a = Fixed::ONE - t_enter;
            let side_b = t_exit;
            (side_a + side_b) / 2
        }
        _ => {
            // 2 opposite — should be unreachable for convex quads.
            debug_assert!(
                false,
                "edge_clip_area: diagonal 2+2 sign pattern unexpected for convex quad"
            );
            // Fallback: degrade to full clip to avoid rendering garbage.
            Fixed::ONE
        }
    }
}

/// Given a pixel-box edge from `p` to `q` (the path between two adjacent
/// pixel corners), find where the quad edge (a → b) crosses it.
/// Returns `(t, point)` where `t ∈ [0, 1]` is the parameter on segment
/// (p → q). If the edges are parallel, returns `t = 0` (caller's sign
/// check ensures this path isn't taken in practice).
#[inline]
#[allow(dead_code)]
fn edge_crossing_param(p: Point, q: Point, a: Point, b: Point) -> (Fixed, Point) {
    // Parameterize pixel-box edge: point(t) = p + t * (q - p)
    // Edge a→b has inward normal n = (-(b.y - a.y), b.x - a.x).
    // Signed dist at p: dp = dot(p - a, n), at q: dq = dot(q - a, n).
    // Solve dp + t * (dq - dp) = 0  →  t = dp / (dp - dq).
    let dp = edge_signed_dist(a, b, p);
    let dq = edge_signed_dist(a, b, q);
    let denom = dp - dq;
    if denom == Fixed::ZERO {
        return (Fixed::ZERO, p);
    }
    let t = dp / denom;
    let t = t.max(Fixed::ZERO).min(Fixed::ONE);
    let x = p.x + (q.x - p.x) * t;
    let y = p.y + (q.y - p.y) * t;
    (t, Point { x, y })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Fixed;

    fn pt(x: f32, y: f32) -> Point {
        Point {
            x: Fixed::from_f32(x),
            y: Fixed::from_f32(y),
        }
    }

    fn approx(a: Fixed, expected: f32) -> bool {
        let diff = (a.to_f32() - expected).abs();
        diff < 0.01
    }

    #[test]
    fn edge_signed_dist_left_hand_positive() {
        // Edge going +x from (0,0) to (1,0). Left-hand normal in screen
        // coords (y down) is (0, +1), pointing downward. A point below
        // the edge sits on that side → positive.
        let d = edge_signed_dist(pt(0.0, 0.0), pt(1.0, 0.0), pt(0.5, 0.5));
        assert!(d > Fixed::ZERO);
    }

    #[test]
    fn edge_signed_dist_right_hand_negative() {
        // Same edge; a point above is on the right-hand side.
        let d = edge_signed_dist(pt(0.0, 0.0), pt(1.0, 0.0), pt(0.5, -0.5));
        assert!(d < Fixed::ZERO);
    }

    #[test]
    fn clip_full_inside_zero() {
        // Edge above the pixel going right→left, inward normal = (0, −1)
        // pointing down into the pixel region. Pixel at (0, 0) is
        // fully on the inside → clip = 0.
        let clip = edge_clip_area(0, 0, pt(20.0, 10.0), pt(10.0, 10.0));
        assert_eq!(clip, Fixed::ZERO);
    }

    #[test]
    fn clip_full_outside_one() {
        // Edge above the pixel going left→right, inward normal = (0, 1)
        // pointing up away from the pixel. Pixel at (0, 0) is
        // fully on the outside → clip = 1.
        let clip = edge_clip_area(0, 0, pt(10.0, 10.0), pt(20.0, 10.0));
        assert_eq!(clip, Fixed::ONE);
    }

    #[test]
    fn clip_diagonal_half() {
        // Edge from (0, 1) → (1, 0) going diagonally; inward normal
        // points toward origin (−1, −1)/√2. Pixel (0,0) (corners at
        // (0,0),(1,0),(1,1),(0,1)): corner (1,1) is outside, other 3 inside.
        // Clipped triangle at (1,1) has legs 1,1 → area = 0.5.
        let clip = edge_clip_area(0, 0, pt(0.0, 1.0), pt(1.0, 0.0));
        assert!(
            approx(clip, 0.5),
            "diagonal edge clip = {:?}, want 0.5",
            clip.to_f32()
        );
    }

    #[test]
    fn clip_horizontal_half() {
        // Edge y = 0.5 crossing pixel horizontally. Going left→right so
        // inward normal points up. Bottom 2 corners (y=1) outside →
        // trapezoid with parallel sides len 1 and 1, clipped area = 0.5.
        let clip = edge_clip_area(0, 0, pt(0.0, 0.5), pt(1.0, 0.5));
        assert!(
            approx(clip, 0.5),
            "horizontal half clip = {:?}, want 0.5",
            clip.to_f32()
        );
    }

    fn square_quad_cw() -> [Point; 4] {
        // Screen y-down: (0,0) top-left → (10,0) top-right → (10,10) BR → (0,10) BL.
        // This traces clockwise on screen, positive shoelace.
        [pt(0.0, 0.0), pt(10.0, 0.0), pt(10.0, 10.0), pt(0.0, 10.0)]
    }

    #[test]
    fn pixel_coverage_center_is_full() {
        // Pixel (5, 5) fully inside the 10×10 square at origin.
        let q = square_quad_cw();
        let cov = quad_pixel_coverage(&q, 5, 5);
        assert_eq!(cov, Fixed::ONE);
    }

    #[test]
    fn pixel_coverage_outside_is_zero() {
        let q = square_quad_cw();
        let cov = quad_pixel_coverage(&q, 20, 20);
        assert_eq!(cov, Fixed::ZERO);
    }

    #[test]
    fn pixel_coverage_ccw_quad_still_works() {
        // Same geometry, vertices given counter-clockwise.
        let q = [pt(0.0, 0.0), pt(0.0, 10.0), pt(10.0, 10.0), pt(10.0, 0.0)];
        let cov = quad_pixel_coverage(&q, 5, 5);
        assert_eq!(cov, Fixed::ONE);
    }

    #[test]
    fn pixel_coverage_on_edge_half() {
        // Quad edge passes through pixel (9, 5): pixel box spans x ∈ [9, 10],
        // edge at x = 10 cuts nothing (quad reaches x = 10), pixel fully
        // inside → cov = 1. Shift quad right edge to x = 9.5 to halve.
        let q = [pt(0.0, 0.0), pt(9.5, 0.0), pt(9.5, 10.0), pt(0.0, 10.0)];
        let cov = quad_pixel_coverage(&q, 9, 5);
        // Pixel [9, 10] × [5, 6], right edge of quad at x = 9.5 cuts
        // the pixel in half.
        assert!((cov.to_f32() - 0.5).abs() < 0.01, "cov = {}", cov.to_f32());
    }

    #[test]
    fn corner_coverage_inside_disk_full() {
        // Pixel (5, 5) center = (5.5, 5.5), disk center (0, 0), r = 10.
        // dist ≈ 7.78 < r - 0.5 → cov = 1.
        let cov = corner_pixel_coverage(5, 5, pt(0.0, 0.0), Fixed::from_f32(10.0));
        assert_eq!(cov, Fixed::ONE);
    }

    #[test]
    fn corner_coverage_outside_disk_zero() {
        // Pixel (10, 10) center = (10.5, 10.5), disk center (0, 0), r = 5.
        // dist ≈ 14.85 > r + 0.5 → cov = 0.
        let cov = corner_pixel_coverage(10, 10, pt(0.0, 0.0), Fixed::from_f32(5.0));
        assert_eq!(cov, Fixed::ZERO);
    }

    #[test]
    fn corner_coverage_on_boundary_half() {
        // Disk center (0, 0), r = 7. Pixel center (5.5, 5.5) → dist =
        // sqrt(60.5) ≈ 7.78 → overshoot 0.78 out of 1 → cov ≈ 0.22.
        // More useful: place pixel so center lies exactly on disk.
        // Disk r = 7.778 at (0, 0), pixel (5, 5) center (5.5, 5.5),
        // dist = 7.778, boundary → t = 0.5.
        let cov = corner_pixel_coverage(
            5,
            5,
            pt(0.0, 0.0),
            Fixed::from_f32((5.5_f32 * 5.5_f32 + 5.5_f32 * 5.5_f32).sqrt()),
        );
        assert!((cov.to_f32() - 0.5).abs() < 0.05, "cov = {}", cov.to_f32());
    }

    #[test]
    fn clip_triangle_corner() {
        // Edge cutting off just top-right corner: from (0.7, 0) to
        // (1, 0.3). Inward normal: pointing up-left. Only corner (1,0)
        // is outside. Triangle legs: 1−0.7=0.3, 0.3 → area = 0.045.
        let clip = edge_clip_area(0, 0, pt(0.7, 0.0), pt(1.0, 0.3));
        assert!(
            approx(clip, 0.045),
            "triangle clip = {:?}, want 0.045",
            clip.to_f32()
        );
    }
}
