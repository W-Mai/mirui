//! Bbox helpers for group-opacity overlap detection.
//!
//! `op_bbox` returns each op's axis-aligned bbox in *parent* coordinates
//! (its own per-op transform applied). `direct_children_bboxes` walks one
//! GroupBegin..GroupEnd level and collapses nested groups to a single
//! bbox = union of their subtree, so `pairwise_disjoint` can be O(K²) on
//! K direct children regardless of nesting depth.

use alloc::vec::Vec;

use super::SceneOp;
use crate::render::path::Path;
use crate::types::{Fixed, Point, Rect, Transform};

/// Bbox of a single leaf op in its own (post-per-op-transform) coords.
/// Returns `None` for group markers — caller must handle them via
/// [`direct_children_bboxes`].
pub fn op_bbox(op: &SceneOp) -> Option<Rect> {
    match op {
        SceneOp::GroupBegin { .. } | SceneOp::GroupEnd => None,
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
        } => {
            let p = Path {
                cmds: path.to_vec(),
            };
            p.bbox().map(|r| transform.apply_rect_bbox(r))
        }
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
        SceneOp::Label { pos, transform, .. } => Some(transform.apply_rect_bbox(Rect {
            x: pos.x,
            y: pos.y,
            w: Fixed::ZERO,
            h: Fixed::ZERO,
        })),
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
    }
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
pub fn direct_children_bboxes(ops: &[SceneOp]) -> Vec<Rect> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        match &ops[i] {
            SceneOp::GroupBegin { .. } => {
                let (sub_bbox, end) = subtree_bbox(ops, i);
                if let Some(r) = sub_bbox {
                    out.push(r);
                }
                i = end + 1;
            }
            SceneOp::GroupEnd => break,
            other => {
                if let Some(r) = op_bbox(other) {
                    out.push(r);
                }
                i += 1;
            }
        }
    }
    out
}

fn subtree_bbox(ops: &[SceneOp], begin_idx: usize) -> (Option<Rect>, usize) {
    let mut acc: Option<Rect> = None;
    let mut depth = 1usize;
    let mut i = begin_idx + 1;
    while i < ops.len() && depth > 0 {
        match &ops[i] {
            SceneOp::GroupBegin { .. } => depth += 1,
            SceneOp::GroupEnd => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            other => {
                if let Some(r) = op_bbox(other) {
                    acc = Some(match acc {
                        Some(a) => a.union(&r),
                        None => r,
                    });
                }
            }
        }
        i += 1;
    }
    (acc, i)
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
        let bboxes = direct_children_bboxes(&ops);
        assert_eq!(bboxes.len(), 2);
        assert!(pairwise_disjoint(&bboxes));
    }

    #[test]
    fn direct_children_bboxes_collapses_nested_group() {
        let ops = [
            SceneOp::GroupBegin {
                transform: None,
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
        let bboxes = direct_children_bboxes(&ops);
        assert_eq!(bboxes.len(), 2);
        assert!(pairwise_disjoint(&bboxes));
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
            path: alloc::borrow::Cow::Owned(path),
            transform: Transform::IDENTITY,
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        };
        let bbox = op_bbox(&op).unwrap();
        assert_eq!(bbox.x, Fixed::ZERO);
        assert_eq!(bbox.w, Fixed::from_int(8));
    }
}
