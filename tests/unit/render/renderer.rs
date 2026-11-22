use super::*;

#[test]
fn target_fallback_budget_uses_physical_rgba_extent() {
    assert_eq!(
        RenderRoute::target_readback(
            PhysicalRect::new(1, 2, 3, 4),
            Some(48),
            PlaneRequirements::CPU,
        ),
        Ok(RenderRoute::ExactFallback(FallbackRegion::from_parts(
            1, 2, 3, 4, 12
        )))
    );
    assert_eq!(
        RenderRoute::target_readback(None, None, PlaneRequirements::CPU),
        Ok(RenderRoute::ExactFallback(FallbackRegion::EMPTY))
    );
    assert_eq!(
        RenderRoute::target_readback(
            PhysicalRect::new(1, 2, 3, 4),
            Some(47),
            PlaneRequirements::CPU,
        ),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 48,
            capacity_bytes: 47,
        })
    );
    assert_eq!(
        RenderRoute::target_readback(PhysicalRect::new(1, 2, 3, 4), None, PlaneRequirements::CPU,),
        Err(RenderError::MissingWorkspace)
    );
}

#[test]
fn physical_fallback_route_stays_compact() {
    assert_eq!(core::mem::size_of::<PhysicalRect>(), 8);
    let word = core::mem::size_of::<usize>();
    assert_eq!(core::mem::size_of::<FallbackRegion>(), 8 + word);
    assert!(core::mem::size_of::<RenderRoute>() <= 16 + word);
}

#[test]
fn scope_plan_clips_hidpi_bounds_before_charging_capacity() {
    let viewport = Viewport::new(20, 12, Fixed::from_int(2));
    assert_eq!(
        FallbackRegion::from_logical_bounds(Rect::new(30, 30, 5, 5), viewport, None),
        Ok(FallbackRegion::EMPTY)
    );
    assert_eq!(
        FallbackRegion::from_logical_bounds(Rect::new(-2, 1, 8, 8), viewport, Some(12 * 10 * 4),),
        Ok(FallbackRegion::from_parts(0, 2, 12, 10, 48))
    );
    assert_eq!(
        FallbackRegion::from_logical_bounds(
            Rect::new(-2, 1, 8, 8),
            viewport,
            Some(12 * 10 * 4 - 1),
        ),
        Err(RenderError::InsufficientWorkspace {
            required_bytes: 12 * 10 * 4,
            capacity_bytes: 12 * 10 * 4 - 1,
        })
    );
}

#[test]
fn target_io_failures_do_not_masquerade_as_unsupported_geometry() {
    assert_eq!(
        RenderError::from(ProjectiveDrawError::BackendFailure),
        RenderError::BackendFailure
    );
}

#[test]
fn clipped_readback_preserves_pixel_rows_and_destination_offset() {
    let mut pixels = [0u8; 4 * 3 * 4];
    let mut dst =
        Texture::new(&mut pixels, 4, 3, ColorFormat::RGBA8888).expect("valid pixel storage");
    let source = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    copy_packed_rgba8(
        &source,
        PhysicalRect::new(1, 1, 2, 2).unwrap(),
        (0, 0),
        &mut dst,
    )
    .unwrap();
    assert_eq!(&pixels[..20], &[0; 20]);
    assert_eq!(&pixels[20..28], &source[..8]);
    assert_eq!(&pixels[36..44], &source[8..]);
    assert_eq!(&pixels[44..], &[0; 4]);
}

#[test]
fn direct_readback_borrows_only_the_clipped_destination_without_allocating() {
    for (origin, expected_area, destination_offset) in [
        ((0, -1), (2, 2), (1, 1)),
        ((-1, -1), (1, 2), (2, 1)),
        ((-1, -2), (1, 1), (2, 2)),
        ((-3, 0), (0, 0), (4, 0)),
    ] {
        let mut bytes = [0xa5; 44];
        let base = bytes.as_ptr();
        let mut dst = Texture::new(&mut bytes, 3, 3, ColorFormat::RGBA8888)
            .unwrap()
            .with_stride(16)
            .unwrap()
            .with_cache_revision(10);
        let mut calls = 0;
        let allocations = crate::test_support::tracked_allocations(|| {
            with_rgba8_readback(
                PhysicalRect::new(1, 0, 2, 2).unwrap(),
                origin,
                &mut dst,
                |area, pixels| {
                    calls += 1;
                    assert_eq!((area.width(), area.height()), expected_area);
                    assert_eq!((area.x(), area.y()), (1, 0));
                    assert_eq!(pixels.stride(), 16);
                    assert_eq!(
                        pixels.bytes().as_ptr(),
                        base.wrapping_add(destination_offset.1 * 16 + destination_offset.0 * 4)
                    );
                    for y in 0..i32::from(pixels.height()) {
                        for x in 0..i32::from(pixels.width()) {
                            pixels.set_pixel(x, y, &crate::types::Color::rgba(11, 22, 33, 44));
                        }
                    }
                    Ok(())
                },
            )
            .unwrap();
        });
        assert_eq!(allocations, 0);
        assert_eq!(calls, usize::from(expected_area.0 != 0));
        assert_eq!(dst.cache_revision, 10 + calls as u64);
        let pixels = dst.read_pixels().unwrap();
        for y in 0..3 {
            for x in 0..3 {
                let covered = x >= destination_offset.0
                    && x < destination_offset.0 + usize::from(expected_area.0)
                    && y >= destination_offset.1
                    && y < destination_offset.1 + usize::from(expected_area.1);
                assert_eq!(
                    &pixels.bytes()[y * 16 + x * 4..][..4],
                    if covered {
                        &[11, 22, 33, 44]
                    } else {
                        &[0xa5; 4]
                    }
                );
            }
        }
        assert_eq!(&pixels.bytes()[12..16], &[0xa5; 4]);
        assert_eq!(&pixels.bytes()[28..32], &[0xa5; 4]);
    }
}

#[test]
fn direct_readback_validates_before_io_and_invalidates_failed_writes() {
    let area = PhysicalRect::from_size(1, 1);
    let mut dst = Texture::owned(2, 2, ColorFormat::RGBA8888).unwrap();
    for origin in [(i32::MIN, 0), (0, i32::MIN), (1, 0), (0, 1)] {
        assert_eq!(
            with_rgba8_readback(area, origin, &mut dst, |_, _| panic!("unexpected read")),
            Err(RenderError::InvalidGeometry)
        );
        assert_eq!(dst.cache_revision, 0);
    }
    let mut readonly = Texture::from_ref(&[0; 16], 2, 2, ColorFormat::RGBA8888).unwrap();
    let mut rgb = Texture::owned(2, 2, ColorFormat::RGB888).unwrap();
    for invalid in [&mut readonly, &mut rgb] {
        assert_eq!(
            with_rgba8_readback(area, (0, 0), invalid, |_, _| panic!("unexpected read")),
            Err(RenderError::InvalidTexture)
        );
    }
    assert_eq!(
        with_rgba8_readback(area, (0, 0), &mut dst, |_, pixels| {
            pixels.bytes_mut()[0] = 7;
            Err(RenderError::BackendFailure)
        }),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(dst.cache_revision, 1);
    assert_eq!(dst.read_pixels().unwrap().bytes()[0], 7);
}

#[test]
fn readback_writes_advance_revision_and_reject_readonly_storage() {
    let bytes = [7u8; 16];
    let region = PhysicalRect::new(0, 0, 2, 2).unwrap();
    let mut owned = Texture::owned(2, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_cache_revision(7);
    let mut readonly = Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap();
    let allocations = crate::test_support::tracked_allocations(|| {
        copy_packed_rgba8(&bytes, region, (0, 0), &mut owned).unwrap();
        assert_eq!(owned.cache_revision, 8);
        assert_eq!(
            copy_packed_rgba8(&bytes[..4], region, (0, 0), &mut owned),
            Err(RenderError::BackendFailure)
        );
        assert_eq!(owned.cache_revision, 8);
        assert_eq!(
            copy_packed_rgba8(&bytes, region, (0, 0), &mut readonly),
            Err(RenderError::InvalidTexture)
        );
        assert_eq!(readonly.cache_revision, 0);
        assert_eq!(
            readonly.read_pixels().unwrap().bytes().as_ptr(),
            bytes.as_ptr()
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn short_readback_fails_before_modifying_destination() {
    let mut pixels = [23u8; 16];
    let mut dst =
        Texture::new(&mut pixels, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
    assert_eq!(
        copy_packed_rgba8(
            &[1, 2, 3, 4],
            PhysicalRect::new(0, 0, 2, 2).unwrap(),
            (0, 0),
            &mut dst,
        ),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(pixels, [23; 16]);
}

#[test]
fn unsupported_scroll_returns_a_typed_error() {
    struct NoScroll;

    impl Renderer for NoScroll {
        type TargetState = ();

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request).map(|_| ())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut renderer = NoScroll;
    assert_eq!(
        renderer.scroll_target_region(&Rect::new(0, 0, 8, 8), Fixed::ONE, Fixed::ZERO),
        Err(RenderError::Unsupported(RenderFeature::ScrollBlit))
    );
}

#[test]
fn unsupported_target_edit_does_not_run_callback() {
    struct NoTargetEdit;

    impl Renderer for NoTargetEdit {
        type TargetState = ();

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request).map(|_| ())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut renderer = NoTargetEdit;
    assert_eq!(
        renderer.modify_target_region(&Rect::new(0, 0, 8, 8), &mut |_| {
            panic!("unsupported edit must not run its callback")
        }),
        Err(RenderError::Unsupported(RenderFeature::Readback))
    );
}

#[test]
fn software_scope_rejects_readonly_target_before_running_body() {
    struct BorrowedTarget<'a>(Texture<'a>);
    impl Renderer for BorrowedTarget<'_> {
        type TargetState = ();

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }
        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
        fn modify_target_region(
            &mut self,
            _: &Rect,
            body: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
        ) -> Result<bool, RenderError> {
            body(&mut self.0)?;
            Ok(true)
        }
    }
    let bytes = [0u8; 16];
    let mut target =
        BorrowedTarget(Texture::from_ref(&bytes, 2, 2, ColorFormat::RGBA8888).unwrap());
    assert_eq!(
        target.render_scope(FallbackRegion::from_parts(0, 0, 2, 2, 8), &mut |_| {
            panic!("read-only scope body");
        }),
        Err(RenderError::InvalidTexture)
    );
    assert_eq!(target.0.cache_revision, 0);
    assert_eq!(
        target.0.read_pixels().unwrap().bytes().as_ptr(),
        bytes.as_ptr()
    );
}

#[test]
fn scope_replay_preserves_global_coordinates() {
    let mut pixels = [0u8; 12 * 10 * 4];
    let texture =
        Texture::new(&mut pixels, 12, 10, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
    let region = FallbackRegion::from_parts(4, 3, 4, 3, 16);
    let command = DrawCommand::Fill {
        area: Rect::new(4, 3, 4, 3),
        transform: crate::types::Transform::IDENTITY,
        quad: None,
        color: crate::types::Color::rgb(20, 80, 140),
        radius: Fixed::ZERO,
        opa: 255,
    };

    let rendered = renderer
        .render_scope(region, &mut |local| {
            local.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 12, 10)))
        })
        .unwrap();

    assert!(rendered);
    assert_eq!(
        &pixels[(3 * 12 + 4) * 4..(3 * 12 + 4) * 4 + 4],
        &[20, 80, 140, 255]
    );
    assert_eq!(
        &pixels[(5 * 12 + 7) * 4..(5 * 12 + 7) * 4 + 4],
        &[20, 80, 140, 255]
    );
    assert_eq!(&pixels[(3 * 12 + 3) * 4..(3 * 12 + 3) * 4 + 4], &[0; 4]);
    assert_eq!(&pixels[(6 * 12 + 4) * 4..(6 * 12 + 4) * 4 + 4], &[0; 4]);
}

#[test]
fn scope_replay_rebases_explicit_quads_once() {
    let mut pixels = [0u8; 12 * 10 * 4];
    let texture =
        Texture::new(&mut pixels, 12, 10, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = crate::render::backends::sw::SwRenderer::new(texture);
    let region = FallbackRegion::from_parts(4, 3, 4, 3, 16);
    let command = DrawCommand::Fill {
        area: Rect::new(4, 3, 4, 3),
        transform: crate::types::Transform::IDENTITY,
        quad: Some([
            crate::types::Point::new(4, 3),
            crate::types::Point::new(8, 3),
            crate::types::Point::new(8, 6),
            crate::types::Point::new(4, 6),
        ]),
        color: crate::types::Color::rgb(200, 70, 30),
        radius: Fixed::ZERO,
        opa: 255,
    };

    renderer
        .render_scope(region, &mut |local| {
            local.draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 12, 10)))
        })
        .unwrap();

    assert_eq!(
        &pixels[(4 * 12 + 5) * 4..(4 * 12 + 5) * 4 + 4],
        &[200, 70, 30, 255]
    );
    assert_eq!(&pixels[(4 * 12 + 3) * 4..(4 * 12 + 3) * 4 + 4], &[0; 4]);
}

#[test]
fn request_keeps_command_data_borrow_independent_of_command_borrow() {
    let clip = Rect::new(0, 0, 12, 12);
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400));
    let request = DrawRequest::new(&command, clip).with_projective(projection);
    assert_eq!(request.clip, clip);
    assert_eq!(request.projective, projection);
    assert!(matches!(request.command, DrawCommand::ApplyBlur { .. }));
}

#[test]
fn region_forwards_text_run_identity_and_rebased_request() {
    #[derive(Default)]
    struct Recorder {
        seen: Option<(TextRunIdentity, Rect, Transform3D, Rect)>,
    }

    impl Renderer for Recorder {
        type TargetState = ();

        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            Ok(RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn submit_text_run(
            &mut self,
            draw: &::mirui::render::PreparedDraw<'_>,
            identity: TextRunIdentity,
        ) -> Result<(), RenderError> {
            let request = &draw.request();
            let DrawCommand::ApplyBlur { region, .. } = request.command else {
                panic!("test command changed");
            };
            self.seen = Some((identity, request.clip, request.projective, *region));
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let identity = TextRunIdentity {
        entity: Entity {
            id: 7,
            generation: 3,
        },
        run_index: 2,
    };
    let clip = Rect::new(3, 4, 8, 9);
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    let mut recorder = Recorder::default();
    RegionRenderer::new(&mut recorder, (3, 4))
        .draw_text_run(&DrawRequest::new(&command, clip), identity)
        .unwrap();
    assert_eq!(
        recorder.seen,
        Some((
            identity,
            Rect::new(0, 0, 8, 9),
            Transform3D::IDENTITY,
            Rect::new(0, 0, 8, 9),
        ))
    );
}

#[test]
fn request_rejects_explicit_quad_with_shared_projection() {
    let clip = Rect::new(0, 0, 12, 12);
    let command = DrawCommand::Fill {
        area: clip,
        transform: crate::types::Transform::IDENTITY,
        quad: Some([
            crate::types::Point::new(0, 0),
            crate::types::Point::new(12, 0),
            crate::types::Point::new(12, 12),
            crate::types::Point::new(0, 12),
        ]),
        color: crate::types::Color::rgb(20, 30, 40),
        radius: Fixed::ZERO,
        opa: 255,
    };
    let request = DrawRequest::new(&command, clip).with_projective(
        Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400)),
    );
    assert_eq!(
        request.validate_projection(),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );
}

struct NoopRenderer;
impl Renderer for NoopRenderer {
    type TargetState = ();

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        let request = &draw.request();
        self.route(request).map(|_| ())
    }
    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

#[test]
fn default_capabilities_are_disabled() {
    let r = NoopRenderer;
    assert!(!r.supports_offscreen());
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: Rect::new(0, 0, 1, 1),
    };
    assert_eq!(
        r.route(&DrawRequest::new(&command, Rect::new(0, 0, 1, 1))),
        Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
    );
}

#[test]
fn default_route_rejects_affine_and_projective_requests() {
    let mut renderer = NoopRenderer;
    let clip = Rect::new(0, 0, 1, 1);
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
    );
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip).with_projective(
            Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400)),
        )),
        Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry))
    );
}

#[test]
fn submit_refuses_unknown_route_before_draw() {
    struct CountingRenderer(usize);

    impl Renderer for CountingRenderer {
        type TargetState = ();

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            self.route(request)?;
            self.0 += 1;
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let clip = Rect::new(0, 0, 1, 1);
    let command = DrawCommand::ApplyBlur {
        alpha: Fixed::ONE,
        region: clip,
    };
    let mut renderer = CountingRenderer(0);
    assert_eq!(
        renderer.draw_request(&DrawRequest::new(&command, clip)),
        Err(RenderError::Unsupported(RenderFeature::AffineGeometry))
    );
    assert_eq!(renderer.0, 0);
}
