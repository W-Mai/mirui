use super::blit_fast::{blit_1to1_fast, blit_2to2_fast, blit_dda, blit_generic_slow};
use super::*;
use crate::render::RenderResource;
use crate::render::texture::ColorFormat;
use alloc::vec;

#[test]
fn unreadable_blits_preserve_the_target_and_fail_the_frame() {
    let source_bytes = [255u8; 16];
    let mut source = Texture::from_ref(&source_bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
    source.stride = 16;
    let clip = Rect::new(0, 0, 8, 8);
    for transform in [
        Transform::IDENTITY,
        Transform::rotate_deg(Fixed::from_int(15)),
    ] {
        for quad in [
            None,
            Some([
                Point::new(0, 0),
                Point::new(8, 0),
                Point::new(8, 8),
                Point::new(0, 8),
            ]),
        ] {
            let mut renderer =
                SwRenderer::new(Texture::owned(8, 8, ColorFormat::RGBA8888).unwrap());
            let command = DrawCommand::Blit {
                pos: Point::ZERO,
                size: Point {
                    x: Fixed::from_int(4),
                    y: Fixed::from_int(4),
                },
                transform,
                quad,
                texture: &source,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            };
            renderer.draw(&command, &clip);
            assert_eq!(
                Canvas::flush(&mut renderer),
                Err(RenderError::InvalidTexture)
            );
            assert_eq!(
                renderer.draw_request(&DrawRequest::new(&command, clip)),
                Err(RenderError::InvalidTexture)
            );
            assert!(
                renderer
                    .target
                    .read_pixels()
                    .unwrap()
                    .bytes()
                    .iter()
                    .all(|byte| *byte == 0)
            );
            assert_eq!(
                Canvas::flush(&mut renderer),
                Err(RenderError::InvalidTexture)
            );
        }
    }
}

#[test]
fn direct_canvas_blit_reports_invalid_source_on_flush() {
    let bytes = [0u8; 4];
    let mut source = Texture::from_ref(&bytes, 1, 1, ColorFormat::RGBA8888).unwrap();
    source.width = 2;
    let mut renderer = SwRenderer::new(Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap());
    renderer.blit(
        &source,
        &Rect::new(0, 0, 2, 1),
        Point::ZERO,
        Point {
            x: Fixed::from_int(2),
            y: Fixed::ONE,
        },
        &Rect::new(0, 0, 4, 4),
        255,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );
    assert_eq!(
        Canvas::flush(&mut renderer),
        Err(RenderError::InvalidTexture)
    );
    assert!(
        renderer
            .target
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn route_rejects_software_draws_that_drop_requested_semantics() {
    let mut renderer = SwRenderer::new(
        Texture::owned(16, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let clip = Rect::new(0, 0, 16, 16);
    let affine = Transform::rotate_deg(Fixed::from_int(20));
    let texture = Texture::owned(2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    let blit = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: affine,
        quad: None,
        texture: &texture,
        opa: 128,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    assert_eq!(
        renderer.route(&DrawRequest::new(&blit, clip)),
        Err(RenderError::Unsupported(RenderFeature::BlitOpacity))
    );
    assert_eq!(
        renderer.submit_raw(&DrawRequest::new(&blit, clip)),
        Err(RenderError::Unsupported(RenderFeature::BlitOpacity))
    );
    assert!(
        renderer
            .target
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );

    let blur = DrawCommand::ApplyBlur {
        alpha: Fixed::from_ratio(1, 2),
        region: clip,
    };
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(18), Fixed::from_int(400));
    assert_eq!(
        renderer.route(&DrawRequest::new(&blur, clip).with_projective(projective)),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&blur, clip).with_projective(projective)),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );
}

#[test]
fn submit_draws_plain_commands() {
    let mut renderer = SwRenderer::new(
        Texture::owned(16, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let clip = Rect::new(0, 0, 16, 16);
    let fill = DrawCommand::Fill {
        area: Rect::new(2, 3, 4, 5),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(20, 30, 40),
        radius: Fixed::ZERO,
        opa: 255,
    };
    renderer
        .draw_request(&DrawRequest::new(&fill, clip))
        .unwrap();
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(3, 4),
        Color::rgb(20, 30, 40)
    );
}

#[test]
fn affine_rounded_fills_preserve_quad_semantics_in_region_scopes() {
    use crate::render::Renderer;

    let clip = Rect::new(4, 4, 24, 24);
    let area = Rect::new(0, 0, 12, 8);
    for scale in [Fixed::ONE, Fixed::from_int(2)] {
        for transform in [
            Transform::translate(Fixed::from_int(8), Fixed::from_int(8)).compose(
                &Transform::scale(Fixed::from_ratio(3, 2), Fixed::from_ratio(5, 4)),
            ),
            Transform::translate(Fixed::from_int(12), Fixed::from_int(8))
                .compose(&Transform::rotate_deg(Fixed::from_int(25))),
        ] {
            for opacity in [128, 255] {
                let command = DrawCommand::Fill {
                    area,
                    transform,
                    quad: None,
                    color: Color::rgba(200, 80, 40, 180),
                    radius: Fixed::from_int(3),
                    opa: opacity,
                };
                let mut expected =
                    SwRenderer::new(Texture::owned(64, 64, ColorFormat::RGBA8888).unwrap());
                expected.viewport = Viewport::new(64, 64, scale);
                let mut explicit = command;
                if let DrawCommand::Fill {
                    transform: tf,
                    quad,
                    ..
                } = &mut explicit
                {
                    *tf = Transform::IDENTITY;
                    *quad = Some(transform.apply_rect(area));
                }
                expected
                    .draw_request(&DrawRequest::new(&explicit, clip))
                    .unwrap();
                assert!(
                    expected
                        .target
                        .read_pixels()
                        .unwrap()
                        .bytes()
                        .iter()
                        .any(|b| *b != 0)
                );
                for scoped in [false, true] {
                    let mut actual =
                        SwRenderer::new(Texture::owned(64, 64, ColorFormat::RGBA8888).unwrap());
                    actual.viewport = expected.viewport;
                    assert_eq!(
                        actual.route(&DrawRequest::new(&command, clip)),
                        Ok(RenderRoute::Native)
                    );
                    if scoped {
                        let region = actual.plan_scope(&clip).unwrap();
                        actual
                            .render_scope(region, &mut |local| {
                                local.draw_request(&DrawRequest::new(&command, clip))
                            })
                            .unwrap();
                    } else {
                        let allocations = crate::test_support::tracked_allocations(|| {
                            actual
                                .draw_request(&DrawRequest::new(&command, clip))
                                .unwrap();
                        });
                        assert_eq!(allocations, 0);
                    }
                    assert_eq!(
                        actual.target.read_pixels().unwrap().bytes(),
                        expected.target.read_pixels().unwrap().bytes()
                    );
                }
            }
        }
    }
}

/// Blit dst origin at a negative x — common when an OffscreenRender
/// entity's WidgetTransform translates the buffer off the left
/// edge — must not write to wrong target rows. Pre-fix, blit_inner
/// passed `dx0=-N` straight to the path that did `dx0 as usize * 4`
/// for the row offset, wrapping into a far positive byte index and
/// scribbling the source texture into a different row of the
/// target.
#[test]
fn blit_at_negative_x_does_not_wrap_into_wrong_rows() {
    let mut tgt_buf = vec![0u8; 64 * 64 * 4];
    let tgt_tex =
        Texture::new(&mut tgt_buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tgt_tex);

    let mut src_buf = vec![0u8; 32 * 32 * 4];
    for px in src_buf.chunks_exact_mut(4) {
        px[0] = 200;
        px[1] = 100;
        px[2] = 50;
        px[3] = 255;
    }
    let src_tex =
        Texture::new(&mut src_buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let src_rect = Rect::new(0, 0, 32, 32);

    let dst = Point::new(Fixed::from_int(-24), Fixed::from_int(8));
    let dst_size = Point::new(Fixed::from_int(32), Fixed::from_int(32));
    let clip = Rect {
        x: Fixed::from_int(-24),
        y: Fixed::from_int(0),
        w: Fixed::from_int(80),
        h: Fixed::from_int(64),
    };
    backend.blit(
        &src_tex,
        &src_rect,
        dst,
        dst_size,
        &clip,
        255,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );

    for y in 8..40 {
        for x in 0..8 {
            let p = backend.target.read_pixels().unwrap().get_pixel(x, y);
            assert_eq!(
                (p.r, p.g, p.b),
                (200, 100, 50),
                "visible pixel ({},{}) should be source colour",
                x,
                y
            );
        }
    }
    // Anywhere outside the visible band must stay zeroed; the
    // pre-fix bug wrote source colour into row 8 columns ~40-64
    // because dx0=-24 cast to usize wrapped the row offset.
    for y in 0..64 {
        for x in 0..64 {
            let in_visible = (8..40).contains(&y) && x < 8;
            if in_visible {
                continue;
            }
            let p = backend.target.read_pixels().unwrap().get_pixel(x, y);
            assert_eq!(
                (p.r, p.g, p.b, p.a),
                (0, 0, 0, 0),
                "out-of-visible pixel ({},{}) was written",
                x,
                y
            );
        }
    }
}

/// Same negative-x guard as the RGBA8888 case but exercising the
/// RGB565Swapped → RGB565Swapped fast path, which has its own
/// `dx0 as usize * 2` byte-offset arithmetic.
#[test]
fn blit_at_negative_x_rgb565_swapped_does_not_wrap() {
    let mut tgt_buf = vec![0u8; 64 * 64 * 2];
    let tgt_tex = Texture::new(&mut tgt_buf, 64, 64, ColorFormat::RGB565Swapped)
        .expect("valid pixel storage");
    let mut backend = SwRenderer::new(tgt_tex);

    let mut src_buf = vec![0u8; 32 * 32 * 2];
    for px in src_buf.chunks_exact_mut(2) {
        // RGB565Swapped: hi byte first. Pack red = 0xF800,
        // wire bytes [0xF8, 0x00].
        px[0] = 0xF8;
        px[1] = 0x00;
    }
    let src_tex = Texture::new(&mut src_buf, 32, 32, ColorFormat::RGB565Swapped)
        .expect("valid pixel storage");
    let src_rect = Rect::new(0, 0, 32, 32);

    let dst = Point::new(Fixed::from_int(-24), Fixed::from_int(8));
    let dst_size = Point::new(Fixed::from_int(32), Fixed::from_int(32));
    let clip = Rect {
        x: Fixed::from_int(-24),
        y: Fixed::from_int(0),
        w: Fixed::from_int(80),
        h: Fixed::from_int(64),
    };
    backend.blit(
        &src_tex,
        &src_rect,
        dst,
        dst_size,
        &clip,
        255,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );

    for y in 8..40 {
        for x in 0..8 {
            let p = backend.target.read_pixels().unwrap().get_pixel(x, y);
            assert_eq!(p.r, 255, "visible pixel ({},{}) should be red", x, y);
        }
    }
    for y in 0..64 {
        for x in 0..64 {
            let in_visible = (8..40).contains(&y) && x < 8;
            if in_visible {
                continue;
            }
            let off = (y as usize * 64 + x as usize) * 2;
            assert_eq!(
                (
                    backend.target.read_pixels().unwrap().bytes()[off],
                    backend.target.read_pixels().unwrap().bytes()[off + 1]
                ),
                (0, 0),
                "out-of-visible pixel ({},{}) was written",
                x,
                y
            );
        }
    }
}

#[test]
fn fill_rect_basic() {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let rect = Rect::new(2, 2, 4, 4);
    let clip = Rect::new(0, 0, 16, 16);
    backend.fill_rect(&rect, &clip, &Color::rgb(255, 0, 0), Fixed::ZERO, 255);

    let c = backend.target.read_pixels().unwrap().get_pixel(3, 3);
    assert_eq!(c.r, 255);
    assert_eq!(c.g, 0);
    assert_eq!(c.b, 0);

    let c = backend.target.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(c.r, 0);
}

#[test]
fn clear_fills_area() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    backend.clear(
        &crate::types::PhysicalRect::from_size(8, 8),
        &Color::rgb(50, 100, 150),
    );

    let c = backend.target.read_pixels().unwrap().get_pixel(4, 4);
    assert_eq!(c.r, 50);
    assert_eq!(c.g, 100);
    assert_eq!(c.b, 150);
}

#[test]
fn clear_preserves_padding_and_matches_pixel_replacement_in_every_format() {
    use crate::types::PhysicalRect;
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        for color in [Color::rgba(0, 0, 0, 0), Color::rgba(193, 79, 37, 128)] {
            for area in [
                PhysicalRect::EMPTY,
                PhysicalRect::from_size(4, 3),
                PhysicalRect::new(1, 1, 9, 9).unwrap(),
                PhysicalRect::new(3, 2, 1, 1).unwrap(),
                PhysicalRect::new(4, 3, 2, 2).unwrap(),
            ] {
                let stride = 4 * format.bytes_per_pixel() + 5;
                let mut actual = [0xa5; 96];
                let mut expected = actual;
                let texture = Texture::new(&mut actual, 4, 3, format)
                    .unwrap()
                    .with_stride(stride)
                    .unwrap();
                let mut renderer = SwRenderer::new(texture);
                renderer.clear(&area, &color);
                assert_eq!(renderer.flush(), Ok(()));
                drop(renderer);
                let mut reference = Texture::new(&mut expected, 4, 3, format)
                    .unwrap()
                    .with_stride(stride)
                    .unwrap();
                if let Some(clipped) = area.intersection(PhysicalRect::from_size(4, 3)) {
                    let mut pixels = reference.pixels_mut().unwrap();
                    for y in clipped.y()..clipped.bottom() {
                        for x in clipped.x()..clipped.right() {
                            pixels.set_pixel(i32::from(x), i32::from(y), &color);
                        }
                    }
                }
                drop(reference);
                assert_eq!(actual, expected, "{format:?} {area:?} {color:?}");
            }
        }
    }
}

#[test]
fn clear_replaces_physical_pixels_without_scaling_or_blending() {
    use crate::types::PhysicalRect;
    let mut bytes = [0_u8; 4 * 3 * 4];
    let texture = Texture::new(&mut bytes, 4, 3, ColorFormat::RGBA8888).unwrap();
    let mut renderer = SwRenderer::new(texture);
    renderer.viewport = crate::types::Viewport::new(4, 3, Fixed::from_f32(1.5));
    let background = Color::rgb(13, 29, 47);
    renderer.clear(&PhysicalRect::from_size(4, 3), &background);
    let partial = Color::rgba(200, 100, 50, 128);
    let area = PhysicalRect::new(1, 1, 9, 9).unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..4 {
            renderer.clear(&area, &partial);
            renderer.clear(
                &PhysicalRect::new(2, 1, 1, 1).unwrap(),
                &Color::rgba(0, 0, 0, 0),
            );
            renderer.clear(&PhysicalRect::EMPTY, &background);
        }
    });
    assert_eq!(allocations, 0);
    let pixels = renderer.target.read_pixels().unwrap();
    for y in 0..3 {
        for x in 0..4 {
            let expected = if x == 2 && y == 1 {
                Color::rgba(0, 0, 0, 0)
            } else if x >= 1 && y >= 1 {
                partial
            } else {
                background
            };
            assert_eq!(pixels.get_pixel(x, y), expected);
        }
    }
}

#[test]
fn fill_path_rect_matches_fill_rect() {
    // A rectangular Path should produce the same interior pixels as fill_rect.
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let path = crate::render::path::Path::rect(
        Fixed::from_int(2),
        Fixed::from_int(2),
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    let clip = Rect::new(0, 0, 16, 16);
    let paint = Paint::Color(Color::rgb(0, 0, 255).into());
    backend.fill_path(
        &path,
        &clip,
        &paint,
        255,
        crate::render::raster::FillRule::EvenOdd,
    );

    let c = backend.target.read_pixels().unwrap().get_pixel(5, 5);
    assert_eq!(c.b, 255);
    assert_eq!(c.r, 0);
    let c = backend.target.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(c.b, 0);
}

#[test]
fn fill_path_empty_is_noop() {
    let mut buf = vec![0u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let path = crate::render::path::Path::new();
    let clip = Rect::new(0, 0, 4, 4);
    let paint = Paint::Color(Color::rgb(255, 255, 255).into());
    backend.fill_path(
        &path,
        &clip,
        &paint,
        255,
        crate::render::raster::FillRule::EvenOdd,
    );

    for y in 0..4 {
        for x in 0..4 {
            assert_eq!(backend.target.read_pixels().unwrap().get_pixel(x, y).r, 0);
        }
    }
}

#[test]
fn fill_path_zero_opa_is_noop() {
    let mut buf = vec![0u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let path = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let clip = Rect::new(0, 0, 4, 4);
    let paint = Paint::Color(Color::rgb(255, 0, 0).into());
    backend.fill_path(
        &path,
        &clip,
        &paint,
        0,
        crate::render::raster::FillRule::EvenOdd,
    );

    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(2, 2).r, 0);
}

#[test]
fn fill_path_triangle_interior_vs_exterior() {
    // Right triangle with vertices (0,0), (10,0), (0,10). Probe interior
    // point (2,2) vs exterior point (8,8).
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let mut path = crate::render::path::Path::new();
    path.move_to(Point {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    })
    .line_to(Point {
        x: Fixed::from_int(10),
        y: Fixed::ZERO,
    })
    .line_to(Point {
        x: Fixed::ZERO,
        y: Fixed::from_int(10),
    })
    .close();

    let clip = Rect::new(0, 0, 16, 16);
    let paint = Paint::Color(Color::rgb(0, 200, 0).into());
    backend.fill_path(
        &path,
        &clip,
        &paint,
        255,
        crate::render::raster::FillRule::EvenOdd,
    );

    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(2, 2).g, 200);
    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(8, 8).g, 0);
}

#[test]
fn draw_glyph_run_is_reachable_via_trait() {
    let mut buf = vec![0u8; 32 * 16 * 4];
    let tex = Texture::new(&mut buf, 32, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let pos = Point {
        x: Fixed::from_int(1),
        y: Fixed::from_int(1),
    };
    let clip = Rect::new(0, 0, 32, 16);
    let font = crate::render::font::Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    Canvas::draw_glyph_run(
        &mut backend,
        &pos,
        &glyphs,
        &font,
        &clip,
        &Color::rgb(255, 0, 0),
        255,
    );

    let mut found = false;
    for y in 0..16 {
        for x in 0..32 {
            if backend.target.read_pixels().unwrap().get_pixel(x, y).r > 0 {
                found = true;
                break;
            }
        }
    }
    assert!(found, "expected at least one red pixel from glyph");
}

#[test]
fn renderer_routes_affine_glyph_runs() {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let font = crate::render::font::Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(u16::from(b'A')),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let transform = Transform::translate(Fixed::from_int(12), Fixed::from_int(1))
        .compose(&Transform::rotate_deg(Fixed::from_int(90)));

    renderer.draw(
        &DrawCommand::GlyphRun {
            pos: Point::ZERO,
            transform,
            glyphs: &glyphs,
            font: &font,
            color: Color::rgb(255, 0, 0),
            opa: 255,
        },
        &Rect::new(0, 0, 16, 16),
    );

    assert!(buf.chunks_exact(4).any(|pixel| pixel[0] != 0));
}

#[test]
fn stroke_path_line_colors_interior_and_skips_far_pixels() {
    // Horizontal line from (2,8) to (14,8), width=2. Interior pixels
    // around y=8 should be colored; pixels several rows away must not.
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let mut path = crate::render::path::Path::new();
    path.move_to(Point {
        x: Fixed::from_int(2),
        y: Fixed::from_int(8),
    })
    .line_to(Point {
        x: Fixed::from_int(14),
        y: Fixed::from_int(8),
    });

    let clip = Rect::new(0, 0, 16, 16);
    let paint = Paint::Color(Color::rgb(255, 0, 0).into());
    backend.stroke_path(
        &path,
        &clip,
        Fixed::from_int(2),
        &paint,
        255,
        crate::render::raster::LineCap::Butt,
        crate::render::raster::LineJoin::Miter,
        Fixed::from_int(4),
        &[],
    );

    assert!(backend.target.read_pixels().unwrap().get_pixel(8, 8).r > 0);
    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(8, 0).r, 0);
    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(8, 15).r, 0);
}

#[test]
fn renderer_dispatches_line_command() {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let cmd = DrawCommand::Line {
        p1: Point {
            x: Fixed::from_int(2),
            y: Fixed::from_int(8),
        },
        p2: Point {
            x: Fixed::from_int(14),
            y: Fixed::from_int(8),
        },
        transform: crate::types::Transform::IDENTITY,
        color: Color::rgb(255, 0, 0),
        width: Fixed::from_int(2),
        opa: 255,
    };
    let clip = Rect::new(0, 0, 16, 16);
    backend.draw_request(&DrawRequest::new(&cmd, clip)).unwrap();

    assert!(backend.target.read_pixels().unwrap().get_pixel(8, 8).r > 0);
}

#[test]
fn renderer_dispatches_arc_command() {
    let mut buf = vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let cmd = DrawCommand::Arc {
        center: Point {
            x: Fixed::from_int(16),
            y: Fixed::from_int(16),
        },
        transform: crate::types::Transform::IDENTITY,
        radius: Fixed::from_int(10),
        start_angle: Fixed::from_int(0),
        end_angle: Fixed::from_int(90),
        color: Color::rgb(0, 255, 0),
        width: Fixed::from_int(2),
        opa: 255,
    };
    let clip = Rect::new(0, 0, 32, 32);
    backend.draw_request(&DrawRequest::new(&cmd, clip)).unwrap();

    let hit = backend.target.read_pixels().unwrap().get_pixel(26, 16).g > 0
        || backend.target.read_pixels().unwrap().get_pixel(25, 16).g > 0;
    assert!(hit);
}

#[test]
fn draw_line_matches_two_command_path() {
    let cases = [
        (Point::new(2, 8), Point::new(14, 8), Rect::new(0, 0, 24, 24)),
        (
            Point::new(2, 3),
            Point::new(18, 19),
            Rect::new(0, 0, 24, 24),
        ),
        (
            Point::new(0, 12),
            Point::new(23, 12),
            Rect::new(4, 4, 16, 16),
        ),
    ];
    for (p1, p2, clip) in cases {
        let mut line = SwRenderer::new(
            Texture::owned(24, 24, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        let mut path_renderer = SwRenderer::new(
            Texture::owned(24, 24, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        let color = Color::rgb(255, 0, 0);
        let width = Fixed::from_int(2);
        line.draw_line(p1, p2, &clip, width, &color, 180);

        let mut path = Path::new();
        path.move_to(p1).line_to(p2);
        assert_eq!(path.commands().len(), 2);
        path_renderer.stroke_path(
            &path,
            &clip,
            width,
            &Paint::Color(color.into()),
            180,
            crate::render::raster::LineCap::Butt,
            crate::render::raster::LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );
        assert_eq!(
            line.target.read_pixels().unwrap().bytes(),
            path_renderer.target.read_pixels().unwrap().bytes()
        );
    }
}

#[test]
fn repeated_draw_line_keeps_stroke_scratch_capacity() {
    let mut renderer = SwRenderer::new(
        Texture::owned(24, 24, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let clip = Rect::new(0, 0, 24, 24);
    let color = Color::rgb(255, 0, 0);
    let draw = |renderer: &mut SwRenderer<'_>| {
        renderer.draw_line(
            Point::new(2, 8),
            Point::new(20, 8),
            &clip,
            Fixed::from_int(2),
            &color,
            255,
        );
    };
    draw(&mut renderer);
    let capacities = (
        renderer.scratch.flatten_buf.capacity(),
        renderer.scratch.stroke_outline.command_capacity(),
        renderer.scratch.stroke_normals.capacity(),
        renderer.scratch.scanline_crossings.capacity(),
    );
    for _ in 0..8 {
        renderer.target.pixels_mut().unwrap().bytes_mut().fill(0);
        draw(&mut renderer);
        assert_eq!(
            (
                renderer.scratch.flatten_buf.capacity(),
                renderer.scratch.stroke_outline.command_capacity(),
                renderer.scratch.stroke_normals.capacity(),
                renderer.scratch.scanline_crossings.capacity(),
            ),
            capacities,
        );
    }
}

#[test]
fn draw_arc_strokes_pixels() {
    let mut buf = vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let center = Point {
        x: Fixed::from_int(16),
        y: Fixed::from_int(16),
    };
    let clip = Rect::new(0, 0, 32, 32);
    backend.draw_arc(
        center,
        Fixed::from_int(10),
        Fixed::from_int(0),
        Fixed::from_int(90),
        &clip,
        Fixed::from_int(2),
        &Color::rgb(0, 255, 0),
        255,
    );

    // The 0°→90° arc runs from (+radius, 0) to (0, +radius) relative to
    // center. Sample a point on the arc and verify green is present.
    assert!(
        backend.target.read_pixels().unwrap().get_pixel(26, 16).g > 0
            || backend.target.read_pixels().unwrap().get_pixel(25, 16).g > 0
    );
}

#[test]
fn draw_arc_matches_explicit_path_across_sweeps_clips_and_scale() {
    let center = Point::new(16, 14);
    let radius = Fixed::from_int(10);
    let color = Color::rgb(18, 211, 93);
    let cases = [
        (0, 90, Rect::new(0, 0, 64, 64), 1, 2, 255),
        (45, 290, Rect::new(5, 3, 25, 19), 1, 3, 167),
        (270, -90, Rect::new(1, 2, 28, 28), 2, 1, 255),
        (0, 0, Rect::new(0, 0, 64, 64), 1, 2, 255),
    ];

    for (start, end, clip, scale, width, opacity) in cases {
        let start = Fixed::from_int(start);
        let end = Fixed::from_int(end);
        let width = Fixed::from_int(width);
        let mut actual = SwRenderer::new(
            Texture::owned(64, 64, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        let mut expected = SwRenderer::new(
            Texture::owned(64, 64, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        let viewport = Viewport::new(64, 64, Fixed::from_int(scale));
        actual.viewport = viewport;
        expected.viewport = viewport;

        actual.draw_arc(center, radius, start, end, &clip, width, &color, opacity);
        let path = Path::arc(center, radius, start, end);
        expected.stroke_path(
            &path,
            &clip,
            width,
            &Paint::Color(color.into()),
            opacity,
            crate::render::raster::LineCap::Butt,
            crate::render::raster::LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );

        assert_eq!(
            actual.target.read_pixels().unwrap().bytes(),
            expected.target.read_pixels().unwrap().bytes()
        );
    }
}

#[test]
fn affine_arc_matches_explicit_path_and_retains_path_storage() {
    let mut actual = SwRenderer::new(
        Texture::owned(64, 64, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let mut expected = SwRenderer::new(
        Texture::owned(64, 64, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let center = Point::new(24, 16);
    let radius = Fixed::from_int(12);
    let start = Fixed::from_int(30);
    let end = Fixed::from_int(250);
    let color = Color::rgb(220, 100, 30);
    let width = Fixed::from_int(2);
    let opacity = 192;
    let clip = Rect::new(0, 0, 64, 64);
    let transform = Transform::rotate_deg(Fixed::from_int(12));
    let physical_transform = actual.viewport.as_transform().compose(&transform);
    let command = DrawCommand::Arc {
        center,
        transform,
        radius,
        start_angle: start,
        end_angle: end,
        color,
        width,
        opa: opacity,
    };

    assert_eq!(
        actual.draw_request(&DrawRequest::new(&command, clip)),
        Ok(())
    );
    let path = Path::arc(center, radius, start, end);
    expected.stroke_path_transformed(
        &path,
        clip,
        &physical_transform,
        width,
        &Paint::Color(color.into()),
        opacity,
        crate::render::raster::LineCap::Butt,
        crate::render::raster::LineJoin::Miter,
        Fixed::from_int(4),
        &[],
    );
    assert_eq!(
        actual.target.read_pixels().unwrap().bytes(),
        expected.target.read_pixels().unwrap().bytes()
    );

    let capacity = actual.scratch.primitive_path.command_capacity();
    assert!(capacity >= path.commands().len());
    actual.target.pixels_mut().unwrap().bytes_mut().fill(0);
    assert_eq!(
        actual.draw_request(&DrawRequest::new(&command, clip)),
        Ok(())
    );
    assert_eq!(actual.scratch.primitive_path.command_capacity(), capacity);
}

#[test]
fn stroke_path_zero_width_is_noop() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    let mut path = crate::render::path::Path::new();
    path.move_to(Point {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
    })
    .line_to(Point {
        x: Fixed::from_int(8),
        y: Fixed::ZERO,
    });

    let clip = Rect::new(0, 0, 8, 8);
    let paint = Paint::Color(Color::rgb(255, 0, 0).into());
    backend.stroke_path(
        &path,
        &clip,
        Fixed::ZERO,
        &paint,
        255,
        crate::render::raster::LineCap::Butt,
        crate::render::raster::LineJoin::Miter,
        Fixed::from_int(4),
        &[],
    );

    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(backend.target.read_pixels().unwrap().get_pixel(x, y).r, 0);
        }
    }
}

#[test]
fn painter_fill_rect_with_backend() {
    use crate::render::painter::Painter;

    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    {
        let mut painter = Painter::new(&mut backend);
        let rect = Rect::new(1, 1, 6, 6);
        let clip = Rect::new(0, 0, 16, 16);
        painter.fill_rect(&rect, &clip, &Color::rgb(0, 255, 0), Fixed::ZERO, 255);
    }

    let c = backend.target.read_pixels().unwrap().get_pixel(3, 3);
    assert_eq!(c.r, 0);
    assert_eq!(c.g, 255);
    assert_eq!(c.b, 0);
}

#[test]
fn painter_forwards_path_and_stroke_methods() {
    use crate::render::painter::Painter;
    use crate::render::path::Path;

    let mut buf = vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let clip = Rect::new(0, 0, 32, 32);

    {
        let mut painter = Painter::new(&mut backend);
        let path = Path::rect(
            Fixed::from_int(4),
            Fixed::from_int(4),
            Fixed::from_int(10),
            Fixed::from_int(10),
        );
        let paint = Paint::Color(Color::rgb(255, 0, 0).into());
        painter.fill_path(
            &path,
            &clip,
            &paint,
            255,
            crate::render::raster::FillRule::EvenOdd,
        );

        painter.draw_line(
            Point {
                x: Fixed::from_int(20),
                y: Fixed::from_int(20),
            },
            Point {
                x: Fixed::from_int(28),
                y: Fixed::from_int(28),
            },
            &clip,
            Fixed::from_int(2),
            &Color::rgb(0, 255, 0),
            255,
        );

        painter.draw_arc(
            Point {
                x: Fixed::from_int(24),
                y: Fixed::from_int(8),
            },
            Fixed::from_int(4),
            Fixed::from_int(0),
            Fixed::from_int(90),
            &clip,
            Fixed::from_int(2),
            &Color::rgb(0, 0, 255),
            255,
        );
    }

    assert_eq!(backend.target.read_pixels().unwrap().get_pixel(8, 8).r, 255);
    assert!(backend.target.read_pixels().unwrap().get_pixel(24, 24).g > 0);
    assert!(
        backend.target.read_pixels().unwrap().get_pixel(28, 8).b > 0
            || backend.target.read_pixels().unwrap().get_pixel(27, 8).b > 0
            || backend.target.read_pixels().unwrap().get_pixel(28, 9).b > 0,
    );
}

#[test]
fn painter_draw_glyph_run_forwards_to_backend() {
    use crate::render::painter::Painter;

    let mut buf = vec![0u8; 32 * 16 * 4];
    let tex = Texture::new(&mut buf, 32, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let clip = Rect::new(0, 0, 32, 16);

    {
        let font = crate::render::font::Font::default();
        let mut painter = Painter::new(&mut backend);
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            crate::render::font::GlyphId::new(66),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        painter.draw_glyph_run(
            &Point {
                x: Fixed::from_int(1),
                y: Fixed::from_int(1),
            },
            &glyphs,
            &font,
            &clip,
            &Color::rgb(200, 100, 50),
            255,
        );
    }

    let mut found = false;
    for y in 0..16 {
        for x in 0..32 {
            if backend.target.read_pixels().unwrap().get_pixel(x, y).r > 0 {
                found = true;
                break;
            }
        }
    }
    assert!(found);
}

/// DDA and the divide-based slow path must land on the same src
/// sample for every dst pixel. Drive both on the same 4×4 → 7×5
/// non-integer scale and compare dst byte-for-byte.
#[test]
fn blit_dda_matches_generic_slow() {
    // Src: 4×4 ARGB with a distinct per-pixel red value so we can
    // tell which src pixel each dst pixel sampled.
    let mut src_buf = vec![0u8; 4 * 4 * 4];
    for y in 0..4 {
        for x in 0..4 {
            let i = (y * 4 + x) * 4;
            src_buf[i] = (y * 4 + x) as u8 * 16 + 1;
            src_buf[i + 3] = 255;
        }
    }
    let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    // Dst: 8×6, draw 7×5 blit at (0,0). Two identical dst buffers.
    let mut dst_a = vec![0u8; 8 * 6 * 4];
    let mut dst_b = vec![0u8; 8 * 6 * 4];

    {
        let mut tex_a =
            Texture::new(&mut dst_a, 8, 6, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_generic_slow(
            &mut tex_a.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            0,
            0,
            7,
            5,
            0,
            0,
            8,
            6,
            255,
        );
    }
    {
        let mut tex_b =
            Texture::new(&mut dst_b, 8, 6, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_dda(
            &mut tex_b.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            0,
            0,
            7,
            5,
            0,
            0,
            8,
            6,
            255,
            None,
        );
    }

    assert_eq!(dst_a, dst_b, "dda sampling diverged from divide path");
}

/// 1× fast path should match the slow path exactly on α==0 and
/// α==255 pixels, and within ±1 per channel on partial-α blends
/// (slow path uses Fixed map_range, fast path uses integer
/// `(src * a + dst * (255 - a)) / 255` — the two differ by at
/// most one LSB per channel).
#[test]
fn blit_1to1_matches_generic_for_argb_to_argb() {
    let mut src_buf = vec![0u8; 4 * 4 * 4];
    for i in 0..16 {
        src_buf[i * 4] = 30 + i as u8;
        src_buf[i * 4 + 1] = 60 + i as u8;
        src_buf[i * 4 + 2] = 90 + i as u8;
        src_buf[i * 4 + 3] = match i {
            0 => 0,
            3 | 7 => 128,
            _ => 255,
        };
    }
    let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_a = vec![0u8; 6 * 6 * 4];
    for (i, byte) in dst_a.iter_mut().enumerate() {
        *byte = (i * 3) as u8;
    }
    let mut dst_b = dst_a.clone();

    {
        let mut tex =
            Texture::new(&mut dst_a, 6, 6, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_generic_slow(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            4,
            4,
            0,
            0,
            6,
            6,
            255,
        );
    }
    {
        let mut tex =
            Texture::new(&mut dst_b, 6, 6, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_1to1_fast(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            0,
            0,
            6,
            6,
        );
    }
    for (i, (&a, &b)) in dst_a.iter().zip(dst_b.iter()).enumerate() {
        assert!(
            (a as i32 - b as i32).abs() <= 1,
            "byte {} diverged by more than 1: slow={} fast={}",
            i,
            a,
            b
        );
    }
}

#[test]
fn blit_1to1_matches_generic_for_argb_to_565sw() {
    let mut src_buf = vec![0u8; 4 * 4 * 4];
    for i in 0..16 {
        src_buf[i * 4] = 30 + i as u8 * 5;
        src_buf[i * 4 + 1] = 40 + i as u8 * 3;
        src_buf[i * 4 + 2] = 50 + i as u8 * 7;
        src_buf[i * 4 + 3] = if i == 0 { 0 } else { 255 };
    }
    let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_a = vec![0u8; 6 * 6 * 2];
    for (i, byte) in dst_a.iter_mut().enumerate() {
        *byte = (i * 5) as u8;
    }
    let mut dst_b = dst_a.clone();

    {
        let mut tex = Texture::new(&mut dst_a, 6, 6, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        blit_generic_slow(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            4,
            4,
            0,
            0,
            6,
            6,
            255,
        );
    }
    {
        let mut tex = Texture::new(&mut dst_b, 6, 6, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        blit_1to1_fast(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            0,
            0,
            6,
            6,
        );
    }
    assert_eq!(dst_a, dst_b);
}

/// 2× integer-scale fast path must produce the same 2×2 block
/// pattern as the slow DDA path for each source pixel. Check
/// 565sw→565sw (pure copy) for exact parity.
#[test]
fn blit_2to2_565sw_matches_dda() {
    let mut src_buf = vec![0u8; 3 * 3 * 2];
    for i in 0..9 {
        src_buf[i * 2] = 0x12 + i as u8;
        src_buf[i * 2 + 1] = 0x34 + i as u8;
    }
    let src =
        Texture::new(&mut src_buf, 3, 3, ColorFormat::RGB565Swapped).expect("valid pixel storage");

    let mut dst_a = vec![0u8; 10 * 10 * 2];
    let mut dst_b = vec![0u8; 10 * 10 * 2];
    {
        let mut tex = Texture::new(&mut dst_a, 10, 10, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        blit_dda(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            3,
            3,
            1,
            1,
            6,
            6,
            0,
            0,
            10,
            10,
            255,
            None,
        );
    }
    {
        let mut tex = Texture::new(&mut dst_b, 10, 10, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        blit_2to2_fast(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            3,
            3,
            1,
            1,
            0,
            0,
            10,
            10,
        );
    }
    assert_eq!(dst_a, dst_b);
}

/// 2× clip that lands on odd boundaries triggers the fast path's
/// DDA fallback; verify the fallback still produces correct output.
#[test]
fn blit_2to2_odd_clip_falls_back_cleanly() {
    let mut src_buf = vec![0u8; 2 * 2 * 2];
    src_buf[0] = 0xAA;
    src_buf[1] = 0xBB;
    let src =
        Texture::new(&mut src_buf, 2, 2, ColorFormat::RGB565Swapped).expect("valid pixel storage");

    let mut dst = vec![0u8; 6 * 6 * 2];
    let mut tex =
        Texture::new(&mut dst, 6, 6, ColorFormat::RGB565Swapped).expect("valid pixel storage");
    // Clip starts at odd x=1: fast path 2×2 alignment broken → DDA fallback.
    blit_2to2_fast(
        &mut tex.pixels_mut().unwrap(),
        &src.read_pixels().unwrap(),
        0,
        0,
        2,
        2,
        0,
        0,
        1,
        0,
        6,
        6,
    );
    // Should not panic; column 0 stays zero, column 1+ has pixels.
    assert_eq!(dst[0], 0);
    assert_eq!(dst[1], 0);
    assert_ne!(dst[2], 0);
}

#[test]
fn fill_rect_transformed_90deg_rotation() {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let mut dst =
        Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let rect = Rect::new(6, 6, 4, 4);
    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);
    let tf = Transform::translate(cx, cy)
        .compose(&Transform::rotate_deg(Fixed::from_int(90)))
        .compose(&Transform::translate(Fixed::ZERO - cx, Fixed::ZERO - cy));
    let red = Color::rgb(255, 0, 0);
    fill_rect_transformed(
        &mut dst.pixels_mut().unwrap(),
        rect,
        Rect::new(0, 0, 16, 16),
        &tf,
        &red,
        255,
    );

    let mut painted = 0;
    for y in 0..16 {
        for x in 0..16 {
            if dst.read_pixels().unwrap().get_pixel(x, y).r == 255 {
                painted += 1;
            }
        }
    }
    assert!(
        (12..=20).contains(&painted),
        "expected ~16 painted pixels, got {}",
        painted
    );
}

#[test]
fn renderer_transformed_fill_uses_logical_area_under_hidpi() {
    let mut buf = vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    renderer.viewport = Viewport::new(32, 32, Fixed::from_int(2));

    renderer.draw(
        &DrawCommand::Fill {
            area: Rect::new(4, 4, 8, 8),
            transform: Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
            quad: None,
            color: Color::rgb(255, 0, 0),
            radius: Fixed::ZERO,
            opa: 255,
        },
        &Rect::new(0, 0, 32, 32),
    );

    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(20, 20).r,
        255
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(10, 10).r,
        0
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(50, 50).r,
        0
    );
}

#[test]
fn blit_1to1_with_clip_restricted() {
    let mut src_buf = vec![0u8; 4 * 4 * 4];
    for i in 0..16 {
        src_buf[i * 4] = 100 + i as u8;
        src_buf[i * 4 + 3] = 255;
    }
    let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_a = vec![0u8; 8 * 8 * 4];
    let mut dst_b = vec![0u8; 8 * 8 * 4];

    // Clip covers only the right half of the blit rect.
    {
        let mut tex =
            Texture::new(&mut dst_a, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_generic_slow(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            4,
            4,
            3,
            0,
            8,
            8,
            255,
        );
    }
    {
        let mut tex =
            Texture::new(&mut dst_b, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_1to1_fast(
            &mut tex.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            3,
            0,
            8,
            8,
        );
    }
    assert_eq!(dst_a, dst_b);
}

/// Clip partially covering the dst rect must still produce the
/// same output as the divide-based slow path (every pixel outside
/// the clip untouched, every pixel inside matching src sampling).
#[test]
fn blit_dda_with_partial_clip() {
    let mut src_buf = vec![0u8; 4 * 4 * 4];
    for i in 0..16 {
        src_buf[i * 4] = (i * 16 + 1) as u8;
        src_buf[i * 4 + 3] = 255;
    }
    let src = Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_a = vec![0u8; 10 * 10 * 4];
    let mut dst_b = vec![0u8; 10 * 10 * 4];
    // Clip covers columns 2..7 only.
    {
        let mut tex_a =
            Texture::new(&mut dst_a, 10, 10, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_generic_slow(
            &mut tex_a.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            8,
            8,
            2,
            0,
            7,
            10,
            255,
        );
    }
    {
        let mut tex_b =
            Texture::new(&mut dst_b, 10, 10, ColorFormat::RGBA8888).expect("valid pixel storage");
        blit_dda(
            &mut tex_b.pixels_mut().unwrap(),
            &src.read_pixels().unwrap(),
            0,
            0,
            4,
            4,
            1,
            1,
            8,
            8,
            2,
            0,
            7,
            10,
            255,
            None,
        );
    }
    assert_eq!(dst_a, dst_b);
}

#[test]
fn sw_renderer_supports_offscreen() {
    let mut buf = vec![0u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let backend = SwRenderer::new(tex);
    assert!(Renderer::supports_offscreen(&backend));
}

/// View `stride` must equal the framebuffer's so writing every
/// pixel of the borrowed view hits exactly the rect's pixels in
/// the underlying buffer.
#[test]
fn modify_target_region_writes_exact_rect() {
    let mut buf = vec![0u8; 16 * 16 * 4];
    let tex = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let rect = Rect::new(
        Fixed::from_int(4),
        Fixed::from_int(4),
        Fixed::from_int(6),
        Fixed::from_int(5),
    );
    let ran = backend.modify_target_region(&rect, &mut |view| {
        assert_eq!(view.width, 6);
        assert_eq!(view.height, 5);
        assert_eq!(view.stride, 16 * 4);
        for y in 0..view.height as i32 {
            for x in 0..view.width as i32 {
                view.pixels_mut()
                    .unwrap()
                    .set_pixel(x, y, &Color::rgb(255, 128, 64));
            }
        }
        Ok(())
    });
    assert_eq!(ran, Ok(true));

    let target = &backend.target;
    for py in 0..16i32 {
        for px in 0..16i32 {
            let p = target.read_pixels().unwrap().get_pixel(px, py);
            let in_rect = (4..10).contains(&px) && (4..9).contains(&py);
            if in_rect {
                assert_eq!((p.r, p.g, p.b), (255, 128, 64), "in-rect ({px},{py})");
            } else {
                assert_eq!((p.r, p.g, p.b), (0, 0, 0), "outside-rect ({px},{py})");
            }
        }
    }
}

#[test]
fn modify_target_region_propagates_callback_failure() {
    let mut backend =
        SwRenderer::new(Texture::owned(8, 8, ColorFormat::RGBA8888).expect("valid pixel storage"));
    let result = backend.modify_target_region(&Rect::new(0, 0, 4, 4), &mut |_| {
        Err(RenderError::ResourceLimit(RenderResource::Geometry))
    });
    assert_eq!(
        result,
        Err(RenderError::ResourceLimit(RenderResource::Geometry))
    );
    assert_eq!(backend.target.cache_revision, 1);
}

#[test]
fn region_writes_and_readback_share_checked_pixel_leases() {
    let mut bytes = [0x35; 44];
    let target = Texture::new(&mut bytes, 3, 3, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(16)
        .unwrap()
        .with_cache_revision(10);
    let mut renderer = SwRenderer::new(target);
    let mut copied = Texture::owned(1, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_cache_revision(20);
    let area = Rect::new(2, 1, 1, 2);
    let allocations = crate::test_support::tracked_allocations(|| {
        assert_eq!(
            renderer.modify_target_region(&area, &mut |view| {
                assert!(view.transient);
                let mut pixels = view.pixels_mut()?;
                assert_eq!(pixels.stride(), 16);
                assert_eq!(pixels.bytes().len(), 20);
                pixels.set_pixel(0, 0, &Color::rgb(255, 0, 0));
                pixels.set_pixel(0, 1, &Color::rgb(0, 255, 0));
                Ok(())
            }),
            Ok(true)
        );
        renderer.read_target_region(&area, &mut copied).unwrap();
    });
    assert_eq!(allocations, 0);
    assert_eq!(renderer.target.cache_revision, 11);
    assert_eq!(copied.cache_revision, 21);
    let pixels = copied.read_pixels().unwrap();
    assert_eq!(pixels.get_pixel(0, 0), Color::rgb(255, 0, 0));
    assert_eq!(pixels.get_pixel(0, 1), Color::rgb(0, 255, 0));
    let target = renderer.target.read_pixels().unwrap();
    assert!(target.bytes()[..24].iter().all(|byte| *byte == 0x35));
    assert!(target.bytes()[28..40].iter().all(|byte| *byte == 0x35));
}

#[test]
fn failed_source_cannot_be_read_back_or_modified_as_a_successful_frame() {
    let mut renderer = SwRenderer::new(Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap());
    renderer.failure = Some(RenderError::BackendFailure);
    let mut destination = Texture::owned(4, 4, ColorFormat::RGBA8888).unwrap();
    let area = Rect::new(0, 0, 4, 4);
    assert_eq!(
        renderer.read_target_region(&area, &mut destination),
        Err(RenderError::BackendFailure)
    );
    assert!(matches!(
        renderer.sample_target_region(&area),
        Err(RenderError::BackendFailure)
    ));
    assert_eq!(
        renderer.modify_target_region(&area, &mut |_| panic!("failed frame callback")),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(
        renderer.scroll_target_region(&area, Fixed::ZERO, Fixed::ONE),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(renderer.target.cache_revision, 0);
    assert_eq!(destination.cache_revision, 0);
}

#[test]
fn raster_batches_advance_revision_without_pixel_allocations() {
    let target = Texture::owned(16, 16, ColorFormat::RGBA8888).unwrap();
    let mut renderer = SwRenderer::new(target);
    let area = Rect::new(0, 0, 16, 16);
    let fill = DrawCommand::Fill {
        area,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(30, 60, 90),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let blur = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE / 2,
        region: area,
    };
    let allocations = crate::test_support::tracked_allocations(|| {
        for revision in 1..=4 {
            renderer
                .draw_request(&DrawRequest::new(&fill, area))
                .unwrap();
            assert_eq!(renderer.target.cache_revision, revision * 2 - 1);
            renderer
                .draw_request(&DrawRequest::new(&blur, area))
                .unwrap();
            assert_eq!(renderer.target.cache_revision, revision * 2);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(8, 8),
        Color::rgb(30, 60, 90)
    );
}

#[test]
fn raster_writes_reject_readonly_targets_without_copying() {
    let bytes = [0u8; 16];
    let area = Rect::new(0, 0, 2, 2);
    let fill = DrawCommand::Fill {
        area,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(30, 60, 90),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let blur = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE / 2,
        region: area,
    };
    for command in [&fill, &blur] {
        let texture = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
        let mut renderer = SwRenderer::new(texture);
        let allocations = crate::test_support::tracked_allocations(|| {
            assert_eq!(
                renderer.draw_request(&DrawRequest::new(command, area)),
                Err(RenderError::InvalidTexture)
            );
            assert_eq!(
                Canvas::flush(&mut renderer),
                Err(RenderError::InvalidTexture)
            );
        });
        assert_eq!(allocations, 0);
        assert_eq!(renderer.target.cache_revision, 0);
        assert_eq!(
            renderer.target.read_pixels().unwrap().bytes().as_ptr(),
            bytes.as_ptr()
        );
    }
}

#[test]
fn target_region_access_rejects_invalid_storage_before_writing() {
    let bytes = [0u8; 16];
    let mut renderer =
        SwRenderer::new(Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap());
    let area = Rect::new(0, 0, 2, 2);
    assert_eq!(
        renderer.modify_target_region(&area, &mut |_| panic!("read-only target")),
        Err(RenderError::InvalidTexture)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().bytes().as_ptr(),
        bytes.as_ptr()
    );
    let mut destination = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
    renderer.target.stride = 16;
    assert_eq!(
        renderer.read_target_region(&area, &mut destination),
        Err(RenderError::InvalidTexture)
    );
    assert_eq!(destination.cache_revision, 0);
    assert!(
        destination
            .read_pixels()
            .unwrap()
            .bytes()
            .iter()
            .all(|byte| *byte == 0)
    );
}

#[test]
fn scroll_target_region_shift_up_moves_rows_correctly() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    // Per-row red gradient (Y * 30) so each row is distinguishable.
    for y in 0..8 {
        for x in 0..8 {
            let off = (y * 8 + x) * 4;
            buf[off] = (y * 30) as u8;
            buf[off + 3] = 255;
        }
    }
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    backend
        .scroll_target_region(&area, Fixed::ZERO, Fixed::from_int(-2))
        .unwrap();

    // Rows 0..6 should now hold pre-scroll rows 2..8; rows 6, 7
    // are the bottom strip the caller is meant to repaint, so
    // their post-shift contents are unspecified and not checked.
    for y in 0..6 {
        let src_y_before = y + 2;
        let p = backend.target.read_pixels().unwrap().get_pixel(0, y);
        assert_eq!(
            p.r,
            (src_y_before * 30) as u8,
            "row {y} should hold pre-scroll row {src_y_before}",
        );
    }
}

#[test]
fn scroll_target_region_shift_down_moves_rows_correctly() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let off = (y * 8 + x) * 4;
            buf[off] = (y * 30) as u8;
            buf[off + 3] = 255;
        }
    }
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    backend
        .scroll_target_region(&area, Fixed::ZERO, Fixed::from_int(2))
        .unwrap();

    for y in 2..8 {
        let src_y_before = y - 2;
        let p = backend.target.read_pixels().unwrap().get_pixel(0, y);
        assert_eq!(
            p.r,
            (src_y_before * 30) as u8,
            "row {y} should hold pre-scroll row {src_y_before}",
        );
    }
}

/// Sub-pixel dy truncates to zero physical pixels and is a no-op;
/// caller is expected to keep and re-submit the residue.
#[test]
fn scroll_target_region_sub_pixel_dy_is_noop() {
    let mut buf = vec![1u8; 4 * 4 * 4];
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let half = Fixed::from_int(1) / Fixed::from_int(2);
    backend
        .scroll_target_region(&area, Fixed::ZERO, half)
        .unwrap();
    for px in backend.target.read_pixels().unwrap().bytes() {
        assert_eq!(*px, 1);
    }
}

/// Negative fractional movement must not round into a full-pixel transfer.
#[test]
fn scroll_target_region_negative_sub_pixel_dy_is_noop() {
    // Stripe the buf per-row so a 1-row shift would be visible.
    let mut buf = vec![0u8; 4 * 4 * 4];
    for y in 0..4 {
        for x in 0..4 {
            let off = (y * 4 + x) * 4;
            buf[off] = (y * 50) as u8;
            buf[off + 3] = 255;
        }
    }
    let snapshot = buf.clone();
    let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let neg_half = Fixed::ZERO - (Fixed::from_int(1) / Fixed::from_int(2));
    backend
        .scroll_target_region(&area, Fixed::ZERO, neg_half)
        .unwrap();
    assert_eq!(
        backend.target.read_pixels().unwrap().bytes(),
        snapshot.as_slice(),
        "negative sub-pixel dy must not modify the buffer"
    );
}

#[test]
fn scroll_target_region_shift_left_moves_columns_correctly() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let off = (y * 8 + x) * 4;
            buf[off] = (x * 30) as u8;
            buf[off + 3] = 255;
        }
    }
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    backend
        .scroll_target_region(&area, Fixed::from_int(-2), Fixed::ZERO)
        .unwrap();

    for x in 0..6 {
        let src_x_before = x + 2;
        let p = backend.target.read_pixels().unwrap().get_pixel(x, 3);
        assert_eq!(
            p.r,
            (src_x_before * 30) as u8,
            "col {x} should hold pre-scroll col {src_x_before}",
        );
    }
}

#[test]
fn scroll_target_region_shift_right_moves_columns_correctly() {
    let mut buf = vec![0u8; 8 * 8 * 4];
    for y in 0..8 {
        for x in 0..8 {
            let off = (y * 8 + x) * 4;
            buf[off] = (x * 30) as u8;
            buf[off + 3] = 255;
        }
    }
    let tex = Texture::new(&mut buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);
    let area = Rect::new(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(8),
    );
    backend
        .scroll_target_region(&area, Fixed::from_int(2), Fixed::ZERO)
        .unwrap();

    for x in 2..8 {
        let src_x_before = x - 2;
        let p = backend.target.read_pixels().unwrap().get_pixel(x, 3);
        assert_eq!(
            p.r,
            (src_x_before * 30) as u8,
            "col {x} should hold pre-scroll col {src_x_before}",
        );
    }
}

#[test]
fn opaque_mode_writes_full_alpha() {
    // Default mode (Opaque): every blend-pixel write must leave
    // dst.a = 255 regardless of source alpha. Regression guard
    // for the framebuffer path post-AlphaMode introduction.
    let mut buf = std::vec![0u8; 4 * 4];
    let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 100), 100);
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(1, 0, &Color::rgba(0, 255, 0, 200), 200);
    assert_eq!(
        tex.read_pixels().unwrap().get_pixel(0, 0).a,
        255,
        "opaque mode: dst.a always 255"
    );
    assert_eq!(
        tex.read_pixels().unwrap().get_pixel(1, 0).a,
        255,
        "opaque mode: dst.a always 255"
    );
}

#[test]
fn blend_alpha_accumulates_on_clear_transparent() {
    // Blend mode + transparent dst (a=0): a single fill of partial
    // alpha must leave dst.a = src.a, not 255.
    let mut buf = std::vec![0u8; 4 * 4];
    let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.alpha_mode = AlphaMode::Blend;
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 128), 128);
    let p = tex.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(
        p.a, 128,
        "blend mode + transparent dst: dst.a should equal src.a, got {}",
        p.a,
    );
}

#[test]
fn blend_alpha_blends_when_dst_partial() {
    // Two overlapping fills in Blend mode must compose alpha via
    //   out.a = src.a + dst.a * (255 − src.a) / 255
    // First fill leaves dst.a = 100; second fill src.a = 100 over
    // that should give roughly 100 + 100 × 155 / 255 = 161.
    let mut buf = std::vec![0u8; 4 * 4];
    let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.alpha_mode = AlphaMode::Blend;
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 100), 100);
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(0, 0, 255, 100), 100);
    let p = tex.read_pixels().unwrap().get_pixel(0, 0);
    // ±2 tolerance for u8 rounding in two-step source-over.
    let expected = 100 + (100 * 155) / 255;
    assert!(
        (p.a as i32 - expected).abs() <= 2,
        "blend mode source-over: expected ~{expected}, got {}",
        p.a,
    );
}

#[test]
fn blend_a_eq_255_writes_full_alpha_in_blend_mode() {
    // Source-over identity at a=255: fully opaque source covers
    // dst, so dst.a = 255 even in Blend mode. Pins the
    // short-circuit behaviour for the common case.
    let mut buf = std::vec![0u8; 4 * 4];
    let mut tex = Texture::new(&mut buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    tex.alpha_mode = AlphaMode::Blend;
    tex.pixels_mut()
        .unwrap()
        .blend_pixel_int(0, 0, &Color::rgba(255, 0, 0, 255), 255);
    assert_eq!(tex.read_pixels().unwrap().get_pixel(0, 0).a, 255);
}

// opa=128, opaque red 4×4 src onto transparent target — dispatch
// must route through blit_dda (per-pixel blend), NOT the 1to1
// memcpy fast path, else opa would be silently ignored.
#[test]
fn blit_opa_128_midgray_red_source() {
    let mut tgt_buf = std::vec![0u8; 4 * 4 * 4];
    let tgt_tex =
        Texture::new(&mut tgt_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tgt_tex);

    let mut src_buf = std::vec![0u8; 4 * 4 * 4];
    for px in src_buf.chunks_exact_mut(4) {
        px[0] = 255;
        px[1] = 0;
        px[2] = 0;
        px[3] = 255;
    }
    let src_tex =
        Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let src_rect = Rect::new(0, 0, 4, 4);
    let dst = Point::new(Fixed::ZERO, Fixed::ZERO);
    let dst_size = Point::new(Fixed::from_int(4), Fixed::from_int(4));
    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(4),
        h: Fixed::from_int(4),
    };
    backend.blit(
        &src_tex,
        &src_rect,
        dst,
        dst_size,
        &clip,
        128,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );

    // effective_a = (255 * 128) / 255 = 128; source-over onto
    // transparent dst yields dst.rgb ≈ 128 red. Target is Opaque
    // mode by default so dst.a stays out of the compositing eqn.
    for y in 0..4 {
        for x in 0..4 {
            let p = backend.target.read_pixels().unwrap().get_pixel(x, y);
            assert!(
                p.r >= 120 && p.r <= 135,
                "({x},{y}) red channel {} not in [120,135]",
                p.r,
            );
            assert_eq!((p.g, p.b), (0, 0), "({x},{y}) green/blue must stay zero",);
        }
    }
}

// opa==0 short-circuits before any pixel write — worst-case
// group opacity must keep the fast path's no-op semantics.
#[test]
fn blit_opa_zero_skips_writes() {
    let mut tgt_buf = std::vec![0u8; 4 * 4 * 4];
    let tgt_tex =
        Texture::new(&mut tgt_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tgt_tex);

    let mut src_buf = std::vec![255u8; 4 * 4 * 4];
    let src_tex =
        Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let src_rect = Rect::new(0, 0, 4, 4);
    let dst = Point::new(Fixed::ZERO, Fixed::ZERO);
    let dst_size = Point::new(Fixed::from_int(4), Fixed::from_int(4));
    let clip = Rect {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        w: Fixed::from_int(4),
        h: Fixed::from_int(4),
    };
    backend.blit(
        &src_tex,
        &src_rect,
        dst,
        dst_size,
        &clip,
        0,
        Fixed::ZERO,
        CompositeMode::SourceOver,
    );

    for y in 0..4 {
        for x in 0..4 {
            let p = backend.target.read_pixels().unwrap().get_pixel(x, y);
            assert_eq!((p.r, p.g, p.b, p.a), (0, 0, 0, 0));
        }
    }
}

// opa==255 must produce byte-identical output to the pre-opa
// fast path. 1to1 RGBA→RGBA path exploits `(src_a * 255) / 255
// == src_a`, the bit-exact identity the dispatch's `opa < 255`
// guard relies on.
#[test]
fn blit_opa_255_bit_exact_with_fast_path() {
    let make_target = || {
        let buf = std::vec![0u8; 4 * 4 * 4];
        (buf, Vec::<u8>::new())
    };
    let (mut tgt_buf_a, _) = make_target();
    let (mut tgt_buf_b, _) = make_target();

    let mut src_buf = std::vec![0u8; 4 * 4 * 4];
    for (i, px) in src_buf.chunks_exact_mut(4).enumerate() {
        px[0] = (i * 16) as u8;
        px[1] = (255 - i * 8) as u8;
        px[2] = (i * 4) as u8;
        px[3] = 255;
    }

    for (tgt, opa) in [(&mut tgt_buf_a, 255), (&mut tgt_buf_b, 255)] {
        let tgt_tex = Texture::new(tgt, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut backend = SwRenderer::new(tgt_tex);
        let src_tex =
            Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
        let src_rect = Rect::new(0, 0, 4, 4);
        let dst = Point::new(Fixed::ZERO, Fixed::ZERO);
        let dst_size = Point::new(Fixed::from_int(4), Fixed::from_int(4));
        let clip = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(4),
            h: Fixed::from_int(4),
        };
        backend.blit(
            &src_tex,
            &src_rect,
            dst,
            dst_size,
            &clip,
            opa,
            Fixed::ZERO,
            CompositeMode::SourceOver,
        );
    }

    assert_eq!(tgt_buf_a, tgt_buf_b, "opa==255 must be deterministic");
    assert_eq!(tgt_buf_a, src_buf, "opa==255 must equal source");
}

// FillPath under a scale transform used to panic (sw/sdl_gpu/wgpu
// draw_transformed had no FillPath arm). Icon widget needs this
// path to render a viewBox-units path scaled to physical pixels.
#[test]
fn fill_path_scale_transform_renders_without_panic() {
    use crate::render::command::DrawCommand;
    use crate::types::Transform;

    let mut buf = vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut backend = SwRenderer::new(tex);

    // 4×4 unit square; scale(4,4) should fill a 16×16 region at origin.
    let path = crate::render::path::Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let clip = Rect::new(0, 0, 32, 32);
    let paint = Paint::Color(Color::rgb(0, 255, 0).into());
    let cmd = DrawCommand::FillPath {
        path: &path,
        transform: Transform::scale(Fixed::from_int(4), Fixed::from_int(4)),
        paint: &paint,
        opa: 255,
        fill_rule: crate::render::raster::FillRule::EvenOdd,
    };
    backend.draw(&cmd, &clip);

    let inside = backend.target.read_pixels().unwrap().get_pixel(8, 8);
    assert_eq!(inside.g, 255, "scaled rect interior should be green");
    let outside = backend.target.read_pixels().unwrap().get_pixel(20, 20);
    assert_eq!(outside.g, 0, "outside scaled rect must stay zero");
}

fn run_composite_blit(
    src_rgba: [u8; 4],
    dst_rgba: [u8; 4],
    composite: CompositeMode,
    opa: u8,
) -> Color {
    let mut src_buf = std::vec![src_rgba[0], src_rgba[1], src_rgba[2], src_rgba[3]];
    let src_tex =
        Texture::new(&mut src_buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_buf = std::vec![dst_rgba[0], dst_rgba[1], dst_rgba[2], dst_rgba[3]];
    let mut backend = SwRenderer::new(
        Texture::new(&mut dst_buf, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    backend.viewport = Viewport::new(1, 1, Fixed::ONE);

    let cmd = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point {
            x: Fixed::from_int(1),
            y: Fixed::from_int(1),
        },
        transform: Transform::IDENTITY,
        quad: None,
        texture: &src_tex,
        opa,
        radius: Fixed::ZERO,
        composite,
    };
    backend.draw(&cmd, &Rect::new(0, 0, 1, 1));
    backend.target.read_pixels().unwrap().get_pixel(0, 0)
}

#[test]
fn composite_add_saturates_at_full_intensity() {
    let out = run_composite_blit([128, 0, 0, 255], [128, 0, 0, 255], CompositeMode::Add, 255);
    assert_eq!(out.r, 255);
}

#[test]
fn composite_screen_blends_half_on_half() {
    let out = run_composite_blit(
        [128, 0, 0, 255],
        [128, 0, 0, 255],
        CompositeMode::Screen,
        255,
    );
    assert!(
        (190..=192).contains(&out.r),
        "screen(128,128) = ~191; got {}",
        out.r
    );
}

#[test]
fn composite_multiply_halves_at_50_percent() {
    let out = run_composite_blit(
        [128, 0, 0, 255],
        [128, 0, 0, 255],
        CompositeMode::Multiply,
        255,
    );
    assert!(
        (63..=65).contains(&out.r),
        "multiply(128,128) = ~64; got {}",
        out.r
    );
}

#[test]
fn composite_darken_picks_smaller_channel() {
    let out = run_composite_blit(
        [64, 0, 0, 255],
        [192, 0, 0, 255],
        CompositeMode::Darken,
        255,
    );
    assert_eq!(out.r, 64);
}

#[test]
fn composite_lighten_picks_larger_channel() {
    let out = run_composite_blit(
        [64, 0, 0, 255],
        [192, 0, 0, 255],
        CompositeMode::Lighten,
        255,
    );
    assert_eq!(out.r, 192);
}

#[test]
fn composite_difference_is_absolute_diff() {
    let out = run_composite_blit(
        [192, 0, 0, 255],
        [64, 0, 0, 255],
        CompositeMode::Difference,
        255,
    );
    assert_eq!(out.r, 128);
}

#[test]
fn composite_with_zero_src_alpha_preserves_dst() {
    for mode in [
        CompositeMode::Add,
        CompositeMode::Screen,
        CompositeMode::Multiply,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::Difference,
    ] {
        let out = run_composite_blit([255, 255, 255, 0], [50, 80, 110, 255], mode, 255);
        assert_eq!(
            (out.r, out.g, out.b),
            (50, 80, 110),
            "mode {mode:?} clobbered dst"
        );
    }
}

#[test]
fn composite_half_alpha_src_blends_mode_against_dst() {
    let out = run_composite_blit([128, 0, 0, 128], [128, 0, 0, 255], CompositeMode::Add, 255);
    assert!(
        (190..=192).contains(&out.r),
        "Add half-alpha = ~191; got {}",
        out.r
    );
}

#[test]
fn blit_radius_mask_clips_corner_pixels() {
    let mut src_buf = vec![255u8; 8 * 8 * 4];
    let src_tex =
        Texture::new(&mut src_buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_buf = vec![0u8; 8 * 8 * 4];
    let mut backend = SwRenderer::new(
        Texture::new(&mut dst_buf, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    backend.viewport = Viewport::new(8, 8, Fixed::ONE);

    let cmd = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point {
            x: Fixed::from_int(8),
            y: Fixed::from_int(8),
        },
        transform: Transform::IDENTITY,
        quad: None,
        texture: &src_tex,
        opa: 255,
        radius: Fixed::from_int(3),
        composite: CompositeMode::SourceOver,
    };
    backend.draw(&cmd, &Rect::new(0, 0, 8, 8));

    let corner = backend.target.read_pixels().unwrap().get_pixel(0, 0);
    assert_eq!(corner.r, 0, "rounded corner must clip top-left pixel");
    let center = backend.target.read_pixels().unwrap().get_pixel(4, 4);
    assert_eq!(center.r, 255, "rect interior unaffected by mask");
}

#[test]
fn blit_zero_radius_matches_unmasked_path() {
    let mut src_buf = vec![200u8; 4 * 4 * 4];
    let src_tex =
        Texture::new(&mut src_buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");

    let mut dst_no_radius = vec![0u8; 4 * 4 * 4];
    let mut dst_zero_radius = vec![0u8; 4 * 4 * 4];

    for (buf, radius) in [
        (&mut dst_no_radius, Fixed::ZERO),
        (&mut dst_zero_radius, Fixed::ZERO),
    ] {
        let mut backend = SwRenderer::new(
            Texture::new(buf, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage"),
        );
        backend.viewport = Viewport::new(4, 4, Fixed::ONE);
        let cmd = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point {
                x: Fixed::from_int(4),
                y: Fixed::from_int(4),
            },
            transform: Transform::IDENTITY,
            quad: None,
            texture: &src_tex,
            opa: 255,
            radius,
            composite: CompositeMode::SourceOver,
        };
        backend.draw(&cmd, &Rect::new(0, 0, 4, 4));
    }

    assert_eq!(dst_no_radius, dst_zero_radius);
}

#[test]
fn fractional_quad_blit_preserves_coverage_under_quarter_turns_and_reflections() {
    let source = [240u8, 80, 20, 255];
    let texture =
        Texture::from_ref(&source, 1, 1, ColorFormat::RGBA8888).expect("valid pixel storage");
    // The same quarter-pixel footprint is sampled under either axis edge order.
    let corners = [
        Point::new(Fixed::from_ratio(11, 8), Fixed::ONE),
        Point::new(Fixed::from_ratio(13, 8), Fixed::ONE),
        Point::new(Fixed::from_ratio(13, 8), Fixed::from_int(2)),
        Point::new(Fixed::from_ratio(11, 8), Fixed::from_int(2)),
    ];
    for opa in [255, 128] {
        let unrounded_alpha = if opa == 255 { 63 } else { 32 };
        for radius in [Fixed::ZERO, Fixed::HALF] {
            let mut reference = None;
            for turn in 0..4 {
                for reflected in [false, true] {
                    let quad = core::array::from_fn(|index| {
                        let offset = if reflected { 4 - index } else { index };
                        corners[(turn + offset) % 4]
                    });
                    let mut pixels = [0u8; 3 * 3 * 4];
                    let mut target = Texture::new(&mut pixels, 3, 3, ColorFormat::RGBA8888)
                        .expect("valid pixel storage");
                    target.alpha_mode = AlphaMode::Blend;
                    let mut renderer = SwRenderer::new(target);
                    let command = DrawCommand::Blit {
                        pos: Point::new(corners[0].x, corners[0].y),
                        size: Point::new(1, 1),
                        transform: Transform::IDENTITY,
                        quad: Some(quad),
                        texture: &texture,
                        opa,
                        radius,
                        composite: CompositeMode::SourceOver,
                    };
                    assert_eq!(
                        renderer.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 3, 3))),
                        Ok(())
                    );
                    let edge = renderer.target.read_pixels().unwrap().get_pixel(1, 1);
                    assert_eq!((edge.r, edge.g, edge.b), (240, 80, 20));
                    assert!(edge.a > 0, "turn {turn}, reflected {reflected}");
                    if radius == Fixed::ZERO {
                        assert_eq!(edge.a, unrounded_alpha);
                    } else {
                        assert!(edge.a < unrounded_alpha, "rounded mask must remain active");
                    }
                    let result: [Color; 9] = core::array::from_fn(|index| {
                        renderer
                            .target
                            .read_pixels()
                            .unwrap()
                            .get_pixel((index % 3) as i32, (index / 3) as i32)
                    });
                    if let Some(expected) = reference {
                        assert_eq!(result, expected, "turn {turn}, reflected {reflected}");
                    } else {
                        reference = Some(result);
                    }
                    for (index, pixel) in result.iter().enumerate() {
                        if index != 4 {
                            assert_eq!(pixel.a, 0, "pixel {index} is outside the footprint");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn projected_quad_blit_preserves_radius_opacity_and_composite() {
    let src = [255u8, 0, 0, 255].repeat(16);
    let texture =
        Texture::from_ref(&src, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut pixels = [0u8, 0, 255, 255].repeat(12 * 12);
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 12, 12, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let command = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: Some([
            Point::new(2, 2),
            Point::new(10, 2),
            Point::new(10, 10),
            Point::new(2, 10),
        ]),
        texture: &texture,
        opa: 128,
        radius: Fixed::from_int(2),
        composite: CompositeMode::Difference,
    };
    let clip = Rect::new(0, 0, 12, 12);

    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Ok(())
    );
    let corner = renderer.target.read_pixels().unwrap().get_pixel(2, 2);
    let center = renderer.target.read_pixels().unwrap().get_pixel(6, 6);
    assert!(center.r > 0 && center.b > 0);
    assert!(corner.r < center.r);
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(0, 0),
        Color::rgb(0, 0, 255)
    );
}

#[test]
fn projective_blit_accepts_effects_without_intermediate_storage() {
    let src = [255u8, 0, 0, 255].repeat(16);
    let texture =
        Texture::from_ref(&src, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut pixels = [0u8, 0, 255, 255].repeat(12 * 12);
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 12, 12, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let command = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 128,
        radius: Fixed::from_int(2),
        composite: CompositeMode::Difference,
    };
    let clip = Rect::new(0, 0, 12, 12);
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));

    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(())
    );
    let center = renderer.target.read_pixels().unwrap().get_pixel(6, 6);
    assert!(center.r > 0 && center.b > 0);
}

#[test]
fn projected_blit_corner_radius_tracks_local_geometry() {
    let source = [255u8, 0, 0, 255].repeat(16);
    let texture =
        Texture::from_ref(&source, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let quad = [
        Point::new(2, 2),
        Point::new(22, 2),
        Point::new(16, 22),
        Point::new(8, 22),
    ];
    let mut first = [0u8; 24 * 24 * 4];
    let mut second = [0u8; 24 * 24 * 4];
    for (pixels, size, radius) in [
        (&mut first, Point::new(20, 20), Fixed::from_int(4)),
        (&mut second, Point::new(40, 40), Fixed::from_int(8)),
    ] {
        let mut target =
            Texture::new(pixels, 24, 24, ColorFormat::RGBA8888).expect("valid pixel storage");
        target.alpha_mode = AlphaMode::Blend;
        blit_quad(
            &mut target.pixels_mut().unwrap(),
            &texture.read_pixels().unwrap(),
            &quad,
            Rect::new(0, 0, 24, 24),
            size,
            radius,
            255,
            CompositeMode::SourceOver,
            None,
        );
    }
    for y in 7..22 {
        for x in 2..22 {
            let offset = (y * 24 + x) * 4;
            assert_eq!(first[offset] > 0, second[offset] > 0, "pixel ({x}, {y})");
        }
    }
    assert!(first[(20 * 24 + 9) * 4] > 0);
}

#[test]
fn projected_quads_combine_the_active_path_clip() {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 8;
    let quad = [
        Point::new(0, 0),
        Point::new(WIDTH as i32, 0),
        Point::new(WIDTH as i32, HEIGHT as i32),
        Point::new(0, HEIGHT as i32),
    ];
    let clip = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
    let mut mask = [0u8; WIDTH * HEIGHT];
    for row in mask.chunks_exact_mut(WIDTH) {
        row[..WIDTH / 2].fill(255);
    }

    let mut fill_pixels = [0u8; WIDTH * HEIGHT * 4];
    let mut fill_target = Texture::new(
        &mut fill_pixels,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    fill_rect_quad(
        &mut fill_target.pixels_mut().unwrap(),
        &quad,
        clip,
        &Color::rgb(220, 20, 30),
        Fixed::ZERO,
        Fixed::from_int(WIDTH as i32),
        Fixed::from_int(HEIGHT as i32),
        255,
        Some(&mask),
    );
    assert_eq!(fill_target.read_pixels().unwrap().get_pixel(4, 4).r, 220);
    assert_eq!(fill_target.read_pixels().unwrap().get_pixel(12, 4).a, 0);

    let source_pixels = [255u8; WIDTH * HEIGHT * 4];
    let source = Texture::from_ref(
        &source_pixels,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    let mut blit_pixels = [0u8; WIDTH * HEIGHT * 4];
    let mut blit_target = Texture::new(
        &mut blit_pixels,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    blit_quad(
        &mut blit_target.pixels_mut().unwrap(),
        &source.read_pixels().unwrap(),
        &quad,
        clip,
        Point::new(WIDTH as i32, HEIGHT as i32),
        Fixed::ZERO,
        255,
        CompositeMode::SourceOver,
        Some(&mask),
    );
    assert_eq!(blit_target.read_pixels().unwrap().get_pixel(4, 4).a, 255);
    assert_eq!(blit_target.read_pixels().unwrap().get_pixel(12, 4).a, 0);

    let mut stroke_pixels = [0u8; WIDTH * HEIGHT * 4];
    let mut stroke_target = Texture::new(
        &mut stroke_pixels,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    stroke_rect_quad(
        &mut stroke_target.pixels_mut().unwrap(),
        &quad,
        clip,
        &Color::rgb(20, 220, 30),
        Fixed::from_int(2),
        Fixed::ZERO,
        255,
        Some(&mask),
    );
    assert!(stroke_target.read_pixels().unwrap().get_pixel(4, 0).g > 0);
    assert_eq!(stroke_target.read_pixels().unwrap().get_pixel(12, 0).a, 0);
}

#[test]
fn projective_glyph_run_renders_without_an_intermediate_surface() {
    let mut pixels = vec![0u8; 32 * 16 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 32, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let font = crate::render::font::Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let command = DrawCommand::GlyphRun {
        pos: Point::new(8, 4),
        transform: Transform::IDENTITY,
        glyphs: &glyphs,
        font: &font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    };
    let projective = Transform3D::rotate_y_perspective(Fixed::from_int(18), Fixed::from_int(400));

    assert_eq!(
        renderer.draw_projective(&command, &Rect::new(0, 0, 32, 16), &projective),
        Ok(())
    );
    assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] != 0));
}

#[test]
fn projective_glyph_run_rejects_ink_behind_near_plane() {
    let mut pixels = vec![0u8; 16 * 16 * 4];
    let before = pixels.clone();
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let font = crate::render::font::Font::default();
    let glyphs = [textflow::shaping::PositionedGlyph::new(
        crate::render::font::GlyphId::new(65),
        textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
    )];
    let command = DrawCommand::GlyphRun {
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        glyphs: &glyphs,
        font: &font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    };
    let projective = Transform3D {
        m20: crate::types::Fixed64::ONE,
        m22: crate::types::Fixed64::from_int(-4),
        ..Transform3D::IDENTITY
    };

    assert_eq!(
        renderer.draw_projective(&command, &Rect::new(0, 0, 16, 16), &projective),
        Err(RenderError::InvalidGeometry)
    );
    assert_eq!(pixels, before);
}

#[test]
fn projective_fill_rejects_singular_geometry_before_writing() {
    let mut pixels = vec![0u8; 8 * 8 * 4];
    let before = pixels.clone();
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let command = DrawCommand::Fill {
        area: Rect::new(0, 0, 8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(255, 255, 255),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let projective = Transform3D {
        m22: crate::types::Fixed64::ZERO,
        ..Transform3D::IDENTITY
    };

    assert_eq!(
        renderer.draw_projective(&command, &Rect::new(0, 0, 8, 8), &projective),
        Err(RenderError::InvalidGeometry)
    );
    assert_eq!(pixels, before);
}

#[test]
fn projective_solid_path_flattens_into_the_software_target() {
    let mut pixels = vec![0u8; 8 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let paint = Paint::Color(Color::rgb(255, 255, 255).into());
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: crate::render::raster::FillRule::NonZero,
    };

    assert_eq!(
        renderer.draw_projective(
            &command,
            &Rect::new(0, 0, 8, 8),
            &Transform3D::translate(Fixed::from_int(2), Fixed::ONE),
        ),
        Ok(())
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(3, 2),
        Color::rgb(255, 255, 255)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(1, 2),
        Color::rgba(0, 0, 0, 0)
    );
}

#[test]
fn projective_gradient_path_samples_back_in_paint_space() {
    use alloc::borrow::Cow;
    use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};

    let mut pixels = vec![0u8; 16 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 16, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(8),
        Fixed::from_int(4),
    );
    let paint = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
        stops: Cow::Owned(vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(0, 20, 240),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(240, 20, 0),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: crate::render::raster::FillRule::NonZero,
    };
    let clip = Rect::new(0, 0, 16, 8);
    let projection = Transform3D::translate(Fixed::from_int(3), Fixed::from_int(2));

    assert_eq!(
        renderer.route(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(RenderRoute::Native)
    );
    renderer
        .draw_request(&DrawRequest::new(&command, clip).with_projective(projection))
        .unwrap();
    let left = renderer.target.read_pixels().unwrap().get_pixel(4, 3);
    let right = renderer.target.read_pixels().unwrap().get_pixel(9, 3);
    assert!(left.b > left.r);
    assert!(right.r > right.b);
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(1, 3),
        Color::rgba(0, 0, 0, 0)
    );
}

#[test]
fn projective_solid_stroke_uses_the_retained_outline() {
    let mut pixels = vec![0u8; 12 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 12, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let mut path = Path::new();
    path.move_to(Point::new(1, 2)).line_to(Point::new(7, 2));
    let paint = Paint::Color(Color::rgb(240, 120, 40).into());
    let command = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        width: Fixed::from_int(2),
        opa: 255,
        line_cap: crate::render::raster::LineCap::Butt,
        line_join: crate::render::raster::LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: &[],
    };
    let projection = Transform3D::translate(Fixed::from_int(2), Fixed::ONE);

    assert_eq!(
        renderer.draw_projective(&command, &Rect::new(0, 0, 12, 8), &projection),
        Ok(())
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(5, 3),
        Color::rgb(240, 120, 40)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(1, 3),
        Color::rgba(0, 0, 0, 0)
    );
}

#[test]
fn projective_gradient_stroke_preserves_its_color_axis() {
    use alloc::borrow::Cow;
    use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};

    let mut pixels = vec![0u8; 16 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 16, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let mut path = Path::new();
    path.move_to(Point::new(1, 3)).line_to(Point::new(9, 3));
    let paint = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
        end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
        stops: Cow::Owned(vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(10, 240, 40),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(240, 20, 180),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::ObjectBoundingBox,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        width: Fixed::from_int(2),
        opa: 255,
        line_cap: crate::render::raster::LineCap::Butt,
        line_join: crate::render::raster::LineJoin::Miter,
        miter_limit: Fixed::from_int(4),
        dash: &[],
    };
    let clip = Rect::new(0, 0, 16, 8);
    let projection = Transform3D::translate(Fixed::from_int(3), Fixed::ONE);

    renderer
        .draw_request(&DrawRequest::new(&command, clip).with_projective(projection))
        .unwrap();
    let left = renderer.target.read_pixels().unwrap().get_pixel(5, 4);
    let right = renderer.target.read_pixels().unwrap().get_pixel(10, 4);
    assert!(left.g > left.r);
    assert!(right.r > right.g);
}

#[test]
fn projective_line_uses_stroke_geometry_without_a_path_allocation() {
    let mut pixels = vec![0u8; 12 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 12, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let command = DrawCommand::Line {
        p1: Point::new(1, 2),
        p2: Point::new(7, 2),
        transform: Transform::IDENTITY,
        color: Color::rgb(80, 180, 250),
        width: Fixed::from_int(2),
        opa: 255,
    };
    let clip = Rect::new(0, 0, 12, 8);
    let projection = Transform3D::translate(Fixed::from_int(2), Fixed::ONE);

    assert_eq!(
        renderer.route(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(RenderRoute::Native)
    );
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(())
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(5, 3),
        Color::rgb(80, 180, 250)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(1, 3),
        Color::rgba(0, 0, 0, 0)
    );
}

#[test]
fn projective_arc_reuses_primitive_path_storage() {
    let mut pixels = vec![0u8; 16 * 16 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let command = DrawCommand::Arc {
        center: Point::new(6, 6),
        transform: Transform::IDENTITY,
        radius: Fixed::from_int(4),
        start_angle: Fixed::ZERO,
        end_angle: Fixed::from_int(180),
        color: Color::rgb(250, 190, 70),
        width: Fixed::from_int(2),
        opa: 255,
    };
    let clip = Rect::new(0, 0, 16, 16);
    let projection = Transform3D::translate(Fixed::ONE, Fixed::from_int(2));

    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(())
    );
    let capacity = renderer.scratch.primitive_path.command_capacity();
    assert!(capacity >= 3);
    assert!(
        renderer
            .target
            .read_pixels()
            .unwrap()
            .bytes()
            .chunks_exact(4)
            .any(|pixel| pixel[3] != 0)
    );

    renderer.target.pixels_mut().unwrap().bytes_mut().fill(0);
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip).with_projective(projection)),
        Ok(())
    );
    assert_eq!(renderer.scratch.primitive_path.command_capacity(), capacity);
}

#[test]
fn projective_clip_masks_following_draws() {
    let mut pixels = vec![0u8; 8 * 8 * 4];
    let mut renderer = SwRenderer::new(
        Texture::new(&mut pixels, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage"),
    );
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(4),
        Fixed::from_int(4),
    );
    let clip = Rect::new(0, 0, 8, 8);
    let push = DrawCommand::PushClip {
        path: &path,
        transform: Transform::translate(Fixed::ONE, Fixed::ONE),
        fill_rule: crate::render::raster::FillRule::NonZero,
    };
    let fill = DrawCommand::Fill {
        area: clip,
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(120, 180, 240),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let projection = Transform3D::translate(Fixed::ONE, Fixed::ZERO);

    renderer
        .draw_request(&DrawRequest::new(&push, clip).with_projective(projection))
        .unwrap();
    renderer
        .draw_request(&DrawRequest::new(&fill, clip))
        .unwrap();
    renderer
        .draw_request(&DrawRequest::new(&DrawCommand::PopClip, clip).with_projective(projection))
        .unwrap();

    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(3, 2),
        Color::rgb(120, 180, 240)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(1, 2),
        Color::rgba(0, 0, 0, 0)
    );
    assert_eq!(
        renderer.target.read_pixels().unwrap().get_pixel(6, 2),
        Color::rgba(0, 0, 0, 0)
    );
}
