//! Bbox helpers for group-opacity overlap detection.
//!
//! Child bounds include nested affine groups. Geometry without exact ink
//! bounds is reported as an error before a filter or overlap decision.

use alloc::vec::Vec;

use super::SceneOp;
use crate::types::{Fixed, Point, Rect, Transform};

const MAX_GROUP_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundsError {
    UnbalancedGroup,
    InsufficientWorkspace { required: usize, available: usize },
    ProjectiveGroup,
    GlyphInk,
}

#[derive(Clone, Copy)]
struct BoundsFrame {
    transform: Transform,
    bounds: Option<Rect>,
}

impl BoundsFrame {
    const EMPTY: Self = Self {
        transform: Transform::IDENTITY,
        bounds: None,
    };

    fn include(&mut self, bounds: Rect) {
        self.bounds = Some(match self.bounds {
            Some(current) => current.union(&bounds),
            None => bounds,
        });
    }
}

/// Bbox of a single leaf op after its own transform. Group markers and clip
/// operations have no drawable bounds; glyph ink needs a resolved font.
pub fn op_bbox(op: &SceneOp) -> Result<Option<Rect>, BoundsError> {
    Ok(match op {
        SceneOp::GroupBegin { .. }
        | SceneOp::GroupEnd
        | SceneOp::PushClip { .. }
        | SceneOp::PopClip => None,
        SceneOp::FillRect {
            area,
            transform,
            quad,
            ..
        }
        | SceneOp::Border {
            area,
            transform,
            quad,
            ..
        } => Some(rect_after(area, transform, quad)),
        SceneOp::FillPath {
            path, transform, ..
        } => path.bbox().map(|r| transform.apply_rect_bbox(r)),
        SceneOp::StrokePath {
            path,
            transform,
            width,
            line_join,
            miter_limit,
            ..
        } => path
            .bbox()
            .map(|r| {
                let half = *width / Fixed::from_int(2);
                let extent = if *line_join == crate::render::raster::LineJoin::Miter {
                    half * (*miter_limit).max(Fixed::ONE)
                } else {
                    half
                };
                Rect::new(
                    r.x - extent,
                    r.y - extent,
                    r.w + extent * 2,
                    r.h + extent * 2,
                )
            })
            .map(|r| transform.apply_rect_bbox(r)),
        SceneOp::Line {
            p1,
            p2,
            transform,
            width,
            ..
        } => {
            let half = *width / Fixed::from_int(2);
            let mut r = points_bbox(*p1, *p2);
            r.x -= half;
            r.y -= half;
            r.w += *width;
            r.h += *width;
            Some(transform.apply_rect_bbox(r))
        }
        SceneOp::Arc {
            center,
            transform,
            radius,
            width,
            ..
        } => {
            let extent = *radius + *width / Fixed::from_int(2);
            Some(transform.apply_rect_bbox(Rect {
                x: center.x - extent,
                y: center.y - extent,
                w: extent + extent,
                h: extent + extent,
            }))
        }
        SceneOp::GlyphRun { glyphs, .. } if glyphs.is_empty() => None,
        SceneOp::PosedGlyphRun { glyphs, .. } if glyphs.glyphs().is_empty() => None,
        SceneOp::GlyphRun { .. } | SceneOp::PosedGlyphRun { .. } => {
            return Err(BoundsError::GlyphInk);
        }
        SceneOp::Blit {
            pos,
            size,
            transform,
            quad,
            ..
        } => Some(rect_after(
            &Rect {
                x: pos.x,
                y: pos.y,
                w: size.x,
                h: size.y,
            },
            transform,
            quad,
        )),
    })
}

fn rect_after(area: &Rect, transform: &Transform, quad: &Option<[Point; 4]>) -> Rect {
    if let Some(q) = quad {
        return Rect::bounding_quad(q);
    }
    transform.apply_rect_bbox(*area)
}

fn points_bbox(p1: Point, p2: Point) -> Rect {
    let (x0, x1) = if p1.x < p2.x {
        (p1.x, p2.x)
    } else {
        (p2.x, p1.x)
    };
    let (y0, y1) = if p1.y < p2.y {
        (p1.y, p2.y)
    } else {
        (p2.y, p1.y)
    };
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Walk one GroupBegin..GroupEnd level (or the entire slice if not yet
/// inside a group) and collect each direct child's bbox. Nested groups
/// collapse to the union of their subtree's leaf bboxes.
pub fn direct_children_bboxes(ops: &[SceneOp]) -> Result<Vec<Rect>, BoundsError> {
    direct_child_bounds(ops).collect()
}

/// Direct child bounds without retaining an intermediate collection.
pub fn direct_child_bounds(
    ops: &[SceneOp],
) -> impl Iterator<Item = Result<Rect, BoundsError>> + '_ {
    let mut i = 0;
    core::iter::from_fn(move || {
        while i < ops.len() {
            match &ops[i] {
                SceneOp::GroupBegin { .. } => match subtree_bbox(ops, i) {
                    Ok((sub_bbox, end)) => {
                        i = end + 1;
                        if let Some(bounds) = sub_bbox {
                            return Some(Ok(bounds));
                        }
                    }
                    Err(error) => {
                        i = ops.len();
                        return Some(Err(error));
                    }
                },
                SceneOp::GroupEnd => {
                    i = ops.len();
                    return None;
                }
                other => {
                    i += 1;
                    match op_bbox(other) {
                        Ok(Some(bounds)) => return Some(Ok(bounds)),
                        Ok(None) => {}
                        Err(error) => {
                            i = ops.len();
                            return Some(Err(error));
                        }
                    }
                }
            }
        }
        None
    })
}

pub fn union_of_children(ops: &[SceneOp], parent_tf: &Transform) -> Result<Rect, BoundsError> {
    let mut acc = Rect::new(Fixed::ZERO, Fixed::ZERO, Fixed::ZERO, Fixed::ZERO);
    let mut started = false;
    for bounds in direct_child_bounds(ops) {
        let transformed = parent_tf.apply_rect_bbox(bounds?);
        if !started {
            acc = transformed;
            started = true;
        } else {
            acc = acc.union(&transformed);
        }
    }
    Ok(acc)
}

/// Check sibling bounds without allocating a list of rectangles.
pub fn children_disjoint(ops: &[SceneOp]) -> Result<bool, BoundsError> {
    if !has_multiple_children(ops) {
        return Ok(true);
    }
    for (index, bounds) in direct_child_bounds(ops).enumerate() {
        let bounds = bounds?;
        for other in direct_child_bounds(ops).skip(index + 1) {
            if bounds.intersect(&other?).is_some() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn has_multiple_children(ops: &[SceneOp]) -> bool {
    let mut depth = 0usize;
    let mut found = false;
    for op in ops {
        match op {
            SceneOp::GroupBegin { .. } => {
                if depth == 0 {
                    if found {
                        return true;
                    }
                    found = true;
                }
                depth += 1;
            }
            SceneOp::GroupEnd => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            SceneOp::PushClip { .. } | SceneOp::PopClip => {}
            _ if depth == 0 => {
                if found {
                    return true;
                }
                found = true;
            }
            _ => {}
        }
    }
    false
}

fn subtree_bbox(ops: &[SceneOp], begin_idx: usize) -> Result<(Option<Rect>, usize), BoundsError> {
    let mut frames = [BoundsFrame::EMPTY; MAX_GROUP_DEPTH];
    frames[0].transform = group_transform(&ops[begin_idx])?;
    let mut depth = 1;
    let mut i = begin_idx + 1;
    while i < ops.len() {
        match &ops[i] {
            SceneOp::GroupBegin { .. } => {
                if depth == frames.len() {
                    return Err(BoundsError::InsufficientWorkspace {
                        required: depth + 1,
                        available: frames.len(),
                    });
                }
                frames[depth] = BoundsFrame {
                    transform: group_transform(&ops[i])?,
                    bounds: None,
                };
                depth += 1;
            }
            SceneOp::GroupEnd => {
                depth -= 1;
                let frame = frames[depth];
                let bounds = frame.bounds.map(|r| frame.transform.apply_rect_bbox(r));
                if depth == 0 {
                    return Ok((bounds, i));
                }
                if let Some(bounds) = bounds {
                    frames[depth - 1].include(bounds);
                }
            }
            other => {
                if let Some(bounds) = op_bbox(other)? {
                    frames[depth - 1].include(bounds);
                }
            }
        }
        i += 1;
    }
    Err(BoundsError::UnbalancedGroup)
}

fn group_transform(op: &SceneOp) -> Result<Transform, BoundsError> {
    let SceneOp::GroupBegin {
        transform,
        projective,
        ..
    } = op
    else {
        return Err(BoundsError::UnbalancedGroup);
    };
    if projective.is_some_and(|value| !value.is_identity()) {
        return Err(BoundsError::ProjectiveGroup);
    }
    Ok(transform.unwrap_or(Transform::IDENTITY))
}

/// `true` when no two rects in the slice intersect.
pub fn pairwise_disjoint(rects: &[Rect]) -> bool {
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            if a.intersect(b).is_some() {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
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
}
