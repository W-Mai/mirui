use super::*;
use crate::render::path::PathCmd;
use alloc::vec::Vec;

fn line_points(path: &Path) -> Vec<Point> {
    path.cmds
        .iter()
        .filter_map(|cmd| match cmd {
            PathCmd::MoveTo(p) | PathCmd::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect()
}

fn approx_eq(a: Fixed, b: Fixed, tol: Fixed) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn membrane_path_returns_to_edge_at_ends() {
    let membrane = MagneticMembrane::default();
    let span = membrane.span();
    let path = membrane.path(
        Fixed::from_int(100),
        Fixed::from_int(100),
        MagneticMembraneState {
            ball_offset: Fixed::from_int(30),
            amp: Fixed::from_int(28),
        },
    );
    let pts = line_points(&path);
    assert!(pts.len() > 4);
    let first = pts.first().unwrap();
    let last = pts.last().unwrap();
    let tol = Fixed::ONE / Fixed::from_int(64);
    // edge_path draws forward arc t=-span..=+span then reverses for the inner edge,
    // so first/last both land at t=-span. Middle of path reaches t=+span (min y).
    assert!(approx_eq(first.x, Fixed::from_int(100), tol));
    assert!(approx_eq(last.x, Fixed::from_int(100), tol));
    assert!(approx_eq(first.y, Fixed::from_int(100) + span, tol));
    assert!(approx_eq(last.y, Fixed::from_int(100) + span, tol));
    let min_y = pts.iter().map(|p| p.y).min().unwrap();
    assert!(approx_eq(min_y, Fixed::from_int(100) - span, tol));
}

#[test]
fn arc_membrane_path_endpoints_lie_on_radius() {
    // Endpoints must sit on the watchface arc; drift here means the drop detaches from the bezel.
    let center = Point {
        x: Fixed::from_int(0),
        y: Fixed::from_int(100),
    };
    let radius = Fixed::from_int(100);
    let membrane = MagneticMembrane {
        edge: MagneticMembraneEdge::Arc {
            center,
            radius,
            angle: Fixed::ZERO,
        },
        ..MagneticMembrane::default()
    };
    let path = membrane.path(
        Fixed::from_int(100),
        Fixed::from_int(100),
        MagneticMembraneState {
            ball_offset: Fixed::from_int(20),
            amp: Fixed::from_int(20),
        },
    );
    let pts = line_points(&path);
    assert!(pts.len() > 4);
    let first = pts.first().unwrap();
    let last = pts.last().unwrap();
    // Squared distance avoids fixed-point sqrt.
    let dist = |p: &Point| -> Fixed {
        let dx = p.x - center.x;
        let dy = p.y - center.y;
        dx * dx + dy * dy
    };
    let r2 = radius * radius;
    // 5% radius slack squares to ~10%, covers fixed-point + arc discretisation.
    let tol = r2 / Fixed::from_int(10);
    assert!(approx_eq(dist(first), r2, tol));
    assert!(approx_eq(dist(last), r2, tol));
}

#[test]
fn max_pull_is_smaller_than_span() {
    let membrane = MagneticMembrane::default();
    assert!(membrane.max_pull() < membrane.span());
}

#[test]
fn zero_sigma_falls_back_to_one_and_returns_to_edge() {
    let membrane = MagneticMembrane {
        sigma: Fixed::ZERO,
        ..MagneticMembrane::default()
    };
    assert!(membrane.span() > Fixed::ZERO);
    let path = membrane.path(
        Fixed::from_int(100),
        Fixed::from_int(100),
        MagneticMembraneState {
            ball_offset: Fixed::from_int(30),
            amp: Fixed::from_int(28),
        },
    );
    let pts = line_points(&path);
    assert!(pts.len() > 4);
    let first = pts.first().unwrap();
    let last = pts.last().unwrap();
    let tol = Fixed::ONE / Fixed::from_int(64);
    assert!(approx_eq(first.x, Fixed::from_int(100), tol));
    assert!(approx_eq(last.x, Fixed::from_int(100), tol));
}
