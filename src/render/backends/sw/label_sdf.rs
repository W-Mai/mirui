use super::SwRenderer;
use crate::render::font::sdf::sample_signed_distance;
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
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let target_size = target_size.max(1) as i32;
        // Source pixels per output pixel. At an integer N× this is
        // exactly 1/N, so the integer-scale path stays byte-identical.
        let inv_scale = Fixed::from_int(source_size as i32) / Fixed::from_int(target_size);
        let half_texel = Fixed::ONE / 2;
        // AA ramp half-width = half an output pixel, in source pixels.
        let edge_half = inv_scale / 2;

        for dy in 0..target_size {
            let py = cy + dy;
            if py < clip_y || py >= clip_y2 {
                continue;
            }
            // Output-pixel-center to source-texel-index transform: the
            // -half_texel shift maps texel index i to its centre at
            // continuous i+0.5, so a 1:1 atlas samples texel centres
            // exactly. Without it, sampling lands on texel boundaries
            // and bilinear averages a solid stem's interior with its
            // outside neighbour, halving the coverage.
            let sy = (Fixed::from_int(dy) + half_texel) * inv_scale - half_texel;

            for dx in 0..target_size {
                let px = cx + dx;
                if px < clip_x || px >= clip_x2 {
                    continue;
                }
                let sx = (Fixed::from_int(dx) + half_texel) * inv_scale - half_texel;

                let dist = sample_signed_distance(atlas, source_size, bit_depth, spread, sx, sy);
                let cov = if dist <= -edge_half {
                    continue;
                } else if dist >= edge_half {
                    Fixed::ONE
                } else {
                    (dist + edge_half) / (edge_half * 2)
                };

                let final_alpha = (cov * Fixed::from_int(opa as i32)).to_int().clamp(0, 255) as u8;
                if final_alpha == 0 {
                    continue;
                }
                self.target.blend_pixel_int(px, py, color, final_alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::backends::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use alloc::vec;

    fn pixel_alpha(buf: &[u8], stride: usize, x: usize, y: usize) -> u8 {
        // RGBA8888: byte 3 of (y * stride + x) * 4.
        buf[(y * stride + x) * 4 + 3]
    }

    /// Build a 4×4 4-bit atlas where the inner 2×2 is "deep inside"
    /// (q = 15) and the outer ring is "deep outside" (q = 0). Blitting
    /// it should paint the centre opaque and leave the corners blank.
    fn build_inside_outside_atlas() -> alloc::vec::Vec<u8> {
        let mut atlas = vec![0u8; 8];
        let set = |buf: &mut [u8], x: usize, y: usize, q: u8| {
            let idx = y * 4 + x;
            let byte_idx = idx >> 1;
            if idx & 1 == 0 {
                buf[byte_idx] = (buf[byte_idx] & 0xF0) | (q & 0x0F);
            } else {
                buf[byte_idx] = (buf[byte_idx] & 0x0F) | ((q & 0x0F) << 4);
            }
        };
        for y in 0..4 {
            for x in 0..4 {
                let inside = (1..=2).contains(&x) && (1..=2).contains(&y);
                set(&mut atlas, x, y, if inside { 15 } else { 0 });
            }
        }
        atlas
    }

    #[test]
    fn blit_inside_pixel_opaque_outside_blank() {
        let mut buf = vec![0u8; 8 * 8 * 4];
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let atlas = build_inside_outside_atlas();
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_sdf_glyph(&atlas, 4, 4, 1, 0, 0, 4, (0, 0, 8, 8), &color, 255);

        // Centre of the inside region (source x=1.5..2.5) → opaque.
        assert!(
            pixel_alpha(&buf, 8, 1, 1) >= 200,
            "expected high alpha inside, got {}",
            pixel_alpha(&buf, 8, 1, 1)
        );
        // Corner pixel: source 0..1 is in the deep-outside region →
        // blank.
        assert_eq!(
            pixel_alpha(&buf, 8, 0, 0),
            0,
            "expected blank corner, got {}",
            pixel_alpha(&buf, 8, 0, 0)
        );
    }

    /// Build an 8×8 4-bit atlas with a 1px-wide vertical stem at
    /// column 4. Inside the stem distance is +; one px either side is
    /// the edge; beyond is outside. This is the thin-stroke case that
    /// a fixed-width coverage ramp renders at ~50% (the faded-stem
    /// bug); the gradient-normalized ramp must resolve it to solid.
    fn build_thin_stem_atlas(spread: u16) -> alloc::vec::Vec<u8> {
        let n = 8usize;
        let mut atlas = vec![0u8; n * n / 2];
        let set = |buf: &mut [u8], x: usize, y: usize, q: u8| {
            let idx = y * n + x;
            let bi = idx >> 1;
            if idx & 1 == 0 {
                buf[bi] = (buf[bi] & 0xF0) | (q & 0x0F);
            } else {
                buf[bi] = (buf[bi] & 0x0F) | ((q & 0x0F) << 4);
            }
        };
        // Signed distance to the stem at column 4 (stem occupies x∈[4,5)).
        // Encode like the host quantizer: q = round(d/spread*7.5 + 7.5).
        for y in 0..n {
            for x in 0..n {
                let cx = x as f32 + 0.5;
                let d = 0.5 - (cx - 4.5).abs();
                let dc = d.clamp(-(spread as f32), spread as f32);
                let q = (dc / spread as f32 * 7.5 + 7.5).round().clamp(0.0, 15.0) as u8;
                set(&mut atlas, x, y, q);
            }
        }
        atlas
    }

    #[test]
    fn thin_stem_renders_solid_not_half_coverage() {
        let spread = 2u16;
        let atlas = build_thin_stem_atlas(spread);
        let mut buf = vec![0u8; 8 * 8 * 4];
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_sdf_glyph(&atlas, 8, 4, spread, 0, 0, 8, (0, 0, 8, 8), &color, 255);

        // The stem column (x=4) must read as solid, not the ~137/255
        // (≈54%) the fixed-ramp produced. Gradient normalization should
        // push the stem-center coverage high.
        let stem = pixel_alpha(&buf, 8, 4, 3);
        assert!(
            stem >= 200,
            "thin stem should render near-solid, got alpha {stem}",
        );
    }

    #[test]
    fn blit_respects_clip_bounds() {
        let mut buf = vec![0u8; 8 * 8 * 4];
        let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let atlas = build_inside_outside_atlas();
        let color = Color::rgba(255, 255, 255, 255);
        // Tight clip: only y < 2 allowed.
        backend.blit_sdf_glyph(&atlas, 4, 4, 1, 0, 0, 4, (0, 0, 8, 2), &color, 255);

        // Pixel below clip stays blank even though the atlas would
        // otherwise paint it opaque.
        assert_eq!(pixel_alpha(&buf, 8, 1, 2), 0);
        assert_eq!(pixel_alpha(&buf, 8, 1, 3), 0);
    }

    #[test]
    fn blit_upscales_to_non_integer_target() {
        // 4px source atlas rendered to a 10px target (2.5×, not an
        // integer multiple) — the path the zoom animation relies on.
        // The inside region (source 1..3) maps to roughly target
        // 2.5..7.5, so the centre is solid and it paints a 10px box.
        let mut buf = vec![0u8; 16 * 16 * 4];
        let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let atlas = build_inside_outside_atlas();
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_sdf_glyph(&atlas, 4, 4, 1, 0, 0, 10, (0, 0, 16, 16), &color, 255);

        // Centre of the 10px render is deep inside → opaque.
        assert!(
            pixel_alpha(&buf, 16, 5, 5) >= 200,
            "expected solid centre at 2.5x, got {}",
            pixel_alpha(&buf, 16, 5, 5)
        );
        // Far corner is outside → blank.
        assert_eq!(pixel_alpha(&buf, 16, 0, 0), 0);
        // Coverage reaches into the lower rows, proving it scaled past
        // the source's 4px extent.
        assert!(
            pixel_alpha(&buf, 16, 5, 7) > 0,
            "render did not reach 10px tall"
        );
    }
}
