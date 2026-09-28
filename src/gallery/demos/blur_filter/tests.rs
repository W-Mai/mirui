use super::scene::{LOGICAL_HEIGHT, LOGICAL_WIDTH, SCENE};
use crate::prelude::{Point, Rect};
use crate::render::scene::{ResourceRef, SceneOp};
use crate::types::Transform;
use alloc::borrow::Cow;

#[test]
fn scene_paths_borrow_static_commands() {
    let mut paths = 0;
    for op in SCENE {
        match op {
            SceneOp::FillPath { path, .. } => {
                assert!(path.is_borrowed());
                paths += 1;
            }
            SceneOp::GroupBegin {
                filter: Some(ResourceRef::Token(token)),
                ..
            } => assert!(matches!(token, Cow::Borrowed(_))),
            _ => {}
        }
    }
    assert_eq!(paths, 2);
}

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
