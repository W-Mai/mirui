use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::render::path::rounded_rect_commands;
use crate::render::renderer::RenderError;
use crate::types::{Color, Fixed, Rect};

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn fill_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.physical_draw_clip(*clip);
        if radius == Fixed::ZERO {
            // Axis-aligned fast path: ask SDL's hardware-accelerated
            // fill_rect directly, no tessellation.
            if let Some(sdl_rect) = sdl_pixel_rect(&phys_area, &phys_clip) {
                let mut canvas = match self.device.canvas() {
                    Ok(canvas) => canvas,
                    Err(error) => {
                        self.failure.get_or_insert(error);
                        return;
                    }
                };
                canvas.set_clip_rect(sdl_pixel_rect(&phys_clip, &phys_clip));
                apply_solid_color(&mut canvas, color, opa);
                if canvas.fill_rect(sdl_rect).is_err() {
                    self.failure.get_or_insert(RenderError::BackendFailure);
                }
                self.device.finish_draw(&mut canvas);
            }
            return;
        }
        let path = rounded_rect_commands(area.x, area.y, area.w, area.h, radius);
        self.fill_path_inner(
            path.commands(),
            clip,
            color,
            opa,
            crate::render::raster::FillRule::NonZero,
        );
    }
}
