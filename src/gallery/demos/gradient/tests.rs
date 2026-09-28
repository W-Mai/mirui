use super::scene::{
    LINEAR_PANEL, LINEAR_STOPS, LOGICAL_HEIGHT, LOGICAL_WIDTH, RADIAL_DISC, RADIAL_STOPS,
};
use crate::prelude::{Fixed, Point, Rect};
use crate::types::Transform;

#[test]
fn gradient_geometry_and_stops_stay_in_static_storage() {
    assert!(LINEAR_PANEL.is_borrowed());
    assert!(RADIAL_DISC.is_borrowed());
    assert_eq!(LINEAR_STOPS.len(), 3);
    assert_eq!(RADIAL_STOPS.len(), 3);
}

#[test]
fn logical_canvas_stays_inside_phone_bounds() {
    let rect = Rect::new(8, 72, 304, 480);
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
