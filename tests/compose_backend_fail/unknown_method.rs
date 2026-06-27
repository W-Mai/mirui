use mirui::render::canvas::Canvas;
use mirui::render::font::Font;
use mirui::render::path::Path;
use mirui::render::texture::Texture;
use mirui::types::{Color, Fixed, Point, Rect};
use mirui_macros::compose_backend;

struct Dummy;
impl Canvas for Dummy {
    fn fill_path(&mut self, _: &Path, _: &Rect, _: &Color, _: u8) {}
    fn stroke_path(&mut self, _: &Path, _: &Rect, _: Fixed, _: &Color, _: u8) {}
    fn blit(&mut self, _: &Texture, _: &Rect, _: Point, _: Point, _: &Rect, _: u8) {}
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_label(&mut self, _: &Point, _: &str, _: &Font, _: &Rect, _: &Color, _: u8) {}
    fn flush(&mut self) {}
}

compose_backend! {
    pub struct Broken {
        sw: Dummy,
    }
    route {
        default => sw,
        fil_path => sw,
    }
}

fn main() {}
