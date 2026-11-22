use super::*;
use crate::render::raster::FillRule;
use crate::render::scene::Paint;
use crate::types::Color;

fn rect_op(x: i32, y: i32, w: i32, h: i32) -> SceneOp {
    SceneOp::FillRect {
        area: Rect {
            x: Fixed::from_int(x),
            y: Fixed::from_int(y),
            w: Fixed::from_int(w),
            h: Fixed::from_int(h),
        },
        transform: Transform::IDENTITY,
        quad: None,
        color: Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        radius: Fixed::ZERO,
        opa: 255,
    }
}

#[test]
fn pairwise_disjoint_two_separate_rects() {
    assert!(pairwise_disjoint(&[
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        },
        Rect {
            x: Fixed::from_int(20),
            y: Fixed::ZERO,
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        },
    ]));
}

#[test]
fn pairwise_disjoint_overlap() {
    assert!(!pairwise_disjoint(&[
        Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        },
        Rect {
            x: Fixed::from_int(5),
            y: Fixed::from_int(5),
            w: Fixed::from_int(10),
            h: Fixed::from_int(10),
        },
    ]));
}

#[test]
fn direct_children_bboxes_disjoint_rects() {
    let ops = [rect_op(0, 0, 10, 10), rect_op(20, 0, 10, 10)];
    let bboxes = direct_children_bboxes(&ops).unwrap();
    assert_eq!(bboxes.len(), 2);
    assert!(pairwise_disjoint(&bboxes));
    assert!(children_disjoint(&ops).unwrap());
    assert_eq!(
        union_of_children(&ops, &Transform::IDENTITY).unwrap(),
        Rect::new(0, 0, 30, 10)
    );
}

#[test]
fn direct_children_bboxes_collapses_nested_group() {
    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        rect_op(0, 0, 10, 10),
        rect_op(5, 5, 10, 10),
        SceneOp::GroupEnd,
        rect_op(30, 0, 10, 10),
    ];
    let bboxes = direct_children_bboxes(&ops).unwrap();
    assert_eq!(bboxes.len(), 2);
    assert!(pairwise_disjoint(&bboxes));
    assert!(children_disjoint(&ops).unwrap());
    assert_eq!(
        union_of_children(&ops, &Transform::IDENTITY).unwrap(),
        Rect::new(0, 0, 40, 15)
    );
}

#[test]
fn streaming_child_bounds_detect_sibling_overlap() {
    let ops = [rect_op(0, 0, 10, 10), rect_op(8, 0, 10, 10)];
    assert!(!children_disjoint(&ops).unwrap());
}

#[test]
fn fill_path_bbox_uses_path_extents() {
    let path = alloc::vec![
        crate::render::path::PathCmd::MoveTo(Point::ZERO),
        crate::render::path::PathCmd::LineTo(Point {
            x: Fixed::from_int(8),
            y: Fixed::from_int(4),
        }),
        crate::render::path::PathCmd::Close,
    ];
    let op = SceneOp::FillPath {
        path: crate::render::path::Path::from_owned(path),
        transform: Transform::IDENTITY,
        paint: Paint::Color(mirx::types::Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }),
        opa: 255,
        fill_rule: FillRule::EvenOdd,
    };
    let bbox = op_bbox(&op).unwrap().unwrap();
    assert_eq!(bbox.x, Fixed::ZERO);
    assert_eq!(bbox.w, Fixed::from_int(8));
}

#[test]
fn miter_stroke_bounds_include_the_outer_join() {
    let path = crate::render::path::Path::from_owned(alloc::vec![
        crate::render::path::PathCmd::MoveTo(Point::ZERO),
        crate::render::path::PathCmd::LineTo(Point {
            x: Fixed::from_int(10),
            y: Fixed::ZERO,
        }),
        crate::render::path::PathCmd::LineTo(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        }),
    ]);
    let op = SceneOp::StrokePath {
        path,
        transform: Transform::IDENTITY,
        paint: Paint::Color(mirx::types::Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        }),
        width: Fixed::from_int(4),
        opa: 255,
        line_cap: crate::render::raster::LineCap::Butt,
        line_join: crate::render::raster::LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: alloc::borrow::Cow::Borrowed(&[]),
    };
    assert_eq!(op_bbox(&op).unwrap(), Some(Rect::new(-8, -8, 26, 26)));
    assert!(!children_disjoint(&[op, rect_op(15, 5, 4, 4)]).unwrap());
}

#[test]
fn nested_group_transforms_are_applied_before_sibling_overlap() {
    let begin = |transform| SceneOp::GroupBegin {
        transform: Some(transform),
        projective: None,
        opacity: None,
        clip: None,
        mask: None,
        filter: None,
        disjoint_hint: false,
    };
    let ops = [
        begin(Transform::translate(Fixed::from_int(10), Fixed::ZERO)),
        begin(Transform::translate(Fixed::from_int(20), Fixed::ZERO)),
        rect_op(0, 0, 10, 10),
        SceneOp::GroupEnd,
        SceneOp::GroupEnd,
        rect_op(35, 0, 10, 10),
    ];
    assert_eq!(
        direct_children_bboxes(&ops).unwrap()[0],
        Rect::new(30, 0, 10, 10)
    );
    assert!(!children_disjoint(&ops).unwrap());
    assert_eq!(
        union_of_children(&ops, &Transform::IDENTITY).unwrap(),
        Rect::new(30, 0, 15, 10)
    );
}

#[test]
fn unknown_group_projection_is_not_treated_as_disjoint() {
    let ops = [
        SceneOp::GroupBegin {
            transform: None,
            projective: Some(crate::types::Transform3D::rotate_y_perspective(
                Fixed::from_int(20),
                Fixed::from_int(400),
            )),
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        },
        rect_op(0, 0, 10, 10),
        SceneOp::GroupEnd,
        rect_op(20, 0, 10, 10),
    ];
    assert_eq!(children_disjoint(&ops), Err(BoundsError::ProjectiveGroup));
}

#[test]
fn glyph_ink_is_not_replaced_by_a_zero_size_origin() {
    let ops = [SceneOp::GlyphRun {
        font: crate::render::scene::ResourceRef::Index(0),
        ppem: 16,
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        color: Color::rgb(255, 255, 255),
        opa: 255,
        glyphs: alloc::borrow::Cow::Owned(alloc::vec![
            textflow::shaping::PositionedGlyph::default(),
        ]),
    }];
    assert_eq!(children_disjoint(&ops), Ok(true));
    assert_eq!(
        union_of_children(&ops, &Transform::IDENTITY),
        Err(BoundsError::GlyphInk)
    );
    let pair = [ops[0].clone(), rect_op(20, 0, 10, 10)];
    assert_eq!(children_disjoint(&pair), Err(BoundsError::GlyphInk));
}

#[test]
fn nested_bounds_have_explicit_depth_capacity() {
    let mut ops = alloc::vec::Vec::new();
    for _ in 0..MAX_GROUP_DEPTH + 1 {
        ops.push(SceneOp::GroupBegin {
            transform: None,
            projective: None,
            opacity: None,
            clip: None,
            mask: None,
            filter: None,
            disjoint_hint: false,
        });
    }
    ops.push(rect_op(0, 0, 10, 10));
    for _ in 0..MAX_GROUP_DEPTH + 1 {
        ops.push(SceneOp::GroupEnd);
    }
    ops.push(rect_op(20, 0, 10, 10));
    assert_eq!(
        children_disjoint(&ops),
        Err(BoundsError::InsufficientWorkspace {
            required: MAX_GROUP_DEPTH + 1,
            available: MAX_GROUP_DEPTH,
        })
    );
}
