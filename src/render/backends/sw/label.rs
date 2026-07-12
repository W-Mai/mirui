use super::SwRenderer;
use crate::render::font::{Font, GlyphKind};
use crate::types::{Color, Fixed, Point, Rect};

impl SwRenderer<'_> {
    pub(super) fn draw_label_inner(
        &mut self,
        pos: &Point,
        text: &str,
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        let phys_pos = self.viewport.point_to_physical(*pos);
        let phys_clip = self.viewport.rect_to_physical(*clip);
        let scale = self.viewport.scale().to_int().max(1);
        let phys_bounds = phys_clip.pixel_bounds();
        let (mut cx, cy) = phys_pos.floor();
        let metrics = font.metrics();
        let char_h = metrics.line_height as i32;
        let requested_size = (font.size as i32 * scale).clamp(1, u16::MAX as i32) as u16;
        for ch in text.chars() {
            let Some(g) = font.glyph(ch, requested_size) else {
                continue;
            };
            let advance;
            match &g.kind {
                GlyphKind::Mono(bitmap) => {
                    advance = g.advance as i32 * scale;
                    self.blit_mono_glyph(bitmap, cx, cy, scale, char_h, phys_bounds, color, opa);
                }
                GlyphKind::Sdf {
                    atlas,
                    source_size,
                    bit_depth,
                    spread,
                    ..
                } => {
                    advance =
                        g.advance as i32 * requested_size as i32 / (*source_size as i32).max(1);
                    self.blit_sdf_glyph(
                        atlas,
                        *source_size,
                        *bit_depth,
                        *spread,
                        cx,
                        cy,
                        requested_size,
                        phys_bounds,
                        color,
                        opa,
                    );
                }
                GlyphKind::Grayscale {
                    coverage,
                    bpp,
                    w,
                    h,
                    ..
                } => {
                    advance = g.advance as i32 * scale;
                    self.blit_gray_glyph(coverage, *bpp, *w, *h, cx, cy, phys_bounds, color, opa);
                }
            }
            cx += advance;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_mono_glyph(
        &mut self,
        bitmap: &[u8],
        cx: i32,
        cy: i32,
        scale: i32,
        char_h: i32,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        for row in 0..char_h.min(bitmap.len() as i32) {
            let byte = bitmap[row as usize];
            for col in 0..8 {
                if byte & (0x80 >> col) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = cx + col * scale + sx;
                        let py = cy + row * scale + sy;
                        if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                            let alpha = match clip_mask {
                                Some(m) => {
                                    let ca = m[py as usize * target_w + px as usize];
                                    if ca == 0 {
                                        continue;
                                    }
                                    ((opa as u16 * ca as u16 + 127) / 255) as u8
                                }
                                None => opa,
                            };
                            self.target.blend_pixel(
                                Fixed::from_int(px),
                                Fixed::from_int(py),
                                color,
                                alpha,
                            );
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_gray_glyph(
        &mut self,
        coverage: &[u8],
        bpp: u8,
        w: u8,
        h: u8,
        x0: i32,
        y0: i32,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        base_opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let w = w as usize;
        let h = h as usize;
        let max_q = (1u16 << bpp) - 1;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        let mut bit_cursor = 0usize;
        for row in 0..h {
            for col in 0..w {
                let q = read_packed(coverage, bit_cursor, bpp);
                bit_cursor += bpp as usize;
                if q == 0 {
                    continue;
                }
                let cov = (q as u32 * 255 / max_q as u32) as u8;
                let mut alpha = (cov as u32 * base_opa as u32 / 255) as u8;
                if alpha == 0 {
                    continue;
                }
                let px = x0 + col as i32;
                let py = y0 + row as i32;
                if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                    if let Some(mask) = clip_mask {
                        let ca = mask[py as usize * target_w + px as usize];
                        if ca == 0 {
                            continue;
                        }
                        if ca < 255 {
                            alpha = ((alpha as u16 * ca as u16 + 127) / 255) as u8;
                            if alpha == 0 {
                                continue;
                            }
                        }
                    }
                    self.target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }
}

fn read_packed(data: &[u8], bit_pos: usize, bpp: u8) -> u16 {
    let byte_idx = bit_pos / 8;
    let bit_off = bit_pos % 8;
    let hi = *data.get(byte_idx).unwrap_or(&0) as u16;
    let lo = *data.get(byte_idx + 1).unwrap_or(&0) as u16;
    let window = (hi << 8) | lo;
    let shift = 16 - bit_off - bpp as usize;
    let mask = (1u16 << bpp) - 1;
    (window >> shift) & mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::backends::sw::SwRenderer;
    use crate::render::texture::{ColorFormat, Texture};
    use alloc::vec;

    fn pixel_alpha(buf: &[u8], stride: usize, x: usize, y: usize) -> u8 {
        buf[(y * stride + x) * 4 + 3]
    }

    #[test]
    fn read_packed_4bit_msb_first() {
        let data = [0xAB_u8, 0xCD];
        assert_eq!(read_packed(&data, 0, 4), 0xA);
        assert_eq!(read_packed(&data, 4, 4), 0xB);
        assert_eq!(read_packed(&data, 8, 4), 0xC);
        assert_eq!(read_packed(&data, 12, 4), 0xD);
    }

    #[test]
    fn read_packed_handles_unaligned_and_cross_byte() {
        let data = [0b11_01_00_10_u8, 0b01_11_00_10];
        assert_eq!(read_packed(&data, 0, 2), 0b11);
        assert_eq!(read_packed(&data, 6, 2), 0b10);
        assert_eq!(read_packed(&data, 8, 2), 0b01);
        assert_eq!(read_packed(&data, 4, 8), 0b0010_0111);
    }

    #[test]
    fn read_packed_past_end_reads_zero() {
        let data = [0xFF_u8];
        assert_eq!(read_packed(&data, 8, 4), 0);
    }

    #[test]
    fn gray_full_coverage_is_opaque_zero_is_blank() {
        let coverage = [0xF0_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_gray_glyph(&coverage, 4, 2, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255, "0xF -> opaque");
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0, "0x0 -> blank");
    }

    #[test]
    fn gray_mid_coverage_scales_to_alpha() {
        let coverage = [0x80_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_gray_glyph(&coverage, 4, 1, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        let r = buf[0];
        assert!((130..=140).contains(&r), "0x8/0xF * 255 ≈ 136, got {r}");
    }

    #[test]
    fn gray_respects_clip_bounds() {
        let coverage = [0xFF_u8, 0xFF];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_gray_glyph(&coverage, 4, 4, 1, 0, 0, (0, 0, 2, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 0, "clipped at x=2");
    }

    #[test]
    fn gray_rows_pack_without_byte_padding() {
        let coverage = [0xF0_u8, 0xF0, 0xF0];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_gray_glyph(&coverage, 4, 3, 2, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 0, 1), 0);
        assert_eq!(pixel_alpha(&buf, 4, 1, 1), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 1), 0);
    }
}
