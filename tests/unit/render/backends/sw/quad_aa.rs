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
