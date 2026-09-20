use crate::prelude::*;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::view::ViewCtx;

pub(crate) struct PlayPainter<'a, 'b> {
    renderer: &'a mut dyn Renderer,
    ctx: &'a mut ViewCtx<'b>,
    transform: Transform,
    clip: Rect,
}

impl<'a, 'b> PlayPainter<'a, 'b> {
    pub(crate) fn new(
        renderer: &'a mut dyn Renderer,
        ctx: &'a mut ViewCtx<'b>,
        transform: Transform,
        clip: Rect,
    ) -> Self {
        Self {
            renderer,
            ctx,
            transform,
            clip,
        }
    }

    pub(crate) fn fill(&mut self, area: Rect, color: Color, radius: Fixed) {
        self.ctx.draw(
            self.renderer,
            &DrawCommand::Fill {
                area,
                transform: self.transform,
                quad: None,
                color,
                radius,
                opa: 255,
            },
            &self.clip,
        );
    }

    pub(crate) fn border(&mut self, area: Rect, color: Color, width: Fixed, radius: Fixed) {
        self.ctx.draw(
            self.renderer,
            &DrawCommand::Border {
                area,
                transform: self.transform,
                quad: None,
                color,
                width,
                radius,
                opa: 255,
            },
            &self.clip,
        );
    }

    pub(crate) fn line(&mut self, p1: Point, p2: Point, color: Color, width: Fixed) {
        self.ctx.draw(
            self.renderer,
            &DrawCommand::Line {
                p1,
                p2,
                transform: self.transform,
                color,
                width,
                opa: 255,
            },
            &self.clip,
        );
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
        self.ctx.draw(
            self.renderer,
            &DrawCommand::Arc {
                center,
                transform: self.transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa: 255,
            },
            &self.clip,
        );
    }
}
