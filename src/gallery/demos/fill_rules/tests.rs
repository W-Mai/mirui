use super::render::{LOGICAL_HEIGHT, LOGICAL_WIDTH, STAR};
use crate::prelude::{Fixed, Point, Rect};
use crate::types::Transform;

#[test]
fn star_geometry_stays_in_static_storage() {
    assert!(STAR.is_borrowed());
}

#[test]
fn logical_canvas_stays_inside_phone_bounds() {
    let rect = Rect {
        x: Fixed::from_int(8),
        y: Fixed::from_int(12),
        w: Fixed::from_int(304),
        h: Fixed::from_int(544),
    };
    let transform = crate::gallery::fit_logical_canvas(
        rect,
        Transform::IDENTITY,
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
    );
    let top_left = transform.apply_point(Point::ZERO);
    let bottom_right = transform.apply_point(Point::new(
        Fixed::from_int(LOGICAL_WIDTH),
        Fixed::from_int(LOGICAL_HEIGHT),
    ));
    assert!(top_left.x >= rect.x && top_left.y >= rect.y);
    assert!(bottom_right.x <= rect.x + rect.w);
    assert!(bottom_right.y <= rect.y + rect.h);
}
