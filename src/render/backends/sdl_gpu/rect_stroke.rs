use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::render::path::Path;
use crate::render::renderer::RenderError;
use crate::types::{Color, Fixed, Rect};

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn stroke_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        if radius == Fixed::ZERO && width == Fixed::ONE {
            // 1px axis-aligned border: SDL draws it directly.
            let phys_area = self.viewport.rect_to_physical(*area);
            let phys_clip = self.viewport.rect_to_physical(*clip);
            if let Some(sdl_rect) = sdl_pixel_rect(&phys_area, &phys_clip) {
                let mut canvas = match self.device.canvas() {
                    Ok(canvas) => canvas,
                    Err(error) => {
                        self.failure.get_or_insert(error);
                        return;
                    }
                };
                apply_solid_color(&mut canvas, color, opa);
                if canvas.draw_rect(sdl_rect).is_err() {
                    self.failure.get_or_insert(RenderError::BackendFailure);
                }
            }
            return;
        }
        // Center stroke on the geometric rect edge, so outer edge stays at `area`.
        let path = Path::rounded_rect(
            area.x + width / 2,
            area.y + width / 2,
            area.w - width,
            area.h - width,
            (radius - width / 2).max(Fixed::ZERO),
        );
        self.stroke_path_inner(&path, clip, width, color, opa);
    }
}
