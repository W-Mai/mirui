use crate::types::{Color, Fixed, Point, Rect};

use super::path::Path;
use super::texture::Texture;

pub trait DrawBackend {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8);
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8);
    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect);
    fn clear(&mut self, area: &Rect, color: &Color);
    fn flush(&mut self);

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        let path = Path::rounded_rect(area.x, area.y, area.w, area.h, radius);
        self.fill_path(&path, clip, color, opa);
    }

    fn stroke_rect(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let path = Path::rounded_rect(
            area.x + width / 2,
            area.y + width / 2,
            area.w - width,
            area.h - width,
            (radius - width / 2).max(Fixed::ZERO),
        );
        self.stroke_path(&path, clip, width, color, opa);
    }
}
