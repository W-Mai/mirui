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

    pub fn draw_text(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        self.backend.draw_label(pos, text, clip, color, opa);
    }

    pub fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.backend.fill_path(path, clip, color, opa);
    }

    pub fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.backend.stroke_path(path, clip, width, color, opa);
    }

    pub fn draw_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        self.backend.draw_line(p1, p2, clip, width, color, opa);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_arc(
        &mut self,
        center: Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        self.backend.draw_arc(
            center,
            radius,
            start_angle,
            end_angle,
            clip,
            width,
            color,
            opa,
        );
    }

    pub fn clear(&mut self, area: &Rect, color: &Color) {
        self.backend.clear(area, color);
    }
}
