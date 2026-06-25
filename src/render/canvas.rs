use crate::types::{Color, Fixed, Point, Rect};

use super::font::Font;
use super::path::Path;
use super::texture::Texture;

/// Rasterization interface. All coordinate parameters (`area`, `clip`,
/// `pos`, path points, widths, radii, `dst`, `dst_size`) are in
/// **logical pixels**; implementations convert to physical internally
/// (typically by holding a `Viewport` field).
pub trait Canvas {
    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8);
    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8);
    fn blit(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
    );
    fn clear(&mut self, area: &Rect, color: &Color);
    fn draw_label(
        &mut self,
        pos: &Point,
        text: &[u8],
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    );
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

    fn draw_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        self.stroke_path(&path, clip, width, color, opa);
    }

    /// Draw a stroked circular arc. Angles in degrees, CCW from +X axis.
    // Signature locked by design.md §8 (user-confirmed):
    // (center, radius, start_angle, end_angle) + standard stroke params.
    #[allow(clippy::too_many_arguments)]
    fn draw_arc(
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
        let path = Path::arc(center, radius, start_angle, end_angle);
        self.stroke_path(&path, clip, width, color, opa);
    }
}
