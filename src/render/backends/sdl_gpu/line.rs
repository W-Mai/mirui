use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::render::path::Path;
use crate::render::renderer::RenderError;
use crate::types::{Color, Fixed, Point, Rect};

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn draw_line_inner(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        if width == Fixed::ONE {
            // 1px line → SDL draw_line, no tessellation.
            let phys_clip = self.viewport.rect_to_physical(*clip);
            let phys_p1 = self.viewport.point_to_physical(p1);
            let phys_p2 = self.viewport.point_to_physical(p2);
            let Some(clip_rect) = sdl_pixel_rect(&phys_clip, &phys_clip) else {
                return;
            };
            let mut canvas = match self.device.canvas() {
                Ok(canvas) => canvas,
                Err(error) => {
                    self.failure.get_or_insert(error);
                    return;
                }
            };
            canvas.set_clip_rect(clip_rect);
            apply_solid_color(&mut canvas, color, opa);
            if canvas
                .draw_line(
                    sdl2::rect::Point::new(phys_p1.x.to_int(), phys_p1.y.to_int()),
                    sdl2::rect::Point::new(phys_p2.x.to_int(), phys_p2.y.to_int()),
                )
                .is_err()
            {
                self.failure.get_or_insert(RenderError::BackendFailure);
            }
            canvas.set_clip_rect(None);
            return;
        }
        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        self.stroke_path_inner(&path, clip, width, color, opa);
    }
}
