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
#[path = "../../../tests/unit/render/scene/bbox.rs"]
mod tests;
