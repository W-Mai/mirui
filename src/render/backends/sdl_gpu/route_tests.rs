use super::*;
use crate::render::raster::FillRule;

#[test]
fn target_workspace_admission_accounts_for_full_plane_alignment_without_allocating() {
    let layout = crate::render::PlaneLayout::packed(3, 2, ColorFormat::RGBA8888).unwrap();
    assert_eq!(
        check_target_workspace::<&mut [u8]>(None, layout),
        Err(RenderError::MissingWorkspace)
    );
    let requirements = PlaneRequirements::new(1, 16).unwrap();
    let mut short = [0u8; 31];
    let fallback = ProjectiveFallback::borrowed_with(&mut short, requirements).unwrap();
    assert_eq!(
        check_target_workspace(Some(&fallback), layout),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 32,
            capacity_bytes: 31
        })
    );
    let mut bytes = [0u8; 32];
    let fallback = ProjectiveFallback::borrowed_with(&mut bytes, requirements).unwrap();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for _ in 0..16 {
                assert_eq!(check_target_workspace(Some(&fallback), layout), Ok(()));
            }
        }),
        0
    );
}

#[test]
fn axis_blit_fallback_preserves_rounded_and_composite_requests() {
    let pixels = [255u8; 4 * 4 * 4];
    let texture =
        Texture::from_ref(&pixels, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    for radius in [Fixed::ZERO, Fixed::from_int(3)] {
        let command = DrawCommand::Blit {
            pos: Point::new(2, 2),
            size: Point::new(12, 12),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 200,
            radius,
            composite: CompositeMode::Screen,
        };
        let request = DrawRequest::new(&command, Rect::new(0, 0, 20, 20));
        assert!(SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
            &request, false
        ));
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::classify_request(&request),
            Ok(())
        );
    }
    let ordinary = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(12, 12),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    assert!(!SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
        &DrawRequest::new(&ordinary, Rect::new(0, 0, 20, 20)),
        false
    ));
    let rotated = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(12, 12),
        transform: Transform::rotate_deg(Fixed::from_int(20)),
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    assert!(SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
        &DrawRequest::new(&rotated, Rect::new(0, 0, 20, 20)),
        false
    ));
}

#[test]
fn non_affine_explicit_quad_uses_exact_fallback() {
    let pixels = [255u8; 4 * 4 * 4];
    let texture =
        Texture::from_ref(&pixels, 4, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
    let quad = [
        Point::new(2, 2),
        Point::new(18, 2),
        Point::new(15, 18),
        Point::new(4, 18),
    ];
    let command = DrawCommand::Blit {
        pos: Point::new(2, 2),
        size: Point::new(16, 16),
        transform: Transform::IDENTITY,
        quad: Some(quad),
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    let request = DrawRequest::new(&command, Rect::new(0, 0, 20, 20));
    assert!(SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
        &request, false
    ));
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&request),
        Ok(())
    );
}

#[test]
fn solid_stroke_with_full_style_has_a_native_route() {
    let path = Path::rect(
        Fixed::ZERO,
        Fixed::ZERO,
        Fixed::from_int(16),
        Fixed::from_int(12),
    );
    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let dash = [Fixed::from_int(3), Fixed::from_int(2)];
    let stroke = DrawCommand::StrokePath {
        path: &path,
        transform: Transform::rotate_deg(Fixed::from_int(20)),
        paint: &paint,
        width: Fixed::from_int(2),
        opa: 255,
        line_cap: crate::render::raster::LineCap::Round,
        line_join: crate::render::raster::LineJoin::Bevel,
        miter_limit: Fixed::from_int(4),
        dash: &dash,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(
            &stroke,
            Rect::new(0, 0, 32, 32),
        )),
        Ok(())
    );
}

#[test]
fn borrowed_fallback_factory_keeps_the_caller_budget() {
    let mut bytes = [0u8; 256];
    let factory =
        SdlGpuFactory::new().with_projective_fallback(ProjectiveFallback::borrowed(&mut bytes));
    assert_eq!(factory.projective_fallback.unwrap().capacity(), 256);
}

#[test]
fn factory_configuration_retains_geometry_and_trim_releases_it() {
    let mut factory = SdlGpuFactory::new().with_texture_cache_budget(1234);
    let path = Path::rect(0.into(), 0.into(), 10.into(), 6.into());
    let color = Color::rgb(20, 80, 120);
    factory
        .tessellator
        .fill(path.commands(), None, &color, 255, FillRule::NonZero)
        .unwrap();
    let vertices = factory.tessellator.verts.as_ptr();
    let indices = factory.tessellator.indices.as_ptr();
    let counts = (
        factory.tessellator.verts.len(),
        factory.tessellator.indices.len(),
    );
    assert!(counts.0 > 0 && counts.1 > 0);
    let mut bytes = [0u8; 256];
    let mut configured = None;
    let allocations = crate::test_support::tracked_allocations(|| {
        configured =
            Some(factory.with_projective_fallback(ProjectiveFallback::borrowed(&mut bytes)));
    });
    assert_eq!(allocations, 0);
    let mut factory = configured.unwrap();
    assert_eq!(factory.tessellator.verts.as_ptr(), vertices);
    assert_eq!(factory.tessellator.indices.as_ptr(), indices);
    assert_eq!(factory.texture_cache_budget, 1234);
    let geometry_budget = factory.tessellator.geometry.budget;
    assert!(factory.tessellator.geometry.inspect().cache_len() > 0);
    RendererFactory::<SdlGpuSurface>::trim_memory(&mut factory);
    assert_eq!(factory.tessellator.geometry.budget, geometry_budget);
    assert_eq!(factory.tessellator.geometry.inspect().cache_len(), 0);
    assert_eq!(factory.tessellator.verts.capacity(), 0);
    assert_eq!(factory.tessellator.indices.capacity(), 0);
    assert_eq!(
        factory.projective_fallback.as_ref().unwrap().capacity(),
        256
    );
    assert_eq!(factory.texture_cache_budget, 1234);
    factory
        .tessellator
        .fill(path.commands(), None, &color, 255, FillRule::NonZero)
        .unwrap();
    assert_eq!(
        (
            factory.tessellator.verts.len(),
            factory.tessellator.indices.len()
        ),
        counts
    );
}

#[test]
fn classifies_sdl_gpu_commands_and_texture_storage() {
    let clip = Rect::new(0, 0, 16, 16);
    let path = Path::new();
    let push = DrawCommand::PushClip {
        path: &path,
        transform: Transform::IDENTITY,
        fill_rule: crate::render::raster::FillRule::EvenOdd,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&push, clip)),
        Err(RenderError::Unsupported(RenderFeature::PathClip))
    );

    let paint = Paint::Color(Color::rgb(20, 30, 40).into());
    let fill = DrawCommand::FillPath {
        path: &path,
        transform: Transform::IDENTITY,
        paint: &paint,
        opa: 255,
        fill_rule: crate::render::raster::FillRule::NonZero,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&fill, clip)),
        Ok(())
    );

    let texture = Texture::owned(2, 2, ColorFormat::RGB565Swapped).expect("valid pixel storage");
    let blit = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: Transform::IDENTITY,
        quad: None,
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&blit, clip)),
        Ok(())
    );

    assert!(matches!(
        Texture::from_ref(&[0u8; 1], 2, 2, ColorFormat::RGBA8888),
        Err(RenderError::InvalidTexture)
    ));
}

#[test]
fn distinguishes_sdl_gpu_quad_composite_and_affine_support() {
    let clip = Rect::new(0, 0, 16, 16);
    let texture = Texture::owned(2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    let quad = [
        Point::ZERO,
        Point::new(2, 0),
        Point::new(2, 2),
        Point::new(0, 2),
    ];
    let blit = DrawCommand::Blit {
        pos: Point::ZERO,
        size: Point::new(2, 2),
        transform: Transform::IDENTITY,
        quad: Some(quad),
        texture: &texture,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::Add,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&blit, clip)),
        Ok(())
    );
    assert!(SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
        &DrawRequest::new(&blit, clip),
        false
    ));

    let line = DrawCommand::Line {
        p1: Point::ZERO,
        p2: Point::new(10, 10),
        transform: Transform::rotate_deg(Fixed::from_int(20)),
        color: Color::rgb(20, 30, 40),
        width: Fixed::ONE,
        opa: 255,
    };
    assert_eq!(
        SdlGpuRenderer::<Box<[u8]>>::classify_request(&DrawRequest::new(&line, clip)),
        Ok(())
    );
    assert!(crate::render::projective_fallback::transformed_stroke(
        &DrawRequest::new(&line, clip)
    ));
}

#[test]
fn transparent_targets_use_straight_alpha_fallback_for_composite_modes() {
    let texture = Texture::from_static(&[255; 4], 1, 1, ColorFormat::RGBA8888).unwrap();
    for composite in [
        CompositeMode::SourceOver,
        CompositeMode::Add,
        CompositeMode::Multiply,
        CompositeMode::Screen,
        CompositeMode::Darken,
        CompositeMode::Lighten,
        CompositeMode::Difference,
    ] {
        let command = DrawCommand::Blit {
            pos: Point::ZERO,
            size: Point::new(4, 4),
            transform: Transform::IDENTITY,
            quad: None,
            texture: &texture,
            opa: 128,
            radius: Fixed::ZERO,
            composite,
        };
        assert_eq!(
            SdlGpuRenderer::<Box<[u8]>>::needs_blit_fallback(
                &DrawRequest::new(&command, Rect::new(0, 0, 4, 4)),
                true
            ),
            composite != CompositeMode::SourceOver,
        );
    }
}
