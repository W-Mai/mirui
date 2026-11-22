use crate::raster::sprites::SpritePaint;
use mirui::types::Fixed;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct LayerEntrance {
    pub elapsed: Fixed,
    pub delay_per_layer: Fixed,
    pub duration: Fixed,
    pub viewport_height_scale: Fixed,
    pub distance: Fixed,
}

impl LayerEntrance {
    pub(crate) fn offset(self, layer: usize, first: usize, height: usize) -> Option<Fixed> {
        let remaining = self.elapsed
            - Fixed::from_int(layer.saturating_sub(first) as i32) * self.delay_per_layer;
        let progress = remaining / self.duration;
        if progress <= Fixed::ZERO {
            return None;
        }
        let offset = if progress < Fixed::ONE {
            -(Fixed::ONE - progress * progress)
                * (Fixed::from_int(height as i32) * self.viewport_height_scale + self.distance)
        } else {
            Fixed::ZERO
        };
        Some(offset)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SampledStyle {
    pub axes: [[Fixed; 3]; 3],
    pub scale: Fixed,
    pub layer_spacing: Fixed,
    pub depth_radius: Fixed,
    pub max_cell_pixels: Fixed,
    pub cull_margin: Fixed,
    pub entrance: LayerEntrance,
    pub paint: SpritePaint,
    pub palettes: [&'static [u32]; 2],
}
