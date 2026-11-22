use super::*;

#[test]
fn raster_pixel_geometry_preserves_fractional_origin_and_negative_bearing() {
    use textflow::shaping::{FlowPoint, PositionedGlyph};

    let first = FlowPoint { x: 256, y: 512 };
    let glyph = PositionedGlyph::new(GlyphId::new(65), FlowPoint { x: 640, y: 384 });
    let scale = Fixed::from_ratio(3, 2);
    let allocations = crate::test_support::tracked_allocations(|| {
        let origin = raster_glyph_origin(
            Point {
                x: Fixed::from_ratio(-1, 2),
                y: Fixed::from_ratio(11, 4),
            },
            first,
            &glyph,
            Fixed::from_ratio(11, 2),
            scale,
        )
        .unwrap();
        assert_eq!(origin, (1, 9));
        assert_eq!(
            raster_glyph_rect(
                origin,
                Point {
                    x: Fixed::from_ratio(-3, 2),
                    y: Fixed::from_ratio(7, 2),
                },
                ::mirx::image::Region::new(0, 0, 3, 5).unwrap(),
                8,
                8,
                scale,
            ),
            (-2, 4, 5, 8)
        );
        assert!(
            raster_glyph_origin(
                Point::ZERO,
                FlowPoint { x: i32::MIN, y: 0 },
                &glyph,
                Fixed::ONE,
                scale,
            )
            .is_none()
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn positioned_raster_preserves_baseline_offsets_and_representation_scale() {
    use textflow::shaping::{FlowPoint, PositionedGlyph};

    let font = Font::default();
    let mut raster = font
        .raster_for_output(font.map_char('A').unwrap(), 8, 8)
        .unwrap();
    raster.offset_x = Fixed::from_int(-2);
    raster.offset_y = Fixed::from_int(5);
    let first = FlowPoint {
        x: 10 * 256,
        y: 20 * 256,
    };
    let glyph = PositionedGlyph::new(
        GlyphId::new(65),
        FlowPoint {
            x: 13 * 256,
            y: 24 * 256,
        },
    )
    .with_offset(FlowPoint { x: 128, y: -64 });
    let transform = Transform::translate(Fixed::from_int(2), Fixed::from_int(-3));
    let expected = RasterQuad {
        rect: Rect {
            x: Fixed::from_ratio(17, 2),
            y: Fixed::from_ratio(75, 4),
            w: Fixed::from_int(12),
            h: Fixed::from_int(12),
        },
        transform,
    };
    let allocations = crate::test_support::tracked_allocations(|| {
        let quad = raster
            .positioned_quad(
                Point::new(7, 11),
                first,
                &glyph,
                12,
                Fixed::from_int(9),
                transform,
            )
            .unwrap();
        assert_eq!(quad, expected);
        let posed = raster
            .posed_quad(
                Point::new(7, 20),
                textflow::placement::GlyphFrame {
                    local_origin: FlowPoint {
                        x: 3 * 256 + 128,
                        y: 4 * 256 - 64,
                    },
                    unit_tangent: FlowPoint { x: 256, y: 0 },
                },
                12,
                transform,
            )
            .unwrap();
        assert_eq!(
            quad.transform.apply_rect(quad.rect),
            posed.transform.apply_rect(posed.rect)
        );
    });
    assert_eq!(allocations, 0);
}

#[test]
fn raster_geometry_rejects_missing_ink_and_position_overflow() {
    use textflow::shaping::{FlowPoint, PositionedGlyph};

    let font = Font::default();
    let mut raster = font
        .raster_for_output(font.map_char('A').unwrap(), 8, 8)
        .unwrap();
    let quad = |raster: RasterGlyph<'_>, first, glyph: PositionedGlyph| {
        raster.positioned_quad(
            Point::ZERO,
            first,
            &glyph,
            8,
            Fixed::ONE,
            Transform::IDENTITY,
        )
    };
    let origin = FlowPoint { x: 0, y: 0 };
    let glyph = PositionedGlyph::new(GlyphId::new(65), origin);
    assert!(quad(raster, FlowPoint { x: i32::MIN, y: 0 }, glyph).is_none());
    assert!(
        quad(
            raster,
            origin,
            PositionedGlyph::new(glyph.glyph_id(), FlowPoint { x: 0, y: i32::MAX },)
                .with_offset(FlowPoint { x: 0, y: 1 })
        )
        .is_none()
    );
    for region in [None, ::mirx::image::Region::new(0, 0, 0, 0).ok()] {
        raster.region = region;
        assert!(quad(raster, origin, glyph).is_none());
        assert!(
            raster
                .posed_quad(Point::ZERO, Default::default(), 8, Transform::IDENTITY)
                .is_none()
        );
    }
}

#[cfg(any(
    feature = "sdl-gpu",
    all(feature = "web-canvas", target_arch = "wasm32")
))]
#[test]
fn cached_run_origin_maps_a_baseline_to_a_line_top() {
    let font = Font::default();
    let baseline = Point::new(11, 21);
    let origin = font.line_origin_for_baseline(baseline);
    assert_eq!(origin.x, baseline.x);
    assert_eq!(origin.y + font.metrics(font.size).ascender, baseline.y);
}

#[cfg(any(feature = "sdl-gpu", feature = "wgpu"))]
#[test]
fn glyph_raster_extent_preserves_fractional_display_scale() {
    assert_eq!(
        scaled_glyph_raster_extent(17, Fixed::from_ratio(3, 2)),
        Some(26)
    );
    assert_eq!(
        scaled_glyph_raster_extent(17, Fixed::from_ratio(5, 4)),
        Some(22)
    );
}

#[cfg(feature = "sdl-gpu")]
#[test]
fn raster_run_bounds_quantize_after_fractional_scaling() {
    let scale = Fixed::from_ratio(3, 2);
    let bounds = RasterRunBounds::from_logical(
        crate::types::Rect {
            x: Fixed::from_ratio(-1, 2),
            y: Fixed::from_ratio(1, 4),
            w: Fixed::from_ratio(21, 2),
            h: Fixed::from_ratio(15, 2),
        },
        scale,
    )
    .unwrap();

    assert_eq!((bounds.width, bounds.height), (16, 12));
    assert_eq!(bounds.offset.x, Fixed::from_int(-1) / scale);
    assert_eq!(bounds.offset.y, Fixed::ZERO);
    assert_eq!(bounds.size.x, Fixed::from_int(16) / scale);
    assert_eq!(bounds.size.y, Fixed::from_int(12) / scale);
}

#[cfg(feature = "sdl-gpu")]
#[test]
fn raster_run_key_uses_font_identity_but_not_composite_alpha() {
    let font = Font::default();
    let scale = Fixed::from_ratio(3, 2);
    let faint = font.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 40), scale);
    let opaque = font.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 255), scale);
    assert_eq!(faint, opaque);

    let mut larger = font.clone();
    larger.size += 1;
    assert_ne!(
        faint,
        larger.raster_run_key(&[], &crate::types::Color::rgba(1, 2, 3, 40), scale)
    );
}

#[test]
fn default_token_resolves_to_mirx() {
    let mgr = default_font_manager();
    let f = mgr.resolve(FontToken::Default.cache_key());
    assert_eq!(f.family, "default");
    assert_eq!(f.size, 8);
}

#[test]
fn resolved_stack_validates_capacity_before_an_optional_manager() {
    static FALLBACKS: &[FontToken] = &[FontToken::Heading];
    let stack = FontStack::default().with_fallbacks(Cow::Borrowed(FALLBACKS));
    let result = ResolvedFontStack::resolve(None, &stack, None, 1);
    assert!(matches!(
        result,
        Err(FontStackCapacityError {
            required: 2,
            limit: 1
        })
    ));
    assert!(
        ResolvedFontStack::resolve(None, &stack, None, 2)
            .unwrap()
            .is_none()
    );
    let manager = default_font_manager();
    manager.prefetch(FontToken::Default.cache_key());
    manager.prefetch(FontToken::Heading.cache_key());
    for _ in 0..2 {
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let fonts = ResolvedFontStack::resolve(Some(&manager), &stack, Some(14), 2)
                    .unwrap()
                    .unwrap();
                assert_eq!(fonts.len, 2);
                assert_eq!(fonts.primary().size, 14);
            }),
            0
        );
    }
}

#[test]
fn registered_fallback_face_closes_every_stack() {
    let manager = default_font_manager();
    let plain = ResolvedFontStack::resolve(Some(&manager), &FontStack::default(), None, 4)
        .unwrap()
        .unwrap();
    assert_eq!(plain.len, 1);

    manager.add_static(
        FontToken::FALLBACK.cache_key(),
        Font {
            family: "backing",
            size: 20,
            provider: Font::default().provider,
        },
    );
    let backed = ResolvedFontStack::resolve(Some(&manager), &FontStack::default(), Some(14), 4)
        .unwrap()
        .unwrap();
    assert_eq!(backed.len, 2);
    let last = backed.fonts[1].as_ref().unwrap();
    assert_eq!((last.family, last.size), ("backing", 14));

    let named = FontStack::default().with_fallbacks(&[FontToken::FALLBACK][..]);
    let named = ResolvedFontStack::resolve(Some(&manager), &named, None, 4)
        .unwrap()
        .unwrap();
    assert_eq!(named.len, 2);
    let full = ResolvedFontStack::resolve(Some(&manager), &FontStack::default(), None, 1)
        .unwrap()
        .unwrap();
    assert_eq!(full.len, 1);
}

#[test]
fn unbound_token_falls_back_to_mirx() {
    let mgr = default_font_manager();
    let f = mgr.resolve(FontToken::Heading.cache_key());
    assert_eq!(f.family, "default");
}

#[test]
fn registered_token_overrides_fallback() {
    let mgr = default_font_manager();
    mgr.add_static(
        FontToken::Heading.cache_key(),
        Font {
            family: "fake-heading",
            size: 16,
            provider: Font::default().provider,
        },
    );
    let f = mgr.resolve(FontToken::Heading.cache_key());
    assert_eq!(f.family, "fake-heading");
    assert_eq!(f.size, 16);
}

#[test]
fn registering_default_key_overrides_bundled_font() {
    let mgr = default_font_manager();
    mgr.add_static(
        FontToken::Default.cache_key(),
        Font {
            family: "user-default",
            size: 12,
            provider: Font::default().provider,
        },
    );
    assert_eq!(
        mgr.resolve(FontToken::Default.cache_key()).family,
        "user-default"
    );
}

#[test]
fn custom_token_resolves_when_registered() {
    let mgr = default_font_manager();
    let token = FontToken::Custom("brand");
    mgr.add_static(
        token.cache_key(),
        Font {
            family: "brand-face",
            size: 10,
            provider: Font::default().provider,
        },
    );
    assert_eq!(mgr.resolve(token.cache_key()).family, "brand-face");
}

#[test]
fn cache_key_is_stable_per_token() {
    assert_eq!(FontToken::Default.cache_key(), "\0mirui-font-default");
    assert_eq!(FontToken::Heading.cache_key(), "\0mirui-font-heading");
    assert_eq!(FontToken::Custom("brand").cache_key(), "brand");
}

#[test]
fn cache_key_never_allocates() {
    assert_eq!(
        FontToken::Custom("brand").cache_key().as_ptr(),
        "brand".as_ptr()
    );
}

#[test]
fn font_stack_keeps_borrowed_fallback_order() {
    static FALLBACKS: [FontToken; 2] = [FontToken::Mono, FontToken::Custom("symbols")];
    let stack = FontStack::new(FontToken::Heading).with_fallbacks(&FALLBACKS[..]);
    assert_eq!(
        stack.iter().collect::<alloc::vec::Vec<_>>(),
        alloc::vec![
            &FontToken::Heading,
            &FontToken::Mono,
            &FontToken::Custom("symbols")
        ]
    );
    assert!(matches!(stack.fallbacks, Cow::Borrowed(_)));
}

#[test]
fn glyph_surface_rejects_short_rows_and_storage() {
    let id = FontSurfaceId::new(7);
    assert!(matches!(
        GlyphSurface::new(
            &[0; 1],
            9,
            1,
            1,
            ::mirx::image::SampleLayout::A1,
            ::mirx::types::ByteAlignment::ONE,
            id,
        ),
        Err(GlyphSurfaceError::StrideTooSmall {
            minimum: 2,
            actual: 1
        })
    ));
    assert!(matches!(
        GlyphSurface::new(
            &[0; 1],
            8,
            2,
            1,
            ::mirx::image::SampleLayout::A1,
            ::mirx::types::ByteAlignment::ONE,
            id,
        ),
        Err(GlyphSurfaceError::DataTooSmall {
            needed: 2,
            available: 1
        })
    ));
}

#[test]
fn glyph_surface_reports_actual_address_alignment() {
    #[repr(align(64))]
    struct Aligned([u8; 65]);

    let bytes = Aligned([0; 65]);
    let aligned = GlyphSurface::new(
        &bytes.0[..1],
        8,
        1,
        1,
        ::mirx::image::SampleLayout::A1,
        ::mirx::types::ByteAlignment::new(64).unwrap(),
        FontSurfaceId::new(7),
    )
    .unwrap();
    let unaligned = GlyphSurface::new(
        &bytes.0[1..2],
        8,
        1,
        1,
        ::mirx::image::SampleLayout::A1,
        ::mirx::types::ByteAlignment::new(64).unwrap(),
        FontSurfaceId::new(8),
    )
    .unwrap();
    assert!(aligned.address_is_aligned());
    assert!(!unaligned.address_is_aligned());
}

#[test]
fn metrics_is_extractable_via_has_probe() {
    let font = Font::default();
    let meta = font.extract_meta();
    assert_eq!(meta.ascender, Fixed::from_int(7));
    assert_eq!(meta.descender, Fixed::from_int(-1));
    assert_eq!(meta.line_height, crate::types::Fixed::from_int(8));
}

#[test]
fn glyph_roundtrip_for_ascii() {
    let font = Font::default();
    for ppem in [8, 16, 24] {
        let glyph = font.glyph('A', ppem).expect("ASCII glyph");
        assert_eq!(glyph.advance, Fixed::from_int(i32::from(ppem)));
        assert_eq!(
            font.metrics(ppem).line_height,
            Fixed::from_int(i32::from(ppem))
        );
        let GlyphKind::Raster {
            region,
            representation,
            bearing_y,
            ..
        } = glyph.kind;
        assert_eq!((region.width(), region.height()), (8, 8));
        assert_eq!(representation.design_ppem(), 8);
        assert_eq!(bearing_y, Fixed::from_ratio(i32::from(ppem) * 7, 8));
    }
}

#[test]
fn glyph_falls_back_to_question_mark_outside_ascii() {
    let font = Font::default();
    let GlyphKind::Raster { region: actual, .. } = font.glyph('日', 16).unwrap().kind;
    let GlyphKind::Raster {
        region: expected, ..
    } = font.glyph('?', 16).unwrap().kind;
    assert_eq!(actual, expected);
}

#[test]
fn default_font_borrows_shared_mirx_surface() {
    let font = Font::default();
    let cloned = font.clone();
    assert!(Rc::ptr_eq(&font.provider, &cloned.provider));
    let a = font.raster(font.map_char('A').unwrap(), 8).unwrap();
    let b = cloned.raster(cloned.map_char('B').unwrap(), 8).unwrap();
    assert_eq!(a.surface.samples().as_ptr(), b.surface.samples().as_ptr());
    assert_eq!(a.surface.sample_layout(), ::mirx::image::SampleLayout::A1);
    assert_eq!(a.surface.samples().len(), 760);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for size in [8, 12, 16, 24] {
                let clone = font.clone();
                for ch in ' '..='~' {
                    let glyph = clone.map_char(ch).unwrap();
                    assert!(clone.raster(glyph, size).is_some());
                    assert_eq!(
                        clone.glyph_advance(glyph, size),
                        Some(Fixed::from_int(i32::from(size)))
                    );
                }
            }
        }),
        0
    );
}

#[cfg(feature = "wgpu")]
#[test]
fn default_font_exposes_the_shared_coverage_surface_to_gpu_backends() {
    let font = Font::default();
    let raster = font
        .raster_for_output(font.map_char('A').unwrap(), 8, 16)
        .unwrap();

    assert_eq!(
        raster.surface.sample_layout(),
        ::mirx::image::SampleLayout::A1
    );
    assert_eq!(raster.surface.width(), 8);
    assert_eq!(raster.surface.height(), 8 * 95);
    assert_eq!(raster.region.unwrap().y(), u32::from(b'A' - b' ') * 8);
    assert_eq!(raster.representation.design_ppem(), 8);
}

#[test]
fn custom_provider_routes_through_backend() {
    struct AllX;
    impl FontProvider for AllX {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(2)
        }
        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }
        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(6))
        }
        fn raster(
            &self,
            _glyph: GlyphId,
            _layout_ppem: u16,
            _output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            Some(RasterGlyph {
                surface: GlyphSurface::new(
                    &[],
                    0,
                    0,
                    0,
                    ::mirx::image::SampleLayout::A1,
                    ::mirx::types::ByteAlignment::ONE,
                    FontSurfaceId::new(2),
                )
                .unwrap(),
                region: None,
                offset_x: Fixed::ZERO,
                offset_y: Fixed::ZERO,
                representation: ::mirx::font::FontRepresentation::coverage(1, 6, 0).unwrap(),
            })
        }
        fn metrics(&self, _requested_size: u16) -> FontMetrics {
            FontMetrics {
                ascender: crate::types::Fixed::from_int(6),
                descender: crate::types::Fixed::ZERO,
                line_height: crate::types::Fixed::from_int(6),
            }
        }
    }
    let font = Font {
        family: "all-x",
        size: 6,
        provider: Rc::new(AllX),
    };
    assert_eq!(
        font.glyph('A', 16).unwrap().advance,
        crate::types::Fixed::from_int(6)
    );
    assert_eq!(
        font.metrics(font.size).line_height,
        crate::types::Fixed::from_int(6)
    );

    let mut output = [textflow::shaping::ShapedGlyph::default(); 1];
    let request = textflow::shaping::ShapeRequest::new(
        "A",
        0..1,
        textflow::bidi::Direction::LeftToRight,
        textflow::unicode::Script::Latin,
    );
    assert_eq!(font.shape_into(6, &request, &mut output), Ok(1));
    assert_eq!(output[0].glyph_id(), GlyphId::new(1));
    assert_eq!(output[0].advance.x, to_textflow(Fixed::from_int(6)));
}

#[test]
fn paragraph_language_reaches_the_font_adapter() {
    struct LanguageProbe;

    impl FontProvider for LanguageProbe {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(3)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(6))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            _layout_ppem: u16,
            _output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            None
        }

        fn metrics(&self, _ppem: u16) -> FontMetrics {
            FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-1),
                line_height: Fixed::from_int(8),
            }
        }

        fn shape_into(
            &self,
            _ppem: u16,
            request: &textflow::shaping::ShapeRequest<'_>,
            output: &mut [textflow::shaping::ShapedGlyph],
        ) -> Result<usize, textflow::shaping::ShapeError> {
            assert_eq!(request.language, Some("zh-Hans"));
            output[0] = textflow::shaping::ShapedGlyph::new(
                GlyphId::new(1),
                textflow::shaping::TextRange::new(0, 1),
            );
            Ok(1)
        }
    }

    let font = Font {
        family: "language-probe",
        size: 8,
        provider: Rc::new(LanguageProbe),
    };
    let face = FontTypeface::new(&font, 8).with_language(Some("zh-Hans"));
    let request = textflow::shaping::ShapeRequest::new(
        "A",
        0..1,
        textflow::bidi::Direction::LeftToRight,
        textflow::unicode::Script::Latin,
    );
    let mut output = [textflow::shaping::ShapedGlyph::default(); 1];

    assert_eq!(
        textflow::shaping::Typeface::shape_into(&face, &request, &mut output),
        Ok(1)
    );
}

#[test]
fn required_shaping_rejects_simple_faces() {
    let font = Font::default();
    let face =
        FontTypeface::new(&font, 8).with_shaping(crate::ui::widgets::ShapingPolicy::Required);
    let request = textflow::shaping::ShapeRequest::new(
        "A",
        0..1,
        textflow::bidi::Direction::LeftToRight,
        textflow::unicode::Script::Latin,
    );
    let mut output = [textflow::shaping::ShapedGlyph::default(); 1];

    assert_eq!(
        textflow::shaping::Typeface::shape_into(&face, &request, &mut output),
        Err(textflow::shaping::ShapeError::ShapingUnavailable)
    );
}

#[test]
fn simple_policy_bypasses_complex_shaping() {
    struct ComplexProbe;

    impl FontProvider for ComplexProbe {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(4)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(6))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            _layout_ppem: u16,
            _output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            None
        }

        fn metrics(&self, _ppem: u16) -> FontMetrics {
            FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-1),
                line_height: Fixed::from_int(8),
            }
        }

        fn supports_complex_shaping(&self) -> bool {
            true
        }

        fn shape_into(
            &self,
            _ppem: u16,
            request: &textflow::shaping::ShapeRequest<'_>,
            output: &mut [textflow::shaping::ShapedGlyph],
        ) -> Result<usize, textflow::shaping::ShapeError> {
            output[0] = textflow::shaping::ShapedGlyph::new(
                GlyphId::new(77),
                textflow::shaping::TextRange::new(
                    request.range.start as u32,
                    request.range.end as u32,
                ),
            );
            Ok(1)
        }
    }

    let font = Font {
        family: "complex-probe",
        size: 8,
        provider: Rc::new(ComplexProbe),
    };
    let request = textflow::shaping::ShapeRequest::new(
        "A",
        0..1,
        textflow::bidi::Direction::LeftToRight,
        textflow::unicode::Script::Latin,
    );
    let mut output = [textflow::shaping::ShapedGlyph::default(); 1];
    let automatic = FontTypeface::new(&font, 8);
    assert_eq!(
        textflow::shaping::Typeface::shape_into(&automatic, &request, &mut output),
        Ok(1)
    );
    assert_eq!(output[0].glyph_id(), GlyphId::new(77));

    let simple =
        FontTypeface::new(&font, 8).with_shaping(crate::ui::widgets::ShapingPolicy::Simple);
    assert_eq!(
        textflow::shaping::Typeface::shape_into(&simple, &request, &mut output),
        Ok(1)
    );
    assert_eq!(output[0].glyph_id(), GlyphId::new(1));
}
