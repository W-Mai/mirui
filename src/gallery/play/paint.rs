use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::path::Path;
use crate::render::raster::FillRule;
use crate::render::scene::{LineCap, LineJoin, Paint};
use crate::types::Transform;
use crate::ui::view::PaintCtx;

/// Radius-one circle, scaled by `PlayPainter::disc`.
static UNIT_DISC: Path = crate::path!(
    "M 1 0 C 1 0.5523 0.5523 1 0 1 C -0.5523 1 -1 0.5523 -1 0 C -1 -0.5523 -0.5523 -1 0 -1 C 0.5523 -1 1 -0.5523 1 0 Z"
);

pub(crate) struct PlayPainter<'paint, 'frame> {
    paint: &'paint mut PaintCtx<'frame>,
    transform: Transform,
}

impl<'paint, 'frame> PlayPainter<'paint, 'frame> {
    pub(crate) fn new(paint: &'paint mut PaintCtx<'frame>, transform: Transform) -> Self {
        Self { paint, transform }
    }

    pub(crate) fn fill(&mut self, area: Rect, color: Color, radius: Fixed) {
        self.paint.draw(&DrawCommand::Fill {
            area,
            transform: self.transform,
            quad: None,
            color,
            radius,
            opa: 255,
        });
    }

    /// A filled circle, drawn as curves so it stays round at any size.
    pub(crate) fn disc(&mut self, center: Point, radius: Fixed, color: Color) {
        let size = Transform::scale(radius, radius);
        self.transformed(Transform::translate(center.x, center.y).compose(&size))
            .fill_path(&UNIT_DISC, color);
    }

    pub(crate) fn border(&mut self, area: Rect, color: Color, width: Fixed, radius: Fixed) {
        self.paint.draw(&DrawCommand::Border {
            area,
            transform: self.transform,
            quad: None,
            color,
            width,
            radius,
            opa: 255,
        });
    }

    pub(crate) fn line(&mut self, p1: Point, p2: Point, color: Color, width: Fixed) {
        self.paint.draw(&DrawCommand::Line {
            p1,
            p2,
            transform: self.transform,
            color,
            width,
            opa: 255,
        });
    }

    pub(crate) fn circle(&mut self, center: Point, radius: Fixed, color: Color) {
        self.fill(
            Rect {
                x: center.x - radius,
                y: center.y - radius,
                w: radius * Fixed::from_int(2),
                h: radius * Fixed::from_int(2),
            },
            color,
            radius,
        );
    }

    pub(crate) fn arc(
        &mut self,
        center: Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: Color,
        width: Fixed,
    ) {
        self.paint.draw(&DrawCommand::Arc {
            center,
            transform: self.transform,
            radius,
            start_angle,
            end_angle,
            color,
            width,
            opa: 255,
        });
    }

    /// Fill a path. The alpha of `color` is the opacity of the fill.
    pub(crate) fn fill_path(&mut self, path: &Path, color: Color) {
        self.paint.draw(&DrawCommand::FillPath {
            path,
            transform: self.transform,
            paint: &Paint::Color(Color { a: 255, ..color }.into()),
            opa: color.a,
            fill_rule: FillRule::NonZero,
        });
    }

    /// Stroke a path with round caps and joins. The alpha of `color` is the opacity.
    pub(crate) fn stroke_path(&mut self, path: &Path, color: Color, width: Fixed) {
        self.paint.draw(&DrawCommand::StrokePath {
            path,
            transform: self.transform,
            paint: &Paint::Color(Color { a: 255, ..color }.into()),
            width,
            opa: color.a,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        });
    }

    /// A painter whose drawing is first moved by `local`, for a part that carries its own origin, angle or scale.
    pub(crate) fn transformed(&mut self, local: Transform) -> PlayPainter<'_, 'frame> {
        PlayPainter {
            paint: &mut *self.paint,
            transform: self.transform.compose(&local),
        }
    }
}
