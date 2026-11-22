use super::*;
use crate::render::backends::sw::SwRenderer;
use crate::render::font::{
    FontFaceId, FontMetrics, FontProvider, FontSurfaceId, GlyphId, GlyphSurface, RasterGlyph,
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
fn raster_inverse_keeps_precision_after_large_translation() {
    let transform = Transform {
        m00: Fixed::from_ratio(4, 5),
        m01: Fixed::from_ratio(-3, 5),
        tx: Fixed::from_ratio(2_801, 4),
        m10: Fixed::from_ratio(3, 5),
        m11: Fixed::from_ratio(4, 5),
        ty: Fixed::from_ratio(2_003, 4),
    };
    let local = Point {
        x: Fixed::from_ratio(35, 2),
        y: Fixed::from_ratio(37, 4),
    };
    let screen = transform.apply_point(local);
    let inverse = RasterInverse::new(&transform).unwrap();
    let (actual_x, actual_y) = inverse.apply(Fixed64::from(screen.x), Fixed64::from(screen.y));
    let tolerance = Fixed64::from_ratio(1, 32);

    assert!((actual_x - Fixed64::from(local.x)).abs() <= tolerance);
    assert!((actual_y - Fixed64::from(local.y)).abs() <= tolerance);
}

#[test]
fn glyph_run_uses_layout_origins_instead_of_remeasuring_text() {
    use textflow::bidi::BaseDirection;
    use textflow::shaping::Typeface;

    use crate::render::font::FontTypeface;
    use crate::text::layout::{TextLayoutCache, TextLayoutRequest};

    let font = Font::default();
    let face = FontTypeface::new(&font, font.size);
    let faces: [&dyn Typeface; 1] = [&face];
    let mut cache = TextLayoutCache::default();
    cache.begin_frame();
    let handle = cache
        .layout(
            TextLayoutRequest {
                text: "AB",
                max_width: i32::MAX,
                width: None,
                max_lines: usize::MAX,
                line_height: 8 * 256,
                baseline: 7 * 256,
                direction: BaseDirection::LeftToRight,
                wrap: textflow::layout::WrapMode::NoWrap,
                alignment: textflow::layout::Alignment::Start,
                overflow: textflow::layout::Overflow::Clip,
                spacing: textflow::layout::TextSpacing::default(),
                features: &[],
                line_widths: None,
            },
            &faces,
        )
        .unwrap();
    let mut glyphs = cache.get(handle).unwrap().glyphs().to_vec();
    glyphs[1].origin.x += 5 * 256;
    let clip = Rect::new(0, 0, 24, 8);
    let color = Color::rgba(255, 255, 255, 255);

    let mut actual = vec![0u8; 24 * 8 * 4];
    let texture =
        Texture::new(&mut actual, 24, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    SwRenderer::new(texture).draw_glyph_run_inner(&Point::ZERO, &glyphs, &font, &clip, &color, 255);

    let mut expected = vec![0u8; 24 * 8 * 4];
    let texture =
        Texture::new(&mut expected, 24, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);
    let a = [textflow::shaping::PositionedGlyph::new(
        font.map_char('A').unwrap(),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    renderer.draw_glyph_run_inner(&Point::ZERO, &a, &font, &clip, &color, 255);
    let b = [textflow::shaping::PositionedGlyph::new(
        font.map_char('B').unwrap(),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    renderer.draw_glyph_run_inner(
        &Point {
            x: Fixed::from_int(13),
            y: Fixed::ZERO,
        },
        &b,
        &font,
        &clip,
        &color,
        255,
    );

    assert_eq!(actual, expected);
}

#[test]
fn scaled_glyphs_keep_their_fractional_layout_spacing() {
    // The builtin face has an 8 px strike; drawing it at 12 px scales
    // every texel by 1.5, and the second copy sits 10.5 px along.
    let font = Font {
        size: 12,
        ..Font::default()
    };
    let glyph = font.map_char('I').unwrap();
    let glyphs = [
        textflow::shaping::PositionedGlyph::new(glyph, textflow::shaping::FlowPoint { x: 0, y: 0 }),
        textflow::shaping::PositionedGlyph::new(
            glyph,
            textflow::shaping::FlowPoint {
                x: 10 * 256 + 128,
                y: 0,
            },
        ),
    ];
    let (width, height) = (32usize, 16usize);
    let mut pixels = vec![0u8; width * height * 4];
    let texture = Texture::new(
        &mut pixels,
        width as u16,
        height as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    SwRenderer::new(texture).draw_glyph_run_inner(
        &Point::ZERO,
        &glyphs,
        &font,
        &Rect::new(0, 0, width as i32, height as i32),
        &Color::rgba(255, 255, 255, 255),
        255,
    );
    let centroid = |from: usize, to: usize| {
        let (mut sum, mut weight) = (0.0, 0.0);
        for y in 0..height {
            for x in from..to {
                let alpha = f64::from(pixel_alpha(&pixels, width, x, y));
                sum += (x as f64 + 0.5) * alpha;
                weight += alpha;
            }
        }
        assert!(weight > 0.0, "glyph painted");
        sum / weight
    };
    let spacing = centroid(9, width) - centroid(0, 9);
    assert!(
        (spacing - 10.5).abs() < 0.15,
        "copies {spacing} px apart, laid out 10.5 px apart"
    );
}

#[test]
fn affine_glyph_run_renders_inside_its_transformed_quad() {
    let font = Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        font.map_char('A').unwrap(),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let transform = Transform::translate(Fixed::from_int(12), Fixed::from_int(1))
        .compose(&Transform::rotate_deg(Fixed::from_int(90)));
    let mut buf = vec![0u8; 16 * 16 * 4];
    let texture =
        Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);

    renderer.draw_glyph_run_transformed_inner(TransformedRun {
        pos: &Point::ZERO,
        glyphs: &glyphs,
        font: &font,
        transform: &transform,
        clip: Rect::new(0, 0, 16, 16),
        color: &Color::rgba(255, 255, 255, 255),
        opacity: 255,
    });

    let painted: alloc::vec::Vec<_> = buf
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(index, pixel)| (pixel[0] != 0).then_some((index % 16, index / 16)))
        .collect();
    assert!(!painted.is_empty());
    assert!(
        painted
            .iter()
            .all(|&(x, y)| (4..12).contains(&x) && (1..9).contains(&y))
    );
}

#[test]
fn posed_glyph_run_uses_each_glyph_tangent() {
    let font = Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        font.map_char('A').unwrap(),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let frames = [textflow::placement::GlyphFrame {
        local_origin: textflow::shaping::FlowPoint {
            x: 12 << 8,
            y: 1 << 8,
        },
        unit_tangent: textflow::shaping::FlowPoint { x: 0, y: 1 << 8 },
    }];
    let mut buf = vec![0u8; 16 * 16 * 4];
    let texture =
        Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);

    renderer.draw_posed_glyph_run_inner(PosedRun {
        pos: &Point::ZERO,
        glyphs: &glyphs,
        frames: &frames,
        font: &font,
        transform: &Transform::IDENTITY,
        clip: Rect::new(0, 0, 16, 16),
        color: &Color::rgba(255, 255, 255, 255),
        opacity: 255,
    });

    let painted: alloc::vec::Vec<_> = buf
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(index, pixel)| (pixel[0] != 0).then_some((index % 16, index / 16)))
        .collect();
    assert!(!painted.is_empty());
    assert!(
        painted
            .iter()
            .all(|&(x, y)| (11..16).contains(&x) && (1..9).contains(&y))
    );
}

#[test]
fn posed_native_atlas_bitmap_uses_the_coverage_path() {
    let font = Font {
        family: "atlas-bitmap",
        size: 8,
        provider: Rc::new(AtlasBitmapProvider),
    };
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        GlyphId::new(1),
        textflow::shaping::FlowPoint { x: 0, y: 0 },
    )];
    let frames = [textflow::placement::GlyphFrame {
        local_origin: textflow::shaping::FlowPoint {
            x: 2 << 8,
            y: 2 << 8,
        },
        unit_tangent: textflow::shaping::FlowPoint { x: 1 << 8, y: 0 },
    }];
    let mut buf = vec![0u8; 8 * 8 * 4];
    let texture = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);

    renderer.draw_posed_glyph_run_inner(PosedRun {
        pos: &Point::ZERO,
        glyphs: &glyphs,
        frames: &frames,
        font: &font,
        transform: &Transform::IDENTITY,
        clip: Rect::new(0, 0, 8, 8),
        color: &Color::rgba(255, 255, 255, 255),
        opacity: 255,
    });

    assert_eq!(pixel_alpha(&buf, 8, 2, 2), 255);
    assert_eq!(pixel_alpha(&buf, 8, 1, 2), 0);
}

#[test]
fn coverage_full_value_is_opaque_and_zero_is_blank() {
    let coverage = [0xF0_u8];
    let mut buf = vec![0u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
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
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
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
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
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
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
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
fn exact_binary_coverage_matches_scalar_sampling_with_atlas_clipping_and_masks() {
    let packed = [0xff, 0xff, 0x16, 0xa0, 0x09, 0x60, 0xff, 0xff];
    let region = mirx::image::Region::new(3, 1, 9, 2).unwrap();
    let field = ScalarField::new(&packed, 2, region, 1).unwrap();
    let mut expanded = [0u8; 18];
    for y in 0..2 {
        for x in 0..9 {
            expanded[(y * 9 + x) as usize] = field.alpha(x, y);
        }
    }
    for opacity in [0, 1, 127, 254, 255] {
        for offset in [-3, 0, 2] {
            for masked in [false, true] {
                let render = |samples: &[u8], stride, region, bits| {
                    let mut pixels = vec![37; 8 * 4 * 4];
                    let texture = Texture::new(&mut pixels, 8, 4, ColorFormat::RGBA8888).unwrap();
                    let mut renderer = SwRenderer::new(texture);
                    if masked {
                        renderer.scratch.clip_stack.push(super::super::ClipMask {
                            alpha: (0..32).map(|index| (index * 7) as u8).collect(),
                        });
                    }
                    renderer.blit_coverage_region(
                        samples,
                        stride,
                        region,
                        bits,
                        offset,
                        1,
                        9,
                        2,
                        (1, 0, 7, 4),
                        &Color::rgba(21, 154, 239, 255),
                        opacity,
                    );
                    drop(renderer);
                    pixels
                };
                assert_eq!(
                    render(&packed, 2, region, 1),
                    render(
                        &expanded,
                        9,
                        mirx::image::Region::new(0, 0, 9, 2).unwrap(),
                        8
                    ),
                    "opacity={opacity}, offset={offset}, masked={masked}",
                );
            }
        }
    }
}

#[test]
fn coverage_resamples_to_the_physical_extent() {
    let coverage = [0x80_u8];
    let region = mirx::image::Region::new(0, 0, 1, 1).unwrap();
    let mut buf = vec![0u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
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

#[test]
fn coverage_scaling_interpolates_between_texels() {
    let coverage = [0_u8, 255];
    let region = mirx::image::Region::new(0, 0, 2, 1).unwrap();
    let mut buf = vec![0u8; 4 * 4];
    let tex = Texture::new(&mut buf, 4, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    backend.blit_coverage_region(
        &coverage,
        2,
        region,
        8,
        0,
        0,
        4,
        1,
        (0, 0, 4, 1),
        &Color::rgba(255, 255, 255, 255),
        255,
    );

    assert_eq!(buf[0], 0);
    assert!((60..=65).contains(&buf[4]));
    assert!((190..=192).contains(&buf[8]));
    assert_eq!(buf[12], 255);
}

struct AtlasBitmapProvider;

impl FontProvider for AtlasBitmapProvider {
    fn face_id(&self) -> FontFaceId {
        FontFaceId::new(4)
    }

    fn map_char(&self, _ch: char) -> Option<GlyphId> {
        Some(GlyphId::new(1))
    }

    fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
        Some(Fixed::ONE)
    }

    fn raster(
        &self,
        _glyph: GlyphId,
        _layout_ppem: u16,
        _output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        Some(RasterGlyph {
            surface: GlyphSurface::new(
                &[0x0f],
                2,
                1,
                1,
                mirx::image::SampleLayout::A4,
                mirx::types::ByteAlignment::ONE,
                FontSurfaceId::new(4),
            )
            .unwrap(),
            region: Some(mirx::image::Region::new(1, 0, 1, 1).unwrap()),
            representation: mirx::font::FontRepresentation::coverage(4, 8, 1).unwrap(),
            offset_x: Fixed::ZERO,
            offset_y: Fixed::ZERO,
        })
    }

    fn metrics(&self, _ppem: u16) -> FontMetrics {
        FontMetrics {
            ascender: Fixed::ZERO,
            descender: Fixed::ZERO,
            line_height: Fixed::from_int(8),
        }
    }
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

    fn raster(
        &self,
        _glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        self.glyph_size.set(output_ppem);
        assert_eq!(layout_ppem, 16);
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
fn viewport_scale_changes_only_output_representation_selection() {
    let glyph_size = Rc::new(Cell::new(0));
    let metric_size = Rc::new(Cell::new(0));
    let provider = RecordingProvider {
        glyph_size: glyph_size.clone(),
        metric_size: metric_size.clone(),
    };
    let font = Font {
        family: "recording",
        size: 16,
        provider: Rc::new(provider),
    };
    let mut buf = vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    backend.viewport = Viewport::new(64, 64, Fixed::from_int(2));

    let glyphs = [textflow::shaping::PositionedGlyph::new(
        GlyphId::new(1),
        textflow::shaping::FlowPoint { x: 0, y: 9 << 8 },
    )];
    backend.draw_glyph_run_inner(
        &Point::ZERO,
        &glyphs,
        &font,
        &Rect::new(0, 0, 32, 32),
        &Color::rgba(255, 255, 255, 255),
        255,
    );

    assert_eq!(metric_size.get(), 16);
    assert_eq!(glyph_size.get(), 32);
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

    fn raster(
        &self,
        _glyph: GlyphId,
        layout_ppem: u16,
        output_ppem: u16,
    ) -> Option<RasterGlyph<'_>> {
        assert_eq!(layout_ppem, 8);
        assert_eq!(output_ppem, 8);
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
        provider: Rc::new(SizedRasterProvider),
    };
    let mut buf = vec![0u8; 12 * 2 * 4];
    let tex = Texture::new(&mut buf, 12, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let glyphs = [
        textflow::shaping::PositionedGlyph::new(
            GlyphId::new(1),
            textflow::shaping::FlowPoint { x: 0, y: 0 },
        ),
        textflow::shaping::PositionedGlyph::new(
            GlyphId::new(1),
            textflow::shaping::FlowPoint { x: 4 << 8, y: 0 },
        ),
    ];
    backend.draw_glyph_run_inner(
        &Point::ZERO,
        &glyphs,
        &font,
        &Rect::new(0, 0, 12, 2),
        &Color::rgba(255, 255, 255, 255),
        255,
    );

    assert_eq!(pixel_alpha(&buf, 12, 0, 0), 255);
    assert_eq!(pixel_alpha(&buf, 12, 4, 0), 255);
    assert_eq!(pixel_alpha(&buf, 12, 2, 0), 0);
}
