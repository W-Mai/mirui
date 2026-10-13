use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::path::{Path, PathCmd};
use crate::render::raster::FillRule;
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::types::{Fixed, Fixed64, Point, Rect, Transform, Transform3D};

const UNIT_RECT: Path = Path::from_static(&[
    PathCmd::MoveTo(Point::ZERO),
    PathCmd::LineTo(Point {
        x: Fixed::ONE,
        y: Fixed::ZERO,
    }),
    PathCmd::LineTo(Point {
        x: Fixed::ONE,
        y: Fixed::ONE,
    }),
    PathCmd::LineTo(Point {
        x: Fixed::ZERO,
        y: Fixed::ONE,
    }),
    PathCmd::Close,
]);

fn placement(rect: Rect, affine: Transform, projective: Transform3D) -> Transform3D {
    let transform = if projective.is_identity() {
        Transform3D::from(affine)
    } else {
        projective
    };
    transform.compose(&Transform3D::translate(rect.x, rect.y))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    world: &World,
    entity: Entity,
    rect: Rect,
    affine: Transform,
    projective: Transform3D,
    renderer: &mut dyn Renderer,
    clip: Rect,
    body: impl FnOnce(&mut dyn Renderer, Rect) -> Result<(), RenderError>,
) -> Result<(), RenderError> {
    let Some(source_clip) = super::clip(world, entity) else {
        return body(renderer, clip);
    };
    if source_clip.w <= Fixed::ZERO || source_clip.h <= Fixed::ZERO {
        return Ok(());
    }
    let Some((global_rect, global_affine, global_projective)) =
        crate::ui::geometry::widget_render_geometry(world, entity)
    else {
        return Ok(());
    };
    let global = placement(global_rect, global_affine, global_projective);
    let local = placement(rect, affine, projective);
    let screen_to_target = if local == global {
        Transform3D::IDENTITY
    } else {
        let Some(inverse) = global.inverse() else {
            return Ok(());
        };
        local.compose(&inverse)
    };
    let Some(corners) = screen_to_target.apply_rect(source_clip) else {
        return Ok(());
    };
    let Some(clip) = clip.intersect(&Rect::bounding_quad(&corners)) else {
        return Ok(());
    };
    let axis_aligned = screen_to_target.m20 == Fixed64::ZERO
        && screen_to_target.m21 == Fixed64::ZERO
        && ((screen_to_target.m01 == Fixed64::ZERO && screen_to_target.m10 == Fixed64::ZERO)
            || (screen_to_target.m00 == Fixed64::ZERO && screen_to_target.m11 == Fixed64::ZERO));
    if axis_aligned {
        return body(renderer, clip);
    }

    let transform = screen_to_target
        .compose(&Transform3D::translate(source_clip.x, source_clip.y))
        .compose(&Transform3D::scale(source_clip.w, source_clip.h));
    renderer.submit(
        &DrawRequest::new(
            &DrawCommand::PushClip {
                path: &UNIT_RECT,
                transform: Transform::IDENTITY,
                fill_rule: FillRule::EvenOdd,
            },
            clip,
        )
        .with_projective(transform),
    )?;
    let result = body(renderer, clip);
    let pop = renderer.submit(&DrawRequest::new(&DrawCommand::PopClip, clip));
    result.and(pop)
}
