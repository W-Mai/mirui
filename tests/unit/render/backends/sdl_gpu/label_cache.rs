use super::*;
use crate::render::font::FontSurfaceId;

#[test]
fn scalar_admission_checks_actual_representation_and_surface_budget() {
    let font = Font::default();
    let mut raster = font
        .raster_for_output(font.map_char('A').unwrap(), 8, 8)
        .unwrap();
    assert!(scalar_admitted(raster, (8, 760)));
    assert!(!scalar_admitted(raster, (7, 760)));
    assert!(!scalar_admitted(raster, (8, 759)));
    let bytes = alloc::vec![0; 1024 * 1025];
    raster.surface = GlyphSurface::new(
        &bytes,
        1024,
        1025,
        1024,
        mirx::image::SampleLayout::A8,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(99),
    )
    .unwrap();
    assert!(!scalar_admitted(raster, (4096, 4096)));
    let sdf = Font::from_mirx(
        "Play",
        24,
        include_bytes!("../../../../../src/gallery/demos/assets/play_ui.mirx"),
        &mirx::reader::PayloadLimits::HOST,
    )
    .unwrap();
    assert!(!scalar_admitted(
        sdf.raster_for_output(sdf.map_char('A').unwrap(), 24, 48)
            .unwrap(),
        (4096, 4096)
    ));
}

#[test]
fn glyph_opacity_preserves_run_rounding_and_low_alpha() {
    for (alpha, opacity, expected) in [
        (0, 255, 0),
        (1, 128, 0),
        (127, 128, 63),
        (128, 255, 128),
        (255, 255, 255),
    ] {
        assert_eq!(glyph_opacity(alpha, opacity), expected);
    }
}

#[test]
fn cached_runs_preserve_coverage_and_filterable_rgb() {
    let mut pixels = Vec::new();
    for size in [14, 48] {
        let font = Font::from_mirx(
            "UI",
            size,
            include_bytes!("../../../../../src/gallery/demos/assets/misans_ui.mirx"),
            &mirx::reader::PayloadLimits::HOST,
        )
        .unwrap();
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            font.map_char('A').unwrap(),
            textflow::shaping::FlowPoint { x: 0, y: 0 },
        )];
        let scale = Fixed::from_int(2);
        let bounds = font.raster_run_bounds(&glyphs, size * 2, scale).unwrap();
        let origin = Point {
            x: -bounds.offset.x,
            y: -bounds.offset.y,
        };
        let color = Color::rgba(146, 204, 255, 128);
        rasterize_run_pixels(
            RasterContent::Positioned {
                glyphs: &glyphs,
                origin,
            },
            &font,
            color,
            bounds,
            scale,
            &mut pixels,
        )
        .unwrap();
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
        for pixel in pixels.chunks_exact(4) {
            assert_eq!(&pixel[..3], &[color.r, color.g, color.b]);
        }
    }
}

#[test]
fn scalar_sampling_preserves_texel_scale_and_clamps_each_region() {
    let surface = GlyphSurface::new(
        &[0; 64],
        8,
        8,
        8,
        mirx::image::SampleLayout::A8,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(1),
    )
    .unwrap();
    for width in [1, 2, 4] {
        let quad = crate::render::font::RasterQuad {
            rect: Rect::new(0, 0, width, width),
            transform: Transform::IDENTITY,
        };
        let region = mirx::image::Region::new(2, 2, width as u32, width as u32).unwrap();
        let (vertices, _) = LabelCache::scalar_glyph_geometry(
            0,
            quad,
            region,
            surface,
            Color::rgb(255, 255, 255),
            255,
            Viewport::new(20, 20, Fixed::ONE),
        )
        .unwrap();
        for step in 0..=32 {
            let position = width as f32 * step as f32 / 32.0;
            let expected = (2.0 + position.clamp(0.5, width as f32 - 0.5)) / 8.0;
            for vertical in [false, true] {
                let axis = |index: usize| {
                    if vertical {
                        (
                            vertices[index * 4].position.y,
                            vertices[index * 4].tex_coord.y,
                        )
                    } else {
                        (vertices[index].position.x, vertices[index].tex_coord.x)
                    }
                };
                let (left, right) = (0..3)
                    .map(|index| (axis(index), axis(index + 1)))
                    .find(|(left, right)| {
                        right.0 > left.0 && position >= left.0 && position <= right.0
                    })
                    .unwrap();
                let actual = left.1 + (right.1 - left.1) * (position - left.0) / (right.0 - left.0);
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "width={width}, position={position}"
                );
            }
        }
    }
}

#[test]
fn empty_clips_do_not_become_unrestricted_sdl_clips() {
    for rect in [
        Rect::new(1, 2, 0, 3),
        Rect::new(1, 2, 3, 0),
        Rect::new(1, 2, -1, 3),
    ] {
        assert!(sdl_pixel_rect(&rect, &rect).is_none());
    }
    assert_eq!(
        sdl_pixel_rect(&Rect::new(-1, 2, 3, 4), &Rect::new(-1, 2, 3, 4)).unwrap(),
        sdl2::rect::Rect::new(-1, 2, 3, 4)
    );
}

#[test]
fn scalar_geometry_keeps_pose_region_tint_and_display_scale() {
    let surface = GlyphSurface::new(
        &[0; 64],
        8,
        8,
        8,
        mirx::image::SampleLayout::A8,
        mirx::types::ByteAlignment::ONE,
        FontSurfaceId::new(3),
    )
    .unwrap();
    let quad = crate::render::font::RasterQuad {
        rect: Rect::new(1, 2, 4, 3),
        transform: Transform::translate(Fixed::from_int(5), Fixed::from_int(7)),
    };
    let region = mirx::image::Region::new(2, 1, 4, 3).unwrap();

    let (vertices, indices) = LabelCache::scalar_glyph_geometry(
        4,
        quad,
        region,
        surface,
        Color::rgba(10, 20, 30, 128),
        128,
        Viewport::new(100, 100, Fixed::from_int(2)),
    )
    .unwrap();

    assert_eq!(&indices[..6], &[4, 5, 9, 4, 9, 8]);
    assert!(indices.into_iter().all(|index| (4..20).contains(&index)));
    assert_eq!(
        (vertices[0].position.x, vertices[0].position.y),
        (12.0, 18.0)
    );
    assert_eq!(
        (vertices[15].position.x, vertices[15].position.y),
        (20.0, 24.0)
    );
    assert_eq!(
        (vertices[0].tex_coord.x, vertices[0].tex_coord.y),
        (0.3125, 0.1875)
    );
    assert_eq!(
        (vertices[15].tex_coord.x, vertices[15].tex_coord.y),
        (0.6875, 0.4375)
    );
    assert_eq!(
        (
            vertices[0].color.r,
            vertices[0].color.g,
            vertices[0].color.b,
            vertices[0].color.a,
        ),
        (10, 20, 30, 64)
    );
}
