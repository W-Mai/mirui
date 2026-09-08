use super::SwRenderer;
use crate::render::font::{Font, GlyphKind};
use crate::types::{Color, Fixed, Point, Rect, fixed::storage};

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
        let viewport_scale = self.viewport.scale();
        let mono_scale = viewport_scale.to_int().max(1);
        let phys_bounds = phys_clip.pixel_bounds();
        let (mut cx, cy) = phys_pos.floor();
        let requested_size = font.size.max(1);
        let metrics = font.metrics(requested_size);
        let char_h = metrics.line_height.to_int().max(1);
        let baseline = cy + (metrics.ascender * viewport_scale).to_int();
        for ch in text.chars() {
            let Some(g) = font.glyph(ch, requested_size) else {
                continue;
            };
            let advance;
            match &g.kind {
                GlyphKind::Mono(bitmap) => {
                    advance = g.advance.to_int() * mono_scale;
                    self.blit_mono_glyph(
                        bitmap,
                        cx,
                        cy,
                        mono_scale,
                        char_h,
                        phys_bounds,
                        color,
                        opa,
                    );
                }
                GlyphKind::Raster {
                    samples,
                    stride,
                    region,
                    representation,
                    bearing_x,
                    bearing_y,
                } => {
                    if region.width() == 0 || region.height() == 0 {
                        cx += scaled_fixed(
                            g.advance,
                            representation.design_ppem(),
                            requested_size,
                            viewport_scale,
                        )
                        .to_int();
                        continue;
                    }
                    let design = representation.design_ppem().max(1);
                    let glyph_scale = Fixed::from_int(i32::from(requested_size))
                        / Fixed::from_int(i32::from(design))
                        * viewport_scale;
                    advance = (g.advance * glyph_scale).to_int();
                    let x = cx + (*bearing_x * glyph_scale).to_int();
                    let y = baseline - (*bearing_y * glyph_scale).to_int();
                    let width = scaled_extent(region.width(), glyph_scale);
                    let height = scaled_extent(region.height(), glyph_scale);
                    match representation.kind() {
                        mirx::font::FontRepresentationKind::Coverage { bits } => self
                            .blit_coverage_region(
                                samples,
                                *stride,
                                *region,
                                bits,
                                x,
                                y,
                                width,
                                height,
                                phys_bounds,
                                color,
                                opa,
                            ),
                        mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => self
                            .blit_sdf_region(
                                samples,
                                *stride,
                                *region,
                                bits,
                                spread,
                                x,
                                y,
                                width,
                                height,
                                phys_bounds,
                                color,
                                opa,
                            ),
                        mirx::font::FontRepresentationKind::Application(_) => {}
                        _ => {}
                    }
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

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn blit_coverage_glyph(
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
        let layout = match bpp {
            1 => mirx::image::SampleLayout::A1,
            2 => mirx::image::SampleLayout::A2,
            4 => mirx::image::SampleLayout::A4,
            8 => mirx::image::SampleLayout::A8,
            _ => return,
        };
        let surface = mirx::image::SurfaceDescriptor::new(
            w.into(),
            h.into(),
            layout,
            mirx::image::ColorDescription::NONE,
        )
        .unwrap();
        let region = surface.region(0, 0, w.into(), h.into()).unwrap();
        let stride = (u32::from(w) * u32::from(bpp)).div_ceil(8);
        self.blit_coverage_region(
            coverage,
            stride,
            region,
            bpp,
            x0,
            y0,
            u16::from(w),
            u16::from(h),
            phys_bounds,
            color,
            base_opa,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_coverage_region(
        &mut self,
        coverage: &[u8],
        stride: u32,
        region: mirx::image::Region,
        bpp: u8,
        x0: i32,
        y0: i32,
        target_width: u16,
        target_height: u16,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        base_opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let source_width = region.width();
        let source_height = region.height();
        if source_width == 0 || source_height == 0 {
            return;
        }
        let target_width = u32::from(target_width.max(1));
        let target_height = u32::from(target_height.max(1));
        let max_q = (1u16 << bpp) - 1;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        for row in 0..target_height {
            let source_row = u64::from(row) * u64::from(source_height) / u64::from(target_height);
            for col in 0..target_width {
                let source_col = u64::from(col) * u64::from(source_width) / u64::from(target_width);
                let bit_cursor = (u64::from(region.y()) + source_row) * u64::from(stride) * 8
                    + (u64::from(region.x()) + source_col) * u64::from(bpp);
                let Ok(bit_cursor) = usize::try_from(bit_cursor) else {
                    continue;
                };
                let q = read_packed(coverage, bit_cursor, bpp);
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

fn scaled_fixed(value: Fixed, design_ppem: u16, requested_size: u16, viewport: Fixed) -> Fixed {
    value * Fixed::from_int(i32::from(requested_size))
        / Fixed::from_int(i32::from(design_ppem.max(1)))
        * viewport
}

fn scaled_extent(extent: u32, scale: Fixed) -> u16 {
    let raw_scale = u64::try_from(storage::to_i32(scale)).unwrap_or(0);
    let pixels = (u64::from(extent) * raw_scale).div_ceil(256);
    pixels.clamp(1, u64::from(u16::MAX)) as u16
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
    use crate::render::font::{FontBackend, FontMetrics, FontProvider, Glyph};
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::Viewport;
    use alloc::rc::Rc;
    use alloc::vec;
    use core::cell::Cell;

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
    fn coverage_full_value_is_opaque_and_zero_is_blank() {
        let coverage = [0xF0_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 2, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255, "0xF -> opaque");
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0, "0x0 -> blank");
    }

    #[test]
    fn coverage_mid_value_scales_to_alpha() {
        let coverage = [0x80_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 1, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        let r = buf[0];
        assert!((130..=140).contains(&r), "0x8/0xF * 255 ≈ 136, got {r}");
    }

    #[test]
    fn coverage_respects_clip_bounds() {
        let coverage = [0xFF_u8, 0xFF];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 4, 1, 0, 0, (0, 0, 2, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 0, "clipped at x=2");
    }

    #[test]
    fn coverage_rows_use_minimum_byte_stride() {
        let coverage = [0xF0_u8, 0xF0, 0x0F, 0x00];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 3, 2, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 0, 1), 0);
        assert_eq!(pixel_alpha(&buf, 4, 1, 1), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 1), 0);
    }

    #[test]
    fn coverage_resamples_to_the_physical_extent() {
        let coverage = [0x80_u8];
        let region = mirx::image::Region::new(0, 0, 1, 1).unwrap();
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_region(
            &coverage,
            1,
            region,
            1,
            0,
            0,
            2,
            3,
            (0, 0, 4, 4),
            &color,
            255,
        );

        for y in 0..3 {
            for x in 0..2 {
                assert_eq!(pixel_alpha(&buf, 4, x, y), 255);
            }
        }
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 0);
        assert_eq!(pixel_alpha(&buf, 4, 0, 3), 0);
    }

    struct RecordingProvider {
        glyph_size: Rc<Cell<u16>>,
        metric_size: Rc<Cell<u16>>,
    }

    impl FontProvider for RecordingProvider {
        fn glyph(&self, _ch: char, requested_size: u16) -> Option<Glyph> {
            self.glyph_size.set(requested_size);
            Some(Glyph {
                advance: Fixed::from_int(4),
                kind: GlyphKind::Raster {
                    samples: &[],
                    stride: 1,
                    region: mirx::image::Region::new(7, 9, 0, 0).unwrap(),
                    representation: mirx::font::FontRepresentation::coverage(1, 16, 0).unwrap(),
                    bearing_x: Fixed::from_ratio(-1, 2),
                    bearing_y: Fixed::from_ratio(1, 4),
                },
            })
        }

        fn metrics(&self, requested_size: u16) -> FontMetrics {
            self.metric_size.set(requested_size);
            FontMetrics {
                ascender: Fixed::from_int(12),
                descender: Fixed::from_int(-4),
                line_height: Fixed::from_int(16),
            }
        }
    }

    #[test]
    fn viewport_scale_does_not_change_representation_selection_or_draw_empty_cells() {
        let glyph_size = Rc::new(Cell::new(0));
        let metric_size = Rc::new(Cell::new(0));
        let provider = RecordingProvider {
            glyph_size: glyph_size.clone(),
            metric_size: metric_size.clone(),
        };
        let font = Font {
            family: "recording",
            size: 16,
            backend: FontBackend::Custom(Rc::new(provider)),
        };
        let mut buf = vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        backend.viewport = Viewport::new(64, 64, Fixed::from_int(2));

        backend.draw_label_inner(
            &Point::ZERO,
            " ",
            &font,
            &Rect::new(0, 0, 32, 32),
            &Color::rgba(255, 255, 255, 255),
            255,
        );

        assert_eq!(metric_size.get(), 16);
        assert_eq!(glyph_size.get(), 16);
        assert!(buf.iter().all(|byte| *byte == 0));
    }
}
