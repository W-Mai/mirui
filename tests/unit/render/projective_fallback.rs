use super::*;
use crate::render::PosedGlyphs;
use crate::render::canvas::Paint;
use crate::render::command::CompositeMode;
use crate::render::font::Font;
use crate::render::raster::{FillRule, LineCap, LineJoin};
use crate::types::{Color, Point, Transform};
use alloc::borrow::Cow;
use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};
use textflow::placement::GlyphFrame;
use textflow::shaping::{FlowPoint, GlyphId, PositionedGlyph};

#[test]
fn stroke_fallback_preserves_affine_and_projective_raster_routes() {
    use crate::render::renderer::DrawRequest;
    const WIDTH: usize = 96;
    const HEIGHT: usize = 96;
    let transform =
        Transform::translate(16.into(), 12.into()).compose(&Transform::rotate_deg(12.into()));
    let commands = [
        DrawCommand::Line {
            p1: Point::new(4, 8),
            p2: Point::new(26, 17),
            transform,
            color: Color::rgb(200, 70, 30),
            width: 3.into(),
            opa: 171,
        },
        DrawCommand::Arc {
            center: Point::new(16, 16),
            radius: 10.into(),
            start_angle: 20.into(),
            end_angle: 240.into(),
            transform,
            color: Color::rgb(30, 70, 200),
            width: 3.into(),
            opa: 171,
        },
    ];
    for command in commands {
        for scale in [Fixed::ONE, Fixed::from_int(2)] {
            let viewport = Viewport::new(WIDTH as u16, HEIGHT as u16, scale);
            for projective in [
                Transform3D::IDENTITY,
                Transform3D::rotate_y_perspective(12.into(), 400.into()),
            ] {
                for clip in [Rect::new(0, 0, 48, 48), Rect::new(19, 17, 14, 15)] {
                    let mut expected = alloc::vec![19; WIDTH * HEIGHT * 4];
                    let mut actual = expected.clone();
                    let mut texture = Texture::new(
                        &mut expected,
                        WIDTH as u16,
                        HEIGHT as u16,
                        ColorFormat::RGBA8888,
                    )
                    .unwrap();
                    texture.alpha_mode = crate::render::texture::AlphaMode::Blend;
                    let mut direct = SwRenderer::new(texture);
                    direct.viewport = viewport;
                    direct
                        .draw_request(&DrawRequest::new(&command, clip).with_projective(projective))
                        .unwrap();
                    direct.flush().unwrap();
                    drop(direct);

                    let mut fallback = ProjectiveFallback::new(alloc::vec![0; WIDTH * HEIGHT * 4]);
                    let plan = fallback
                        .plan(&command, &clip, &projective, viewport)
                        .unwrap();
                    assert!(plan.x() > 0 && plan.y() > 0);
                    let width = usize::from(plan.width());
                    let height = usize::from(plan.height());
                    for row in 0..height {
                        let start =
                            ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
                        fallback.target_mut(plan)[row * width * 4..(row + 1) * width * 4]
                            .copy_from_slice(&actual[start..start + width * 4]);
                    }
                    fallback
                        .render(plan, &command, &projective, viewport)
                        .unwrap();
                    for row in 0..height {
                        let start =
                            ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
                        actual[start..start + width * 4].copy_from_slice(
                            &fallback.target(plan)[row * width * 4..(row + 1) * width * 4],
                        );
                    }
                    let mut max_composited = 0;
                    for (index, (a, b)) in actual
                        .chunks_exact(4)
                        .zip(expected.chunks_exact(4))
                        .enumerate()
                    {
                        let x = index % WIDTH;
                        let y = index / WIDTH;
                        let inside = x >= usize::from(plan.x())
                            && x < usize::from(plan.x()) + width
                            && y >= usize::from(plan.y())
                            && y < usize::from(plan.y()) + height;
                        if projective.is_identity() || !inside {
                            assert_eq!(a, b, "pixel {index}, scale {scale:?}, clip {clip:?}");
                        }
                        for channel in 0..4 {
                            let difference = if channel == 3 {
                                u16::from(a[3].abs_diff(b[3]))
                            } else {
                                let a = (u16::from(a[channel]) * u16::from(a[3]) + 127) / 255;
                                let b = (u16::from(b[channel]) * u16::from(b[3]) + 127) / 255;
                                a.abs_diff(b)
                            };
                            max_composited = max_composited.max(difference);
                        }
                    }
                    assert!(
                        max_composited <= 1,
                        "composited difference {max_composited}, scale {scale:?}, clip {clip:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn source_read_errors_survive_software_fallback_execution() {
    let source_bytes = [255u8; 16];
    let mut source = Texture::from_ref(&source_bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
    source.stride = 16;
    let viewport = Viewport::new(8, 8, Fixed::ONE);
    let plan = ProjectiveFallbackPlan::from_region(
        crate::render::renderer::FallbackRegion::from_parts(0, 0, 8, 8, 32),
        viewport,
        PlaneRequirements::CPU,
    )
    .unwrap();
    for quad in [
        None,
        Some([
            Point::new(0, 0),
            Point::new(4, 0),
            Point::new(4, 4),
            Point::new(0, 4),
        ]),
    ] {
        let command = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(4, 4),
            transform: Transform::IDENTITY,
            quad,
            texture: &source,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        };
        for projection in [
            Transform3D::IDENTITY,
            Transform3D::translate(Fixed::ONE, Fixed::ZERO),
        ] {
            let mut fallback = ProjectiveFallback::new(alloc::vec![0x35; 256]);
            assert_eq!(
                fallback.render(plan, &command, &projection, viewport),
                Err(RenderError::InvalidTexture)
            );
            assert!(fallback.target(plan).iter().all(|byte| *byte == 0x35));
        }
    }
}
#[test]
fn fallback_region_reconstructs_the_same_plane() {
    let region = crate::render::renderer::FallbackRegion::from_parts(3, 5, 7, 11, 32);
    let plan = ProjectiveFallbackPlan::from_region(
        region,
        Viewport::new(20, 20, Fixed::from_int(2)),
        PlaneRequirements::CPU,
    )
    .unwrap();
    assert_eq!(plan.region(), region);
    assert_eq!(plan.required_bytes(), 352);
    assert_eq!(plan.logical_origin.x, Fixed::from_f32(1.5));
    assert_eq!(plan.logical_origin.y, Fixed::from_f32(2.5));
}

#[test]
fn aligned_fallback_preserves_padded_rows_and_compacts_for_web_upload() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    let region = crate::render::renderer::FallbackRegion::rgba8(
        PhysicalRect::new(2, 3, 17, 2).unwrap(),
        requirements,
    )
    .unwrap();
    assert_eq!(region.stride_bytes(), 128);
    assert_eq!(region.required_bytes(), 256);

    let mut storage = [0u8; 320];
    let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
    let mut fallback = ProjectiveFallback::borrowed_with(
        &mut storage[offset..offset + region.required_bytes()],
        requirements,
    )
    .unwrap();
    let plan = ProjectiveFallbackPlan::from_region(
        region,
        Viewport::new(40, 20, Fixed::ONE),
        requirements,
    )
    .unwrap();
    let packed = alloc::vec![9u8; 17 * 2 * 4];
    fallback.copy_from_packed_rgba8(plan, &packed).unwrap();
    assert_eq!(&fallback.target(plan)[..68], &packed[..68]);
    assert_eq!(&fallback.target(plan)[128..196], &packed[68..]);
    assert_eq!(fallback.packed_rgba8_mut(plan).unwrap(), packed);
}

#[test]
fn aligned_fallback_rejects_a_misaligned_caller_slice() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    let mut storage = [0u8; 128];
    let offset = (64 - (storage.as_ptr() as usize % 64)) % 64;
    let misaligned = &mut storage[offset + 1..];
    assert!(matches!(
        ProjectiveFallback::borrowed_with(misaligned, requirements),
        Err(PlaneError::MisalignedAddress { alignment: 64 })
    ));
}

#[test]
fn owned_aligned_fallback_keeps_a_stable_aligned_view() {
    let requirements = PlaneRequirements::new(64, 64).unwrap();
    let fallback = ProjectiveFallback::aligned(1024, requirements).unwrap();
    assert_eq!(fallback.target.as_ref().as_ptr() as usize % 64, 0);
    assert_eq!(fallback.capacity(), 1024);
    assert_eq!(fallback.requirements(), requirements);
}

#[test]
fn identity_rounded_blit_uses_bounded_local_target() {
    let source = [255u8, 0, 0, 255].repeat(16);
    let texture =
        Texture::from_ref(&source, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let command = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::from_int(3),
        composite: CompositeMode::Screen,
    };
    let mut bytes = [0u8; 12 * 12 * 4];
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 255, 255]);
    }
    let mut fallback = ProjectiveFallback::borrowed(&mut bytes);
    let viewport = Viewport::new(12, 12, Fixed::ONE);
    let plan = fallback
        .plan(
            &command,
            &Rect::new(0, 0, 12, 12),
            &Transform3D::IDENTITY,
            viewport,
        )
        .unwrap();
    assert!(plan.required_bytes() <= fallback.capacity());
    fallback
        .render(plan, &command, &Transform3D::IDENTITY, viewport)
        .unwrap();
    let data = fallback.target(plan);
    let center =
        (usize::from(6 - plan.y()) * usize::from(plan.width()) + usize::from(6 - plan.x())) * 4;
    assert_eq!(&data[center..center + 4], &[255, 0, 255, 255]);
}

#[test]
fn fallback_measure_rejects_insufficient_capacity_before_rendering() {
    let pixels = [200u8, 62, 112, 190].repeat(4 * 4);
    let texture =
        Texture::from_ref(&pixels, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let command = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 216,
        radius: Fixed::from_int(2),
        composite: CompositeMode::Difference,
    };
    let viewport = Viewport::new(16, 16, Fixed::ONE);
    let clip = Rect::new(0, 0, 16, 16);
    let needed = match ProjectiveFallback::<&mut [u8]>::measure(
        0,
        &command,
        &clip,
        &Transform3D::IDENTITY,
        viewport,
        PlaneRequirements::CPU,
    ) {
        Err(ProjectiveDrawError::InsufficientFallbackStorage {
            required_bytes,
            capacity_bytes: 0,
        }) => required_bytes,
        other => panic!("unexpected fallback plan: {other:?}"),
    };
    assert!(needed > 0);
    let plan = ProjectiveFallback::<&mut [u8]>::measure(
        needed,
        &command,
        &clip,
        &Transform3D::IDENTITY,
        viewport,
        PlaneRequirements::CPU,
    )
    .unwrap();
    assert_eq!(plan.required_bytes(), needed);
}

#[test]
fn translated_gradient_path_renders_inside_local_fallback() {
    let path = crate::render::path::Path::rect(
        Fixed::from_int(10),
        Fixed::from_int(6),
        Fixed::from_int(20),
        Fixed::from_int(8),
    );
    let gradient = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(
            mirx::types::Fixed::from_int(10),
            mirx::types::Fixed::from_int(6),
        ),
        end: mirx::types::Point::new(
            mirx::types::Fixed::from_int(30),
            mirx::types::Fixed::from_int(6),
        ),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(255, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(0, 0, 255),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &gradient,
        opa: 255,
        fill_rule: FillRule::NonZero,
    };
    let mut bytes = [0u8; 32 * 16 * 4];
    let mut fallback = ProjectiveFallback::borrowed(&mut bytes);
    let viewport = Viewport::new(32, 16, Fixed::ONE);
    let plan = fallback
        .plan(
            &command,
            &Rect::new(0, 0, 32, 16),
            &Transform3D::IDENTITY,
            viewport,
        )
        .unwrap();
    fallback
        .render(plan, &command, &Transform3D::IDENTITY, viewport)
        .unwrap();
    let pixel = |x: i32| {
        let offset = (usize::from(10 - plan.y()) * usize::from(plan.width())
            + usize::try_from(x - i32::from(plan.x())).unwrap())
            * 4;
        &fallback.target(plan)[offset..offset + 4]
    };
    assert!(pixel(11)[0] > pixel(11)[2]);
    assert!(pixel(28)[2] > pixel(28)[0]);
    matches_software_on_its_projected_region(&command, 1);
}

#[test]
fn gradient_stroke_uses_bounded_local_fallback() {
    let path = crate::render::path::Path::rect(
        Fixed::from_int(8),
        Fixed::from_int(8),
        Fixed::from_int(16),
        Fixed::from_int(8),
    );
    let gradient = Paint::LinearGradient(LinearGradient {
        start: mirx::types::Point::new(
            mirx::types::Fixed::from_int(8),
            mirx::types::Fixed::from_int(8),
        ),
        end: mirx::types::Point::new(
            mirx::types::Fixed::from_int(24),
            mirx::types::Fixed::from_int(8),
        ),
        stops: Cow::Owned(alloc::vec![
            GradientStop {
                offset: mirx::types::Fixed::ZERO,
                color: mirx::types::Color::rgb(255, 0, 0),
            },
            GradientStop {
                offset: mirx::types::Fixed::ONE,
                color: mirx::types::Color::rgb(0, 0, 255),
            },
        ]),
        spread: SpreadMode::Pad,
        units: GradientUnits::UserSpaceOnUse,
        transform: mirx::types::Transform::IDENTITY,
    });
    let command = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &gradient,
        width: Fixed::from_int(3),
        opa: 255,
        line_cap: LineCap::Butt,
        line_join: LineJoin::Miter,
        miter_limit: Fixed::ONE,
        dash: &[],
    };
    let mut bytes = [0u8; 32 * 24 * 4];
    let mut fallback = ProjectiveFallback::borrowed(&mut bytes);
    let viewport = Viewport::new(32, 24, Fixed::ONE);
    let plan = fallback
        .plan(
            &command,
            &Rect::new(0, 0, 32, 24),
            &Transform3D::IDENTITY,
            viewport,
        )
        .unwrap();
    fallback
        .render(plan, &command, &Transform3D::IDENTITY, viewport)
        .unwrap();
    assert!(
        fallback
            .target(plan)
            .chunks_exact(4)
            .any(|pixel| pixel[3] > 0)
    );
    assert!(plan.required_bytes() < fallback.capacity());
    matches_software_on_its_projected_region(&command, 1);
}

#[test]
fn explicit_quad_fallback_uses_its_projected_bounds_and_pixels() {
    let source = [255u8, 0, 0, 255].repeat(16);
    let texture =
        Texture::from_ref(&source, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let quad = [
        Point::new(2, 2),
        Point::new(18, 2),
        Point::new(15, 18),
        Point::new(4, 18),
    ];
    let command = DrawCommand::Blit {
        pos: Point::new(0, 0),
        size: Point::new(4, 4),
        transform: Transform::IDENTITY,
        quad: Some(quad),
        texture: &texture,
        opa: 255,
        radius: Fixed::from_int(3),
        composite: CompositeMode::Screen,
    };
    let mut bytes = [0u8; 20 * 20 * 4];
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 255, 255]);
    }
    let mut fallback = ProjectiveFallback::borrowed(&mut bytes);
    let viewport = Viewport::new(20, 20, Fixed::ONE);
    let plan = fallback
        .plan(
            &command,
            &Rect::new(0, 0, 20, 20),
            &Transform3D::IDENTITY,
            viewport,
        )
        .unwrap();
    fallback
        .render(plan, &command, &Transform3D::IDENTITY, viewport)
        .unwrap();
    let data = fallback.target(plan);
    let center =
        (usize::from(10 - plan.y()) * usize::from(plan.width()) + usize::from(10 - plan.x())) * 4;
    assert_eq!(&data[center..center + 4], &[255, 0, 255, 255]);
}
fn glyph_command<'a>(glyphs: &'a [PositionedGlyph], font: &'a Font) -> DrawCommand<'a> {
    DrawCommand::GlyphRun {
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        glyphs,
        font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    }
}

#[test]
fn clip_is_the_complete_capacity_budget() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let font = Font::default();
    let command = glyph_command(&glyphs, &font);
    let fallback = ProjectiveFallback::new(alloc::vec![0; 15].into_boxed_slice());
    let transform = Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));

    assert_eq!(
        fallback.plan(
            &command,
            &Rect::new(1, 2, 2, 2),
            &transform,
            Viewport::new(8, 8, Fixed::ONE),
        ),
        Err(ProjectiveDrawError::InsufficientFallbackStorage {
            required_bytes: 16,
            capacity_bytes: 15,
        })
    );
}

#[test]
fn borrowed_target_uses_caller_storage_without_growing() {
    let mut bytes = [0u8; 256];
    let mut fallback = ProjectiveFallback::borrowed(&mut bytes);
    let font = Font::default();
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let command = glyph_command(&glyphs, &font);
    let clip = Rect::new(0, 0, 8, 8);
    let transform = Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));
    let viewport = Viewport::new(8, 8, Fixed::ONE);
    let plan = fallback
        .plan(&command, &clip, &transform, viewport)
        .unwrap();
    fallback
        .render(plan, &command, &transform, viewport)
        .unwrap();
    assert_eq!(fallback.capacity(), 256);
    assert!(bytes.iter().any(|byte| *byte != 0));
}

#[test]
fn projected_glyph_uses_ink_bounds_instead_of_full_clip() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let font = Font::default();
    let command = DrawCommand::GlyphRun {
        pos: Point::new(10, 20),
        transform: Transform::IDENTITY,
        glyphs: &glyphs,
        font: &font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    };
    let fallback = ProjectiveFallback::new(alloc::vec![0; 1024]);
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(10), Fixed::from_int(400));
    let plan = fallback
        .plan(
            &command,
            &Rect::new(0, 0, 64, 64),
            &projection,
            Viewport::new(64, 64, Fixed::ONE),
        )
        .expect("a small glyph should fit in a 1 KiB fallback target");
    assert!(plan.required_bytes() <= 1024);
    assert!(plan.width() < 64);
    assert!(plan.height() < 64);
}

#[test]
fn rendering_reuses_seeded_target_without_growth() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 8 << 8 },
    )];
    let font = Font::default();
    let command = glyph_command(&glyphs, &font);
    let mut fallback = ProjectiveFallback::new(alloc::vec![7; 8 * 8 * 4]);
    let viewport = Viewport::new(8, 8, Fixed::ONE);
    let transform = Transform3D::rotate_y_perspective(Fixed::from_int(5), Fixed::from_int(400));
    let plan = fallback
        .plan(&command, &Rect::new(0, 0, 8, 8), &transform, viewport)
        .unwrap();
    let capacity = fallback.capacity();
    fallback.target_mut(plan).fill(7);

    fallback
        .render(plan, &command, &transform, viewport)
        .unwrap();

    assert_eq!(fallback.capacity(), capacity);
    assert!(fallback.target(plan).iter().any(|byte| *byte != 7));
}

#[test]
fn projected_alpha_keeps_straight_color_on_a_transparent_target() {
    let command = DrawCommand::Fill {
        area: Rect::new(2, 2, 8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgba(255, 0, 0, 128),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let viewport = Viewport::new(16, 16, Fixed::ONE);
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(8), Fixed::from_int(400));
    let mut fallback = ProjectiveFallback::new(alloc::vec![0; 16 * 16 * 4]);
    let plan = fallback
        .plan(&command, &Rect::new(0, 0, 16, 16), &projection, viewport)
        .unwrap();
    fallback.target_mut(plan).fill(0);
    fallback
        .render(plan, &command, &projection, viewport)
        .unwrap();
    assert!(fallback.target(plan).chunks_exact(4).any(|pixel| {
        pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0 && (1..255).contains(&pixel[3])
    }));
}

#[test]
fn clipped_target_matches_the_same_region_of_the_software_backend() {
    const WIDTH: usize = 16;
    const HEIGHT: usize = 12;
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint {
            x: 4 << 8,
            y: 9 << 8,
        },
    )];
    let font = Font::default();
    let command = glyph_command(&glyphs, &font);
    let viewport = Viewport::new(WIDTH as u16, HEIGHT as u16, Fixed::ONE);
    let clip = Rect::new(3, 2, 9, 8);
    let transform = Transform3D::rotate_y_perspective(Fixed::from_int(8), Fixed::from_int(400));
    let mut expected = alloc::vec![19; WIDTH * HEIGHT * 4];
    let mut actual = expected.clone();

    let mut expected_target = Texture::new(
        &mut expected,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    expected_target.alpha_mode = crate::render::texture::AlphaMode::Blend;
    let mut direct = SwRenderer::new(expected_target);
    direct.viewport = viewport;
    direct.draw_projective(&command, &clip, &transform).unwrap();

    let mut fallback = ProjectiveFallback::new(alloc::vec![0; 9 * 8 * 4]);
    let plan = fallback
        .plan(&command, &clip, &transform, viewport)
        .unwrap();
    let width = usize::from(plan.width());
    let height = usize::from(plan.height());
    let target = fallback.target_mut(plan);
    for row in 0..height {
        let source = ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
        let offset = row * width * 4;
        target[offset..offset + width * 4].copy_from_slice(&actual[source..source + width * 4]);
    }
    fallback
        .render(plan, &command, &transform, viewport)
        .unwrap();
    let target = fallback.target(plan);
    for row in 0..height {
        let dest = ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
        let offset = row * width * 4;
        actual[dest..dest + width * 4].copy_from_slice(&target[offset..offset + width * 4]);
    }
    if let Some(index) = actual.iter().zip(&expected).position(|(a, b)| a != b) {
        panic!(
            "pixel mismatch at byte {index}: actual {}, expected {}, plan {plan:?}",
            actual[index], expected[index]
        );
    }
}

fn matches_software_on_its_projected_region(command: &DrawCommand<'_>, tolerance: u8) {
    const WIDTH: usize = 64;
    const HEIGHT: usize = 64;
    let viewport = Viewport::new(WIDTH as u16, HEIGHT as u16, Fixed::ONE);
    let clip = Rect::new(0, 0, WIDTH as i32, HEIGHT as i32);
    let transform = Transform3D::rotate_y_perspective(Fixed::from_int(12), Fixed::from_int(400));
    let mut expected = alloc::vec![19; WIDTH * HEIGHT * 4];
    let mut actual = expected.clone();
    let mut expected_target = Texture::new(
        &mut expected,
        WIDTH as u16,
        HEIGHT as u16,
        ColorFormat::RGBA8888,
    )
    .expect("valid pixel storage");
    expected_target.alpha_mode = crate::render::texture::AlphaMode::Blend;
    let mut direct = SwRenderer::new(expected_target);
    direct.viewport = viewport;
    direct.draw_projective(command, &clip, &transform).unwrap();

    let mut fallback = ProjectiveFallback::new(alloc::vec![0; 32 * 32 * 4]);
    let plan = fallback.plan(command, &clip, &transform, viewport).unwrap();
    assert!(plan.width() < WIDTH as u16);
    assert!(plan.height() < HEIGHT as u16);
    let width = usize::from(plan.width());
    let height = usize::from(plan.height());
    let target = fallback.target_mut(plan);
    for row in 0..height {
        let source = ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
        let offset = row * width * 4;
        target[offset..offset + width * 4].copy_from_slice(&actual[source..source + width * 4]);
    }
    fallback
        .render(plan, command, &transform, viewport)
        .unwrap();
    let target = fallback.target(plan);
    for row in 0..height {
        let dest = ((usize::from(plan.y()) + row) * WIDTH + usize::from(plan.x())) * 4;
        let offset = row * width * 4;
        actual[dest..dest + width * 4].copy_from_slice(&target[offset..offset + width * 4]);
    }
    let mut max_inside = 0;
    for (index, (actual_byte, expected_byte)) in actual.iter().zip(&expected).enumerate() {
        let pixel = index / 4;
        let x = pixel % WIDTH;
        let y = pixel / WIDTH;
        let inside = x >= usize::from(plan.x())
            && x < usize::from(plan.x()) + width
            && y >= usize::from(plan.y())
            && y < usize::from(plan.y()) + height;
        let difference = actual_byte.abs_diff(*expected_byte);
        if inside {
            let composited_difference = if index % 4 == 3 {
                difference
            } else {
                let pixel_offset = index / 4 * 4;
                let actual_premul =
                    (u16::from(*actual_byte) * u16::from(actual[pixel_offset + 3]) + 127) / 255;
                let expected_premul =
                    (u16::from(*expected_byte) * u16::from(expected[pixel_offset + 3]) + 127) / 255;
                actual_premul.abs_diff(expected_premul) as u8
            };
            max_inside = max_inside.max(composited_difference);
        } else {
            assert_eq!(difference, 0, "outside plan at byte {index}: {plan:?}");
        }
    }
    assert!(
        max_inside <= tolerance,
        "maximum composited pixel difference {max_inside}: {plan:?}"
    );
}

#[test]
fn projected_shapes_fit_a_tight_budget_and_match_software() {
    let fill = DrawCommand::Fill {
        area: Rect::new(12, 12, 20, 18),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(12, 180, 220),
        radius: Fixed::from_int(3),
        opa: 255,
    };
    matches_software_on_its_projected_region(&fill, 0);

    let border = DrawCommand::Border {
        area: Rect::new(12, 12, 20, 18),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(240, 50, 80),
        width: Fixed::from_int(2),
        radius: Fixed::from_int(3),
        opa: 255,
    };
    matches_software_on_its_projected_region(&border, 0);

    let mut pixels = [160; 8 * 8 * 4];
    let texture =
        Texture::new(&mut pixels, 8, 8, ColorFormat::RGBA8888).expect("valid pixel storage");
    let blit = DrawCommand::Blit {
        pos: Point::new(12, 12),
        size: Point::new(20, 18),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    matches_software_on_its_projected_region(&blit, 0);
}

#[test]
fn projected_glyph_bounds_preserve_full_software_output() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let font = Font::default();
    let command = DrawCommand::GlyphRun {
        pos: Point::new(20, 25),
        transform: Transform::rotate_deg(Fixed::from_int(8)),
        glyphs: &glyphs,
        font: &font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    };
    matches_software_on_its_projected_region(&command, 5);
}

#[test]
fn projected_posed_glyph_bounds_preserve_full_software_output() {
    let glyphs = [PositionedGlyph::new(
        GlyphId::new(u16::from(b'A')),
        FlowPoint { x: 0, y: 0 },
    )];
    let frames = [GlyphFrame {
        local_origin: FlowPoint {
            x: 20 << 8,
            y: 20 << 8,
        },
        unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
    }];
    let font = Font::default();
    let command = DrawCommand::PosedGlyphRun {
        pos: Point::ZERO,
        transform: Transform::IDENTITY,
        glyphs: PosedGlyphs::new(&glyphs, &frames).unwrap(),
        font: &font,
        color: Color::rgb(255, 255, 255),
        opa: 255,
    };
    matches_software_on_its_projected_region(&command, 4);
}

#[test]
fn card_budget_uses_projected_pixels_not_the_full_viewport() {
    let fill = DrawCommand::Fill {
        area: Rect::new(140, 70, 200, 180),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(88, 166, 255),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let fallback = ProjectiveFallback::new(alloc::vec![0; 512 * 160 * 4]);
    let plan = fallback
        .plan(
            &fill,
            &Rect::new(0, 0, 480, 320),
            &Transform3D::rotate_y_perspective(Fixed::from_int(8), Fixed::from_int(400)),
            Viewport::new(480, 320, Fixed::ONE),
        )
        .unwrap();
    assert!(plan.width() > 0 && plan.height() > 0);
    assert!(plan.required_bytes() <= fallback.capacity());
}

#[test]
fn projective_image_survives_a_full_y_rotation() {
    let mut pixels = alloc::vec![255; 16 * 16 * 4];
    let texture =
        Texture::new(&mut pixels, 16, 16, ColorFormat::RGBA8888).expect("valid pixel storage");
    let command = DrawCommand::Blit {
        pos: Point::new(180, 180),
        size: Point::new(120, 120),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    let mut fallback = ProjectiveFallback::new(alloc::vec![0; 512 * 160 * 4]);
    let clip = Rect::new(0, 0, 480, 320);
    let viewport = Viewport::new(960, 640, Fixed::from_int(2));
    let mut rendered = 0;
    for angle in 0..360 {
        let center = Transform3D::translate(Fixed::from_int(240), Fixed::from_int(240));
        let origin = Transform3D::translate(Fixed::from_int(-240), Fixed::from_int(-240));
        let phase = Fixed::from_int(angle % 180) / Fixed::from_int(180);
        let two_t_minus_one = phase * Fixed::from_int(2) - Fixed::ONE;
        let height = Fixed::ONE - two_t_minus_one * two_t_minus_one;
        let bounce =
            Transform3D::translate(Fixed::ZERO, Fixed::ZERO - height * Fixed::from_int(100));
        let scale = Transform3D::scale(
            Fixed::ONE - (Fixed::ONE - height) / Fixed::from_int(4),
            Fixed::ONE + height / Fixed::from_int(8),
        );
        let projective = center
            .compose(&bounce)
            .compose(&Transform3D::rotate_y_perspective(
                Fixed::from_int(angle),
                Fixed::from_int(400),
            ))
            .compose(&scale)
            .compose(&origin);
        if let Ok(plan) = fallback.plan(&command, &clip, &projective, viewport) {
            fallback.target_mut(plan).fill(0);
            fallback
                .render(plan, &command, &projective, viewport)
                .unwrap();
            rendered += 1;
        }
    }
    assert!(rendered > 340, "rendered {rendered} angles");
}
