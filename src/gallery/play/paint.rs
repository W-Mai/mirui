use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::types::Transform;
use crate::ui::view::PaintCtx;

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
}
