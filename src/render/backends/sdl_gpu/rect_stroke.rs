use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::render::path::Path;
use crate::types::{Color, Fixed, Rect};

impl SdlGpuRenderer<'_> {
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
                apply_solid_color(self.canvas, color, opa);
                let _ = self.canvas.draw_rect(sdl_rect);
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
