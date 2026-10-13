//! Shared widget-space transforms for drawing and input.

use super::Parent;
use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect, Transform, Transform3D};
use crate::ui::widgets::transform::WidgetTransform;
use crate::ui::widgets::transform_3d::{TransformOrigin, WidgetTransform3D};

/// Compose the parent transform with the local transform around the widget's center.
pub(super) fn effective_transform(
    parent: &Transform,
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Transform {
    let local = match world.get::<WidgetTransform>(entity) {
        Some(t) if !t.0.is_identity() => t.0,
        _ => return *parent,
    };
    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);
    let to_origin = Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy);
    let from_origin = Transform::translate(cx, cy);
    parent
        .compose(&from_origin)
        .compose(&local)
        .compose(&to_origin)
}

fn origin_point(world: &World, entity: Entity, rect: Rect) -> (Fixed, Fixed) {
    let o = world
        .get::<TransformOrigin>(entity)
        .copied()
        .unwrap_or_default();
    (rect.x + rect.w * o.x, rect.y + rect.h * o.y)
}

pub(super) fn effective_transform_3d(
    parent: &Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
) -> Transform3D {
    let local = match world.get::<WidgetTransform3D>(entity) {
        Some(t) if !t.0.is_identity() => t.0,
        _ => return *parent,
    };
    let (cx, cy) = origin_point(world, entity, rect);
    let to_origin = Transform3D::translate(Fixed::ZERO - cx, Fixed::ZERO - cy);
    let from_origin = Transform3D::translate(cx, cy);
    parent
        .compose(&from_origin)
        .compose(&local)
        .compose(&to_origin)
}

pub(crate) fn render_transforms(
    parent_affine: Transform,
    parent_projective: Transform3D,
    world: &World,
    entity: Entity,
    rect: Rect,
) -> (Transform, Transform3D) {
    let local_affine = effective_transform(&Transform::IDENTITY, world, entity, rect);
    let local_projective = effective_transform_3d(&Transform3D::IDENTITY, world, entity, rect);
    if parent_projective.is_identity() && local_projective.is_identity() {
        return (parent_affine.compose(&local_affine), Transform3D::IDENTITY);
    }
    let outer = if parent_projective.is_identity() {
        Transform3D::from_affine(parent_affine)
    } else {
        parent_projective
    };
    (
        Transform::IDENTITY,
        outer
            .compose(&local_projective)
            .compose(&Transform3D::from_affine(local_affine)),
    )
}

pub(crate) fn widget_render_geometry(
    world: &World,
    entity: Entity,
) -> Option<(Rect, Transform, Transform3D)> {
    fn resolve(
        world: &World,
        entity: Entity,
        depth: u16,
    ) -> Option<(Rect, Transform, Transform3D, Fixed, Fixed)> {
        if depth == 0 {
            return None;
        }
        let (parent_affine, parent_projective, offset_x, offset_y) = if let Some(parent) =
            world.get::<Parent>(entity).map(|parent| parent.0)
        {
            let (_, affine, projective, offset_x, offset_y) = resolve(world, parent, depth - 1)?;
            (affine, projective, offset_x, offset_y)
        } else {
            (
                Transform::IDENTITY,
                Transform3D::IDENTITY,
                Fixed::ZERO,
                Fixed::ZERO,
            )
        };
        let computed = world.get::<super::ComputedRect>(entity)?.0;
        let rect = Rect {
            x: computed.x - offset_x,
            y: computed.y - offset_y,
            w: computed.w,
            h: computed.h,
        };
        let (affine, projective) =
            render_transforms(parent_affine, parent_projective, world, entity, rect);
        let (child_offset_x, child_offset_y) = world
            .get::<crate::input::event::scroll::ScrollOffset>(entity)
            .map_or((offset_x, offset_y), |scroll| {
                (offset_x + scroll.x, offset_y + scroll.y)
            });
        Some((rect, affine, projective, child_offset_x, child_offset_y))
    }

    resolve(world, entity, 1_024).map(|(rect, affine, projective, _, _)| (rect, affine, projective))
}

pub(crate) fn transformed_clip_bounds(
    rect: Rect,
    affine: Transform,
    projective: Transform3D,
) -> Option<Rect> {
    if projective.is_identity() {
        Some(affine.apply_rect_bbox(rect))
    } else {
        projective
            .apply_rect(rect)
            .map(|quad| Rect::bounding_quad(&quad))
    }
}
