use super::*;

#[test]
fn bbox_empty_returns_none() {
    let p = Path::new();
    assert!(p.bbox().is_none());
}

#[test]
fn bbox_rect_matches_source() {
    let p = Path::rect(
        Fixed::from_int(10),
        Fixed::from_int(20),
        Fixed::from_int(30),
        Fixed::from_int(40),
    );
    let bb = p.bbox().unwrap();
    assert_eq!(bb.x.to_int(), 10);
    assert_eq!(bb.y.to_int(), 20);
    assert_eq!(bb.w.to_int(), 30);
    assert_eq!(bb.h.to_int(), 40);
}

#[test]
fn bbox_rounded_rect_is_conservative_superset() {
    let p = Path::rounded_rect(
        Fixed::from_int(0),
        Fixed::from_int(0),
        Fixed::from_int(20),
        Fixed::from_int(20),
        Fixed::from_int(4),
    );
    let bb = p.bbox().unwrap();
    // Control points of quad corners coincide with the rect corners,
    // so bbox must equal [0,0,20,20] exactly (not larger).
    assert_eq!(bb.x.to_int(), 0);
    assert_eq!(bb.y.to_int(), 0);
    assert_eq!(bb.w.to_int(), 20);
    assert_eq!(bb.h.to_int(), 20);
}

#[test]
fn arc_empty_for_zero_sweep() {
    let p = Path::arc(
        Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(50),
        },
        Fixed::from_int(10),
        Fixed::from_int(45),
        Fixed::from_int(45),
    );
    assert!(p.cmds.is_empty());
}

#[test]
fn arc_empty_for_zero_radius() {
    let p = Path::arc(
        Point::ZERO,
        Fixed::ZERO,
        Fixed::from_int(0),
        Fixed::from_int(90),
    );
    assert!(p.cmds.is_empty());
}

#[test]
fn arc_quarter_circle_starts_and_ends_on_circle() {
    let center = Point {
        x: Fixed::from_int(50),
        y: Fixed::from_int(50),
    };
    let r = Fixed::from_int(20);
    let p = Path::arc(center, r, Fixed::from_int(0), Fixed::from_int(90));

    let eps = Fixed::from_f32(0.1);

    let PathCmd::MoveTo(start) = &p.cmds[0] else {
        panic!("expected MoveTo first")
    };
    // 0° → (center + r, center)
    assert!((start.x - (center.x + r)).abs() <= eps);
    assert!((start.y - center.y).abs() <= eps);

    // 90° → (center, center + r)
    let PathCmd::CubicTo { end, .. } = p.cmds.last().unwrap() else {
        panic!("expected CubicTo last")
    };
    assert!((end.x - center.x).abs() <= eps);
    assert!((end.y - (center.y + r)).abs() <= eps);
}

#[test]
fn arc_full_circle_uses_four_segments() {
    let p = Path::arc(
        Point::ZERO,
        Fixed::from_int(10),
        Fixed::from_int(0),
        Fixed::from_int(360),
    );
    // 1 MoveTo + 4 CubicTo
    assert_eq!(p.cmds.len(), 5);
    let cubic_count = p
        .cmds
        .iter()
        .filter(|c| matches!(c, PathCmd::CubicTo { .. }))
        .count();
    assert_eq!(cubic_count, 4);
}

#[test]
fn arc_negative_sweep_goes_clockwise() {
    let p = Path::arc(
        Point::ZERO,
        Fixed::from_int(10),
        Fixed::from_int(0),
        Fixed::from_int(-90),
    );
    assert!(!p.cmds.is_empty());
    let PathCmd::CubicTo { end, .. } = p.cmds.last().unwrap() else {
        panic!()
    };
    // -90° → (0, -10)
    let eps = Fixed::from_f32(0.1);
    assert!(end.x.abs() <= eps);
    assert!((end.y - Fixed::from_int(-10)).abs() <= eps);
}

#[test]
fn rounded_quad_zero_radius_is_plain_polygon() {
    let q = [
        Point {
            x: Fixed::from_int(0),
            y: Fixed::from_int(0),
        },
        Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(0),
        },
        Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        },
        Point {
            x: Fixed::from_int(0),
            y: Fixed::from_int(10),
        },
    ];
    let p = Path::rounded_quad(&q, Fixed::ZERO);
    // Expect MoveTo + 3 × LineTo + Close. No CubicTo.
    let mut has_cubic = false;
    for c in p.cmds.iter() {
        if matches!(c, PathCmd::CubicTo { .. }) {
            has_cubic = true;
        }
    }
    assert!(!has_cubic);
}

#[test]
fn rounded_quad_axis_aligned_matches_rounded_rect_bbox() {
    // When the quad is axis-aligned, its bbox after rounding must
    // equal the input rect (control points of the bezier arcs sit
    // on the rect corners, so bbox doesn't expand).
    let q = [
        Point {
            x: Fixed::from_int(0),
            y: Fixed::from_int(0),
        },
        Point {
            x: Fixed::from_int(20),
            y: Fixed::from_int(0),
        },
        Point {
            x: Fixed::from_int(20),
            y: Fixed::from_int(20),
        },
        Point {
            x: Fixed::from_int(0),
            y: Fixed::from_int(20),
        },
    ];
    let p = Path::rounded_quad(&q, Fixed::from_int(4));
    let bb = p.bbox().unwrap();
    assert_eq!(bb.x.to_int(), 0);
    assert_eq!(bb.y.to_int(), 0);
    assert_eq!(bb.w.to_int(), 20);
    assert_eq!(bb.h.to_int(), 20);
}

#[test]
fn rounded_quad_tilted_still_convex_bbox() {
    // Tilted quad (kite shape). Just check the path builds without
    // panicking and produces 4 arcs.
    let q = [
        Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(0),
        },
        Point {
            x: Fixed::from_int(20),
            y: Fixed::from_int(10),
        },
        Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(20),
        },
        Point {
            x: Fixed::from_int(0),
            y: Fixed::from_int(10),
        },
    ];
    let p = Path::rounded_quad(&q, Fixed::from_int(2));
    let cubic_count = p
        .cmds
        .iter()
        .filter(|c| matches!(c, PathCmd::CubicTo { .. }))
        .count();
    assert_eq!(cubic_count, 4);
}

// Path::from_static must keep the slice in Cow::Borrowed and never
// promote to Owned — that's the whole point of the const path for
// icons / path!-emitted statics on heap-constrained MCUs.
#[test]
fn from_static_stays_borrowed() {
    use alloc::borrow::Cow;
    static CMDS: &[PathCmd] = &[
        PathCmd::MoveTo(Point {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
        }),
        PathCmd::Close,
    ];
    let p = Path::from_static(CMDS);
    assert!(matches!(p.cmds, Cow::Borrowed(_)));
    // sanity: iterating doesn't trigger a copy either.
    assert_eq!(p.cmds.iter().count(), 2);
    assert!(matches!(p.cmds, Cow::Borrowed(_)));
}
