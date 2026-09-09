use super::SwRenderer;
use crate::render::font::{Font, Glyph, GlyphKind};
use crate::types::{Color, Fixed, Point, Rect, fixed::storage};

#[derive(Clone, Copy)]
struct GlyphRasterContext {
    requested_size: u16,
    viewport_scale: Fixed,
    mono_scale: i32,
    mono_height: i32,
    bounds: (i32, i32, i32, i32),
}

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
        let context = GlyphRasterContext {
            requested_size,
            viewport_scale,
            mono_scale,
            mono_height: metrics.line_height.to_int().max(1),
            bounds: phys_bounds,
        };
        let baseline = cy + (metrics.ascender * viewport_scale).to_int();
        for ch in text.chars() {
            let Some(g) = font.glyph(ch, requested_size) else {
                continue;
            };
            self.draw_glyph_at(&g, cx, cy, baseline, context, color, opa);
            cx += (g.advance * viewport_scale).to_int();
        }
    }

    pub(super) fn draw_glyph_run_inner(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        let Some(first) = glyphs.first() else {
            return;
        };
        let phys_pos = self.viewport.point_to_physical(*pos);
        let viewport_scale = self.viewport.scale();
        let requested_size = font.size.max(1);
        let metrics = font.metrics(requested_size);
        let (base_x, base_y) = phys_pos.floor();
        let base_baseline = base_y + (metrics.ascender * viewport_scale).to_int();
        let context = GlyphRasterContext {
            requested_size,
            viewport_scale,
            mono_scale: viewport_scale.to_int().max(1),
            mono_height: metrics.line_height.to_int().max(1),
            bounds: self.viewport.rect_to_physical(*clip).pixel_bounds(),
        };
        for positioned in glyphs {
            let Some(glyph) = font.glyph_by_id(positioned.glyph_id(), requested_size) else {
                continue;
            };
            let Some(dx) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let Some(dy) = positioned
                .origin
                .y
                .checked_sub(first.origin.y)
                .and_then(|value| value.checked_add(positioned.offset.y))
            else {
                continue;
            };
            let dx = crate::types::fixed::from_textflow(dx);
            let dy = crate::types::fixed::from_textflow(dy);
            let x = base_x + (dx * viewport_scale).to_int();
            let baseline = base_baseline + (dy * viewport_scale).to_int();
            let mono_y = baseline - (metrics.ascender * viewport_scale).to_int();
            self.draw_glyph_at(&glyph, x, mono_y, baseline, context, color, opa);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_at(
        &mut self,
        glyph: &Glyph<'_>,
        x: i32,
        mono_y: i32,
        baseline: i32,
        context: GlyphRasterContext,
        color: &Color,
        opa: u8,
    ) {
        match &glyph.kind {
            GlyphKind::Mono(bitmap) => self.blit_mono_glyph(
                bitmap,
                x,
                mono_y,
                context.mono_scale,
                context.mono_height,
                context.bounds,
                color,
                opa,
            ),
            GlyphKind::Raster {
                samples,
                stride,
                region,
                representation,
                bearing_x,
                bearing_y,
            } => {
                if region.width() == 0 || region.height() == 0 {
                    return;
                }
                let design = representation.design_ppem().max(1);
                let glyph_scale = Fixed::from_int(i32::from(context.requested_size))
                    / Fixed::from_int(i32::from(design))
                    * context.viewport_scale;
                let raster_x = x + (*bearing_x * context.viewport_scale).to_int();
                let raster_y = baseline - (*bearing_y * context.viewport_scale).to_int();
                let width = scaled_extent(region.width(), glyph_scale);
                let height = scaled_extent(region.height(), glyph_scale);
                match representation.kind() {
                    mirx::font::FontRepresentationKind::Coverage { bits } => self
                        .blit_coverage_region(
                            samples,
                            *stride,
                            *region,
                            bits,
                            raster_x,
                            raster_y,
                            width,
                            height,
                            context.bounds,
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
                            raster_x,
                            raster_y,
                            width,
                            height,
                            context.bounds,
                            color,
                            opa,
                        ),
                    mirx::font::FontRepresentationKind::Application(_) => {}
                    _ => {}
                }
            }
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
    use crate::render::font::{
        FontBackend, FontFaceId, FontMetrics, FontProvider, FontSurfaceId, GlyphId, GlyphSurface,
        RasterGlyph,
    };
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
    fn glyph_run_uses_layout_origins_instead_of_remeasuring_text() {
        use textflow::bidi::BaseDirection;
        use textflow::shaping::Typeface;

        use crate::render::font::FontTypeface;
        use crate::text::layout::{TextLayoutCache, TextLayoutRequest};

        let font = Font::bitmap_8x8();
        let face = FontTypeface::new(&font, font.size);
        let faces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let handle = cache
            .layout(
                TextLayoutRequest {
                    text: "AB",
                    max_width: i32::MAX,
                    max_lines: usize::MAX,
                    line_height: 8 * 256,
                    baseline: 7 * 256,
                    direction: BaseDirection::LeftToRight,
                    features: &[],
                },
                &faces,
            )
            .unwrap();
        let mut glyphs = cache.get(handle).unwrap().glyphs().to_vec();
        glyphs[1].origin.x += 5 * 256;
        let clip = Rect::new(0, 0, 24, 8);
        let color = Color::rgba(255, 255, 255, 255);

        let mut actual = vec![0u8; 24 * 8 * 4];
        let texture = Texture::new(&mut actual, 24, 8, ColorFormat::RGBA8888);
        SwRenderer::new(texture).draw_glyph_run_inner(
            &Point::ZERO,
            &glyphs,
            &font,
            &clip,
            &color,
            255,
        );

        let mut expected = vec![0u8; 24 * 8 * 4];
        let texture = Texture::new(&mut expected, 24, 8, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(texture);
        renderer.draw_label_inner(&Point::ZERO, "A", &font, &clip, &color, 255);
        renderer.draw_label_inner(
            &Point {
                x: Fixed::from_int(13),
                y: Fixed::ZERO,
            },
            "B",
            &font,
            &clip,
            &color,
            255,
        );

        assert_eq!(actual, expected);
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
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(2)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(4))
        }

        fn raster(&self, _glyph: GlyphId, requested_size: u16) -> Option<RasterGlyph<'_>> {
            self.glyph_size.set(requested_size);
            Some(RasterGlyph {
                surface: GlyphSurface::new(
                    &[],
                    0,
                    0,
                    0,
                    mirx::image::SampleLayout::A1,
                    mirx::types::ByteAlignment::ONE,
                    FontSurfaceId::new(2),
                )
                .unwrap(),
                region: None,
                representation: mirx::font::FontRepresentation::coverage(1, 16, 0).unwrap(),
                offset_x: Fixed::from_ratio(-1, 2),
                offset_y: Fixed::from_ratio(1, 4),
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

    struct SizedRasterProvider;

    impl FontProvider for SizedRasterProvider {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(3)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(4))
        }

        fn raster(&self, _glyph: GlyphId, requested_size: u16) -> Option<RasterGlyph<'_>> {
            assert_eq!(requested_size, 8);
            Some(RasterGlyph {
                surface: GlyphSurface::new(
                    &[0x80],
                    1,
                    1,
                    1,
                    mirx::image::SampleLayout::A1,
                    mirx::types::ByteAlignment::ONE,
                    FontSurfaceId::new(3),
                )
                .unwrap(),
                region: Some(mirx::image::Region::new(0, 0, 1, 1).unwrap()),
                representation: mirx::font::FontRepresentation::coverage(1, 16, 0).unwrap(),
                offset_x: Fixed::ZERO,
                offset_y: Fixed::ZERO,
            })
        }

        fn metrics(&self, requested_size: u16) -> FontMetrics {
            assert_eq!(requested_size, 8);
            FontMetrics {
                ascender: Fixed::ZERO,
                descender: Fixed::ZERO,
                line_height: Fixed::from_int(8),
            }
        }
    }

    #[test]
    fn target_size_advance_is_not_scaled_by_the_raster_representation() {
        let font = Font {
            family: "sized-raster",
            size: 8,
            backend: FontBackend::Custom(Rc::new(SizedRasterProvider)),
        };
        let mut buf = vec![0u8; 12 * 2 * 4];
        let tex = Texture::new(&mut buf, 12, 2, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        backend.draw_label_inner(
            &Point::ZERO,
            "AA",
            &font,
            &Rect::new(0, 0, 12, 2),
            &Color::rgba(255, 255, 255, 255),
            255,
        );

        assert_eq!(pixel_alpha(&buf, 12, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 12, 4, 0), 255);
        assert_eq!(pixel_alpha(&buf, 12, 2, 0), 0);
    }
}
