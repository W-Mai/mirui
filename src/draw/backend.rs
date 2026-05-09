use crate::types::{Color, Fixed, Point, Rect};

use super::path::Path;
use super::texture::Texture;

pub trait DrawBackend {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8);
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8);
    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, clip: &Rect);
    fn clear(&mut self, area: &Rect, color: &Color);
    fn flush(&mut self);
}
