use super::SwRenderer;
use crate::render::font::sdf::SignedDistanceField;
use crate::types::{Color, Fixed};

impl SwRenderer<'_> {
    /// Rasterize one SDF glyph into the current target.
    ///
    /// `cx`, `cy` are the top-left of the glyph in physical pixels;
    /// `target_size` is the rendered square size in physical pixels —
    /// the atlas (`source_size` square) is bilinear-resampled to it, so
    /// any target works, not just integer multiples. Coverage comes
    /// from a one-output-pixel-wide linear ramp around the zero
    /// distance, matching one-pixel anti-aliasing without explicit
    /// super-sampling.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(super) fn blit_sdf_glyph(
        &mut self,
        atlas: &[u8],
        source_size: u16,
        bit_depth: u8,
        spread: u16,
        cx: i32,
        cy: i32,
        target_size: u16,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        let layout = match bit_depth {
            4 => mirx::image::SampleLayout::A4,
            8 => mirx::image::SampleLayout::A8,
            _ => return,
        };
        let surface = mirx::image::SurfaceDescriptor::new(
            u32::from(source_size),
            u32::from(source_size),
            layout,
            mirx::image::ColorDescription::NONE,
        )
        .expect("supported scalar layout");
        let region = surface
            .region(0, 0, u32::from(source_size), u32::from(source_size))
            .unwrap();
        let stride = (u32::from(source_size) * u32::from(bit_depth)).div_ceil(8);
        self.blit_sdf_region(
            atlas,
            stride,
            region,
            bit_depth,
            spread,
            cx,
            cy,
            target_size,
            target_size,
            phys_bounds,
            color,
            opa,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn blit_sdf_region(
        &mut self,
        samples: &[u8],
        stride: u32,
        region: mirx::image::Region,
        bit_depth: u8,
        spread: u16,
        cx: i32,
        cy: i32,
        target_width: u16,
        target_height: u16,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let target_width = target_width.max(1) as i32;
        let target_height = target_height.max(1) as i32;
        let scale_x = Fixed::from_int(region.width() as i32) / Fixed::from_int(target_width);
        let scale_y = Fixed::from_int(region.height() as i32) / Fixed::from_int(target_height);
        let half_texel = Fixed::ONE / 2;
        let Some(field) = SignedDistanceField::new(samples, stride, region, bit_depth, spread)
        else {
            return;
        };
        let target_w = self.target.width as usize;
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());

        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for dy in 0..target_height {
            let py = cy + dy;
            if py < clip_y || py >= clip_y2 {
                continue;
            }
            let sy = (Fixed::from_int(dy) + half_texel) * scale_y - half_texel;
            let row_mask_off = py as usize * target_w;

            for dx in 0..target_width {
                let px = cx + dx;
                if px < clip_x || px >= clip_x2 {
                    continue;
                }
                let sx = (Fixed::from_int(dx) + half_texel) * scale_x - half_texel;

                let (dist, gradient_x, gradient_y) = field.sample_with_gradient(sx, sy);
                let screen_dx = gradient_x * scale_x;
                let screen_dy = gradient_y * scale_y;
                let edge_half = ((screen_dx * screen_dx + screen_dy * screen_dy).sqrt() / 2)
                    .max(Fixed::from_ratio(1, 256));
                let cov = if dist <= -edge_half {
                    continue;
                } else if dist >= edge_half {
                    Fixed::ONE
                } else {
                    (dist + edge_half) / (edge_half * 2)
                };

                let mut final_alpha =
                    (cov * Fixed::from_int(opa as i32)).to_int().clamp(0, 255) as u8;
                if final_alpha == 0 {
                    continue;
                }
                if let Some(mask) = clip_mask {
                    let ca = mask[row_mask_off + px as usize];
                    if ca == 0 {
                        continue;
                    }
                    if ca < 255 {
                        final_alpha = ((final_alpha as u16 * ca as u16 + 127) / 255) as u8;
                        if final_alpha == 0 {
                            continue;
                        }
                    }
                }
                target.blend_pixel_int(px, py, color, final_alpha);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/label_sdf.rs"]
mod tests;
