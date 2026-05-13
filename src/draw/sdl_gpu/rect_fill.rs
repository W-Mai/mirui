use super::{SdlGpuRenderer, apply_solid_color, sdl_pixel_rect};
use crate::draw::path::Path;
use crate::types::{Color, Fixed, Rect};

impl SdlGpuRenderer<'_> {
    pub(super) fn fill_rect_inner(
        &mut self,
        area: &Rect,
        clip: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        let phys_area = self.viewport.rect_to_physical(*area);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        if radius == Fixed::ZERO {
            // Axis-aligned fast path: ask SDL's hardware-accelerated
            // fill_rect directly, no tessellation.
            if let Some(sdl_rect) = sdl_pixel_rect(&phys_area, &phys_clip) {
                apply_solid_color(self.canvas, color, opa);
                let _ = self.canvas.fill_rect(sdl_rect);
            }
            return;
        }
        let path = Path::rounded_rect(area.x, area.y, area.w, area.h, radius);
        self.fill_path_inner(&path, clip, color, opa);
    }
}
