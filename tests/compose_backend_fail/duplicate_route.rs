use mirui::render::canvas::{Canvas, Paint};
use mirui::render::command::CompositeMode;
use mirui::render::font::Font;
use mirui::render::path::Path;
use mirui::render::texture::Texture;
use mirui::types::{Color, Fixed, Point, Rect};
use mirui_macros::compose_backend;

struct Dummy;
impl Canvas for Dummy {
    fn fill_path(&mut self, _: &Path, _: &Rect, _: &Paint, _: u8, _: ::mirui::render::raster::FillRule) {}
    fn stroke_path(&mut self, _: &Path, _: &Rect, _: Fixed, _: &Paint, _: u8, _: ::mirui::render::raster::LineCap, _: ::mirui::render::raster::LineJoin, _: ::mirui::types::Fixed, _: &[Fixed]) {}
    fn blit(&mut self, _: &Texture, _: &Rect, _: Point, _: Point, _: &Rect, _: u8, _: Fixed, _: CompositeMode) {}
    fn clear(&mut self, _: &Rect, _: &Color) {}
    fn draw_glyph_run(&mut self, _: &Point, _: &[mirui::text::PositionedGlyph], _: &Font, _: &Rect, _: &Color, _: u8) {}
    fn draw_posed_glyph_run(&mut self, _: &Point, _: mirui::render::PosedGlyphs<'_>, _: &Font, _: &Rect, _: &Color, _: u8) {}
    fn flush(&mut self) {}
}

compose_backend! {
    pub struct Broken {
        sw: Dummy,
        gpu: Dummy,
    }
    route {
        default => sw,
        blit => gpu,
        blit => sw,
    }
}

fn main() {}
