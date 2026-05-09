use crate::types::{Color, Fixed, Point, Rect};

use super::backend::DrawBackend;
use super::path::Path;
use super::texture::Texture;

pub struct Painter<'a, B: DrawBackend> {
    pub backend: &'a mut B,
}

impl<'a, B: DrawBackend> Painter<'a, B> {
    pub fn new(backend: &'a mut B) -> Self {
        Self { backend }
    }

    pub fn fill_rect(&mut self, rect: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        let path = Path::rounded_rect(rect.x, rect.y, rect.w, rect.h, radius);
        self.backend.fill_path(&path, clip, color, opa);
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
        let path = Path::rounded_rect(
            rect.x + width / 2,
            rect.y + width / 2,
            rect.w - width,
            rect.h - width,
            (radius - width / 2).max(Fixed::ZERO),
        );
        self.backend.stroke_path(&path, clip, width, color, opa);
    }

    pub fn draw_image(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect) {
        self.backend.blit(src, src_rect, dst, clip);
    }

    pub fn clear(&mut self, area: &Rect, color: &Color) {
        self.backend.clear(area, color);
    }
}
