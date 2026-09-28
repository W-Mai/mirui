use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH};
use crate::prelude::{Point, Rect};
use crate::types::Transform;

#[test]
fn scene_canvas_stays_inside_phone_bounds() {
    let rect = Rect::new(8, 72, 304, 480);
    let transform = crate::gallery::fit_logical_canvas(
        rect,
        Transform::IDENTITY,
        LOGICAL_WIDTH,
        LOGICAL_HEIGHT,
    );
    let top_left = transform.apply_point(Point::ZERO);
    let bottom_right = transform.apply_point(Point::new(LOGICAL_WIDTH, LOGICAL_HEIGHT));
    assert!(top_left.x >= rect.x && top_left.y >= rect.y);
    assert!(bottom_right.x <= rect.x + rect.w);
    assert!(bottom_right.y <= rect.y + rect.h);
}
