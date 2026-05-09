use crate::types::{Color, Fixed, Point, Rect};

use super::backend::DrawBackend;
use super::texture::Texture;

pub struct Painter<'a, B: DrawBackend> {
    pub backend: &'a mut B,
}

impl<'a, B: DrawBackend> Painter<'a, B> {
    pub fn new(backend: &'a mut B) -> Self {
        Self { backend }
    }

    pub fn fill_rect(&mut self, rect: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.backend.fill_rect(rect, clip, color, radius, opa);
    }

    pub fn draw_border(
        &mut self,
        rect: &Rect,
        clip: &Rect,
        color: &Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
    ) {
        self.backend
            .stroke_rect(rect, clip, width, color, radius, opa);
    }

    pub fn draw_image(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect) {
        self.backend.blit(src, src_rect, dst, clip);
    }

    pub fn clear(&mut self, area: &Rect, color: &Color) {
        self.backend.clear(area, color);
    }
}
