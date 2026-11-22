use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::render::path::rounded_rect_commands;
use crate::render::renderer::RenderError;
use crate::types::{Color, Fixed, Rect};

fn stroke_pixel_rect<T: sdl2::render::RenderTarget>(
    canvas: &mut sdl2::render::Canvas<T>,
    area: &Rect,
    clip: &Rect,
    retain_clip: bool,
) -> Result<(), RenderError> {
    let Some(scissor) = sdl_pixel_rect(area, clip) else {
        return Ok(());
    };
    let Some(outline) = sdl_pixel_rect(area, area) else {
        return Ok(());
    };
    canvas.set_clip_rect(scissor);
    let result = canvas.draw_rect(outline);
    if !retain_clip {
        canvas.set_clip_rect(None);
    }
    result.map_err(|_| RenderError::BackendFailure)
}

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
            let phys_clip = self.physical_draw_clip(*clip);
            if phys_area.intersect(&phys_clip).is_some() {
                let mut canvas = match self.device.canvas() {
                    Ok(canvas) => canvas,
                    Err(error) => {
                        self.failure.get_or_insert(error);
                        return;
                    }
                };
                apply_solid_color(&mut canvas, color, opa);
                if let Err(error) = stroke_pixel_rect(
                    &mut canvas,
                    &phys_area,
                    &phys_clip,
                    self.device.frame_active(),
                ) {
                    self.failure.get_or_insert(error);
                }
            }
            return;
        }
        // Center stroke on the geometric rect edge, so outer edge stays at `area`.
        let path = rounded_rect_commands(
            area.x + width / 2,
            area.y + width / 2,
            area.w - width,
            area.h - width,
            (radius - width / 2).max(Fixed::ZERO),
        );
        self.stroke_path_inner(path.commands(), clip, width, color, opa);
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sdl_gpu/rect_stroke.rs"]
mod tests;
