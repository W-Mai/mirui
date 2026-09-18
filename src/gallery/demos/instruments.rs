use crate::ecs::World;
use crate::render::command::DrawCommand;
use crate::render::font::{Font, FontManager, FontToken};
use crate::render::renderer::{DrawRequest, RenderError, Renderer};
use crate::types::{Color, Fixed, Point, Rect, Transform};

const FONT_BYTES: &[u8] = include_bytes!("assets/misans_ui.mirx");

pub(super) fn register_fonts(world: &mut World) {
    let Some(manager) = world.resource::<FontManager>() else {
        return;
    };
    let base = Font::from_mirx(
        "Product Instruments",
        24,
        FONT_BYTES,
        &mirx::reader::PayloadLimits::HOST,
    )
    .expect("product instrument font");
    manager.add_static(FontToken::Default.cache_key(), base.clone());
    manager.add_static(FontToken::Heading.cache_key(), base.clone());
    manager.add_static(FontToken::Mono.cache_key(), base);
}

pub(super) struct InstrumentPainter<'a> {
    renderer: &'a mut dyn Renderer,
    clip: Rect,
    transform: Transform,
    error: Option<RenderError>,
}

impl<'a> InstrumentPainter<'a> {
    pub(super) fn new(renderer: &'a mut dyn Renderer, clip: Rect, transform: Transform) -> Self {
        Self {
            renderer,
            clip,
            transform,
            error: None,
        }
    }

    fn draw(&mut self, command: &DrawCommand<'_>) {
        if self.error.is_none() {
            self.error = self
                .renderer
                .submit(&DrawRequest::new(command, self.clip))
                .err();
        }
    }

    pub(super) fn fill(&mut self, area: Rect, color: Color, radius: Fixed, opacity: u8) {
        self.draw(&DrawCommand::Fill {
            area,
            transform: self.transform,
            quad: None,
            color,
            radius,
            opa: opacity,
        });
    }

    pub(super) fn dot(&mut self, center: Point, radius: Fixed, color: Color, opacity: u8) {
        self.fill(
            Rect::new(center.x - radius, center.y - radius, radius * 2, radius * 2),
            color,
            radius,
            opacity,
        );
    }

    pub(super) fn line(
        &mut self,
        start: Point,
        end: Point,
        color: Color,
        width: Fixed,
        opacity: u8,
    ) {
        self.draw(&DrawCommand::Line {
            p1: start,
            p2: end,
            transform: self.transform,
            color,
            width,
            opa: opacity,
        });
    }

    pub(super) fn finish(self) -> Result<(), RenderError> {
        self.error.map_or(Ok(()), Err)
    }
}
