use alloc::vec::Vec;

use super::*;
use crate::render::font::{GlyphId, default_font_manager};
use crate::render::renderer::Renderer;
use crate::types::Transform;
use crate::ui::Style;
use crate::ui::theme::{Theme, WidgetState};
use textflow::shaping::{FlowPoint, PositionedGlyph};

#[derive(Default)]
struct RecordingRenderer {
    glyph_runs: Vec<(Point, usize)>,
}

#[crate::component]
struct LayoutFollower {
    source: Entity,
}

#[crate::view(component = LayoutFollower)]
fn follow_layout(
    follower: component!(),
    handles: query!(crate::text::TextLayoutHandle),
    calls: res!(core::cell::Cell<usize>),
) {
    if handles.get(follower.source).is_some() {
        calls.set(calls.get() + 1);
    }
}

impl Renderer for RecordingRenderer {
    type TargetState = ();

    fn route(
        &self,
        _: &crate::render::DrawRequest<'_, '_>,
    ) -> Result<crate::render::RenderRoute, crate::render::RenderError> {
        Ok(crate::render::RenderRoute::Native)
    }

    fn submit(
        &mut self,
        draw: &::mirui::render::PreparedDraw<'_>,
    ) -> Result<(), crate::render::RenderError> {
        let request = &draw.request();
        self.route(request)?;
        if let DrawCommand::GlyphRun { pos, glyphs, .. } = request.command {
            self.glyph_runs.push((*pos, glyphs.len()));
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Ok(())
    }
}

#[test]
fn static_run_renders_without_a_layout_workspace() {
    static GLYPHS: [PositionedGlyph; 2] = [
        PositionedGlyph::new(GlyphId::new(65), FlowPoint { x: 0, y: 7 << 8 })
            .with_advance(FlowPoint { x: 8 << 8, y: 0 }),
        PositionedGlyph::new(
            GlyphId::new(66),
            FlowPoint {
                x: 8 << 8,
                y: 7 << 8,
            },
        )
        .with_advance(FlowPoint { x: 8 << 8, y: 0 }),
    ];
    let mut world = World::new();
    world.insert_resource(Theme::default());
    world.insert_resource(default_font_manager());
    let entity = world.spawn_empty();
    world.insert(entity, StaticGlyphRun::new(&GLYPHS));
    let style = Style::default();
    let rect = Rect {
        x: Fixed::from_int(3),
        y: Fixed::from_int(5),
        w: Fixed::from_int(20),
        h: Fixed::from_int(10),
    };
    let mut renderer = RecordingRenderer::default();
    {
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        static_glyph_run_view().render().unwrap()(&world, entity, &mut paint);
    }

    assert_eq!(renderer.glyph_runs, vec![(Point::new(3, 5), 2)]);
    assert_eq!(
        StaticGlyphRun::new(&GLYPHS).measure(Fixed::from_int(8)),
        (Fixed::from_int(16), Fixed::from_int(8))
    );
}

#[cfg(feature = "std")]
#[test]
fn static_run_typed_roles_refetch_resources_and_borrow_glyphs_without_allocating() {
    use crate::render::RenderError;
    use crate::render::font::{Font, FontToken};
    use crate::test_support::tracked_allocations;
    use core::any::TypeId;

    static GLYPHS: [PositionedGlyph; 1] = [PositionedGlyph::new(
        GlyphId::new(65),
        FlowPoint { x: 0, y: 7 << 8 },
    )];
    struct Trace {
        expected_color: crate::types::Color,
        family: &'static str,
        size: u16,
        fail: bool,
        calls: usize,
    }
    impl Renderer for Trace {
        type TargetState = ();

        fn route(
            &self,
            _: &::mirui::render::DrawRequest<'_, '_>,
        ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
            Ok(::mirui::render::RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            assert_eq!(request.clip, Rect::new(4, 6, 18, 8));
            let DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } = request.command
            else {
                panic!("expected a borrowed static glyph run");
            };
            assert_eq!(
                *pos,
                Point {
                    x: Fixed::from_int(3),
                    y: Fixed::from_int(12) - font.metrics(font.size).ascender
                }
            );
            assert_eq!(
                *transform,
                Transform::translate(Fixed::from_int(2), Fixed::from_int(3))
            );
            assert_eq!(glyphs.as_ptr(), GLYPHS.as_ptr());
            assert_eq!(glyphs.len(), GLYPHS.len());
            assert_eq!((font.family, font.size), (self.family, self.size));
            assert_eq!(*color, self.expected_color);
            assert_eq!(*opa, 255);
            self.calls += 1;
            if self.fail {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        }
        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }
    }

    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, StaticGlyphRun::new(&GLYPHS));
    let mut render = None;
    for _ in 0..2 {
        let mut descriptor = None;
        assert_eq!(
            tracked_allocations(|| descriptor = Some(static_glyph_run_view())),
            0
        );
        let descriptor = descriptor.unwrap();
        assert_eq!(descriptor.name(), "StaticGlyphRun");
        assert_eq!(
            descriptor.component_filter(),
            Some(TypeId::of::<StaticGlyphRun>())
        );
        assert_eq!(descriptor.priority(), Some(80));
        assert_eq!(
            descriptor.invalidation(),
            crate::ui::ViewInvalidation::Layout
        );
        assert!(descriptor.auto_attach().is_none());
        assert!(descriptor.inputs().is_none());
        assert!(descriptor.observation().is_none());
        let mut installed = 0;
        let descriptor = descriptor.install(|_| installed += 1);
        assert_eq!(installed, 0);
        render = descriptor.render();
    }
    let render = render.unwrap();
    let mut style = Style {
        font_stack: Some(FontToken::Heading.into()),
        ..Style::default()
    };
    let rect = Rect::new(3, 5, 20, 10);
    let clip = Rect::new(4, 6, 18, 8);
    for (family, size, theme, override_size) in [
        (
            "first-static-font",
            16,
            Theme::dark(),
            crate::ui::FontSize::Native,
        ),
        (
            "replacement-static-font",
            20,
            Theme::light(),
            crate::ui::FontSize::Pixels(0),
        ),
    ] {
        let fonts = default_font_manager();
        fonts.add_static(
            FontToken::Heading.cache_key(),
            Font {
                family,
                size,
                provider: Font::default().provider,
            },
        );
        // Prepare the existing font cache before checking the typed paint adapter.
        fonts.prefetch(FontToken::Heading.cache_key());
        world.insert_resource(fonts);
        style.font_size = override_size;
        let expected_color = crate::ui::Typography::local(&style)
            .text_color
            .resolve_in(&theme, WidgetState::Hovered);
        world.insert_resource(theme);
        for fail in [false, true] {
            for _ in 0..2 {
                let mut trace = Trace {
                    expected_color,
                    family,
                    size: crate::ui::Typography::local(&style)
                        .font_size
                        .unwrap_or(size),
                    fail,
                    calls: 0,
                };
                {
                    let mut paint = PaintCtx::new(
                        &mut trace,
                        &style,
                        &rect,
                        &clip,
                        Transform::translate(Fixed::from_int(2), Fixed::from_int(3)),
                        None,
                        WidgetState::Hovered,
                    );
                    assert_eq!(
                        tracked_allocations(|| render(&world, entity, &mut paint)),
                        0
                    );
                    assert_eq!(paint.error(), fail.then_some(RenderError::BackendFailure));
                    assert!(!paint.background_handled());
                }
                assert_eq!(trace.calls, 1);
            }
        }
    }
}

#[cfg(feature = "std")]
#[test]
fn static_run_optional_resources_preserve_early_returns_and_the_theme_requirement() {
    use crate::render::font::{FontManager, FontToken};
    use crate::test_support::tracked_allocations;

    static GLYPHS: [PositionedGlyph; 1] = [PositionedGlyph::new(
        GlyphId::new(65),
        FlowPoint { x: 0, y: 7 << 8 },
    )];
    let mut world = World::new();
    world.insert_resource(Theme::default());
    let entity = world.spawn_empty();
    let style = Style::default();
    let rect = Rect::new(0, 0, 20, 10);
    let render = static_glyph_run_view().render().unwrap();
    let mut renderer = RecordingRenderer::default();
    for stage in 0..3 {
        world.insert(
            entity,
            StaticGlyphRun::new(if stage == 2 { &GLYPHS } else { &[] }),
        );
        if stage == 1 {
            world.insert_resource(default_font_manager());
        } else {
            world.remove_resource::<FontManager>();
        }
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert_eq!(
            tracked_allocations(|| render(&world, entity, &mut paint)),
            0
        );
        assert_eq!(paint.error(), None);
    }
    assert!(renderer.glyph_runs.is_empty());
    let fonts = default_font_manager();
    fonts.prefetch(FontToken::Default.cache_key());
    world.insert_resource(fonts);
    world.remove_resource::<Theme>();
    {
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            render(&world, entity, &mut paint);
        }))
        .unwrap_err();
        let message = error
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| error.downcast_ref::<String>().map(String::as_str))
            .unwrap();
        assert!(message.contains("requires resource"));
        assert!(message.contains("Theme"));
    }
    assert!(renderer.glyph_runs.is_empty());
}

#[test]
fn typed_paint_preserves_missing_dependencies_and_theme_validation() {
    let mut world = World::new();
    world.insert_resource(Theme::default());
    let entity = world.spawn_empty();
    let descriptor = view();
    assert_eq!(descriptor.name(), "Text");
    assert_eq!(descriptor.priority(), Some(80));
    assert_eq!(
        descriptor.component_filter(),
        Some(core::any::TypeId::of::<Text>())
    );
    assert_eq!(
        descriptor.invalidation(),
        crate::ui::ViewInvalidation::Layout
    );
    assert!(descriptor.auto_attach().is_some());
    assert!(descriptor.inputs().is_none());
    let render = descriptor.render().unwrap();
    let style = Style::default();
    let rect = Rect::new(0, 0, 20, 10);
    let mut recorder = RecordingRenderer::default();
    {
        let mut paint = PaintCtx::new(
            &mut recorder,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                render(&world, entity, &mut paint);
            }),
            0
        );
        assert_eq!(paint.error(), None);
    }
    world.insert(entity, Text::new("A"));
    world.remove_resource::<Theme>();
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut paint = PaintCtx::new(
            &mut recorder,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        render(&world, entity, &mut paint);
    }));
    assert!(
        rejected.is_err(),
        "Text requires Theme even before layout is available"
    );
    world.insert_resource(Theme::default());
    for stage in 0..3 {
        match stage {
            1 => world.insert_resource(crate::ui::Outputs::<crate::text::TextGeometry>::new_text(
                crate::text::TextLayoutLimits::EMBEDDED,
            )),
            2 => {
                let fonts = default_font_manager();
                fonts.prefetch(crate::render::font::FontToken::Default.cache_key());
                world.insert_resource(fonts);
            }
            _ => {}
        }
        let mut paint = PaintCtx::new(
            &mut recorder,
            &style,
            &rect,
            &rect,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        assert_eq!(
            crate::test_support::tracked_allocations(|| render(&world, entity, &mut paint)),
            0
        );
        assert_eq!(paint.error(), None);
    }
    assert!(recorder.glyph_runs.is_empty());
}

#[test]
fn unchanged_layout_handles_do_not_publish_query_changes() {
    use crate::surface::Surface;

    let mut app = crate::app::App::headless(32, 16);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        crate::ui! {
            Text ("A", id: "handle_consumer", width: 20, height: 10)
        };
    });
    let label = app.world.find_by_id("handle_consumer").unwrap();
    app.render_dirty().unwrap();
    let handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(label)
        .unwrap();
    app.world.insert(root, crate::ui::dirty::Dirty);
    crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
        .unwrap();
    assert_eq!(
        app.world.get::<crate::text::TextLayoutHandle>(label),
        Some(&handle)
    );
    assert!(!app.world.has::<crate::ui::dirty::Dirty>(label));
    app.world.get_mut::<Text>(label).unwrap().set_content("B");
    crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
        .unwrap();
    assert_ne!(
        app.world.get::<crate::text::TextLayoutHandle>(label),
        Some(&handle)
    );
    assert!(app.world.has::<crate::ui::dirty::Dirty>(label));
}

#[test]
fn replacing_the_layout_cache_repaints_queries_when_handle_numbers_are_reused() {
    let mut app = crate::app::App::headless(32, 16);
    app.with_default_widgets();
    app.with_widget(follow_layout::view());
    app.add_resource(core::cell::Cell::new(0_usize));
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        crate::ui! {
            Text ("A", id: "reused_handle", width: 20, height: 10)
        };
    });
    let label = app.world.find_by_id("reused_handle").unwrap();
    app.compose(root, |cx| {
        crate::ui! {
            View (width: 8, height: 6) [
                LayoutFollower { source: label },
            ]
        };
    });
    app.render_dirty().unwrap();
    let handle = *app
        .world
        .get::<crate::text::TextLayoutHandle>(label)
        .unwrap();
    let calls = app
        .world
        .resource::<core::cell::Cell<usize>>()
        .unwrap()
        .get();
    assert!(calls > 0);
    app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
        .unwrap();
    app.world.get_mut::<Text>(label).unwrap().set_content("B");
    app.render_dirty().unwrap();
    assert_eq!(
        app.world.get::<crate::text::TextLayoutHandle>(label),
        Some(&handle)
    );
    assert!(
        app.world
            .resource::<core::cell::Cell<usize>>()
            .unwrap()
            .get()
            > calls
    );
}

#[test]
fn build_spawns_text_with_style() {
    let mut world = World::new();
    let builder_parent = world.spawn_empty();
    let e = crate::ui::UiScope::new(&mut world, builder_parent)
        .spawn(Text::build("hi").style(crate::ui::Style::default()));
    let text = world.get::<Text>(e).unwrap();
    assert_eq!(text.resolve(world.resource()), "hi");
    assert!(world.has::<crate::ui::Style>(e));
    assert!(world.has::<crate::ui::Widget>(e));
}

#[test]
fn build_without_style_initializes_default_style() {
    let mut world = World::new();
    let builder_parent = world.spawn_empty();
    let e = crate::ui::UiScope::new(&mut world, builder_parent).spawn(Text::build("hi"));
    assert!(world.has::<Text>(e));
    assert!(world.has::<crate::ui::Style>(e));
}

#[test]
fn static_str_is_borrowed() {
    let t: Text = "hi".into();
    match t.content() {
        TextContent::Plain(Cow::Borrowed(s)) => assert_eq!(*s, "hi"),
        _ => panic!("expected borrowed cow for &'static str"),
    }
}

#[test]
fn string_is_owned() {
    let s: String = "hi".into();
    let t: Text = s.into();
    assert!(matches!(t.content(), TextContent::Plain(Cow::Owned(_))));
}

#[test]
fn bounded_text_formats_utf8_without_replacing_valid_content_on_error() {
    let mut text = Text::new_bounded(8).unwrap();
    assert!(!text.has_valid_content());
    assert_eq!(text.try_set_bounded::<8>(format_args!("é🙂")), Ok(true));
    assert!(text.has_valid_content());
    assert_eq!(text.try_set_bounded::<8>(format_args!("é🙂")), Ok(false));
    assert_eq!(text.resolve(None), "é🙂");
    assert_eq!(
        text.try_set_bounded::<8>(format_args!("é{}", "🙂🙂")),
        Err(TextContentError::Capacity {
            required: 10,
            capacity: 8,
        })
    );
    assert_eq!(text.resolve(None), "é🙂");
    assert_eq!(
        text.last_content_error(),
        Some(TextContentError::Capacity {
            required: 10,
            capacity: 8,
        })
    );
    assert_eq!(text.try_set_bounded::<8>(format_args!("ok")), Ok(true));
    assert_eq!(text.last_content_error(), None);
}

#[test]
fn bounded_text_rejects_format_errors_and_capacity_mismatch() {
    struct Broken;
    impl core::fmt::Display for Broken {
        fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("partial")?;
            Err(core::fmt::Error)
        }
    }

    let mut text = Text::new_bounded(8).unwrap();
    assert_eq!(
        text.try_set_bounded::<8>(format_args!("{Broken}")),
        Err(TextContentError::Format)
    );
    assert!(!text.has_valid_content());
    assert_eq!(text.resolve(None), "");
    assert_eq!(
        text.try_set_bounded::<4>(format_args!("ok")),
        Err(TextContentError::CapacityMismatch {
            declared: 8,
            provided: 4,
        })
    );
    assert_eq!(text.try_set_bounded::<8>(format_args!("ok")), Ok(true));
    assert!(text.has_valid_content());
}

#[test]
fn bounded_text_preserves_reserved_storage_when_cloned_or_set() {
    let mut text = Text::new("ready").with_capacity(8).unwrap();
    assert!(text.has_valid_content());
    let TextContent::Plain(Cow::Owned(storage)) = text.content() else {
        panic!("bounded text must own its content storage");
    };
    assert!(storage.capacity() >= 8);

    let cloned = text.clone();
    let TextContent::Plain(Cow::Owned(storage)) = cloned.content() else {
        panic!("cloned bounded text must own its content storage");
    };
    assert!(storage.capacity() >= 8);

    assert_eq!(text.try_set_content("ready"), Ok(false));
    assert_eq!(
        text.try_set_content("too many bytes"),
        Err(TextContentError::Capacity {
            required: 14,
            capacity: 8,
        })
    );
    assert_eq!(text.resolve(None), "ready");
    assert_eq!(
        text.try_set_content(Localized::new("welcome")),
        Err(TextContentError::Localized)
    );
    assert_eq!(text.resolve(None), "ready");
    assert_eq!(text.try_set_content("changed"), Ok(true));
    assert_eq!(text.last_content_error(), None);
    let TextContent::Plain(Cow::Owned(storage)) = text.content() else {
        panic!("bounded text must own its content storage");
    };
    assert!(storage.capacity() >= 8);
}

#[test]
fn localized_text_cannot_declare_unresolved_content_capacity() {
    assert_eq!(
        Text::new(Localized::new("welcome"))
            .with_capacity(8)
            .unwrap_err(),
        TextContentError::Localized
    );
}

#[test]
fn from_localized_yields_localized() {
    let t: Text = Localized::new("welcome").into();
    assert!(t.is_localized());
}

#[test]
fn paragraph_defaults_are_layout_safe() {
    let paragraph = ParagraphStyle::default();
    assert_eq!(paragraph.wrap, TextWrap::Word);
    assert_eq!(paragraph.align, TextAlign::Start);
    assert_eq!(paragraph.vertical_align, TextVerticalAlign::Start);
    assert_eq!(paragraph.shaping, ShapingPolicy::Auto);
    assert!(paragraph.features.as_slice().is_empty());

    let request = paragraph.layout_request(
        "unbroken",
        crate::render::font::FontMetrics {
            ascender: Fixed::from_int(7),
            descender: Fixed::from_int(-1),
            line_height: Fixed::from_int(8),
        },
        Some(Fixed::from_int(4)),
    );
    assert_eq!(request.wrap, textflow::layout::WrapMode::WordOrGrapheme);
}

#[test]
fn label_is_single_line_and_centered_on_both_axes() {
    let text = Text::label("Apply");
    let paragraph = text.paragraph();

    assert_eq!(paragraph.wrap, TextWrap::NoWrap);
    assert_eq!(paragraph.align, TextAlign::Center);
    assert_eq!(paragraph.vertical_align, TextVerticalAlign::Center);
    assert_eq!(paragraph.overflow, TextOverflow::Clip);
}

#[test]
fn vertical_alignment_uses_remaining_box_height() {
    let box_height = Fixed::from_int(30);
    let text_height = Fixed::from_int(12);
    assert_eq!(
        vertical_offset(TextVerticalAlign::Start, box_height, text_height),
        Fixed::ZERO
    );
    assert_eq!(
        vertical_offset(TextVerticalAlign::Center, box_height, text_height),
        Fixed::from_int(9)
    );
    assert_eq!(
        vertical_offset(TextVerticalAlign::End, box_height, text_height),
        Fixed::from_int(18)
    );
    assert_eq!(
        vertical_offset(TextVerticalAlign::Center, text_height, box_height),
        Fixed::ZERO
    );
}

#[test]
fn caret_hit_distance_clamps_to_the_segment() {
    let start = Point::new(10, 10);
    let end = Point::new(10, 20);

    assert_eq!(
        point_segment_distance_squared(Point::new(13, 15), start, end),
        Fixed64::from_int(9)
    );
    assert_eq!(
        point_segment_distance_squared(Point::new(13, 24), start, end),
        Fixed64::from_int(25)
    );
}

#[test]
fn caret_hit_distance_handles_a_collapsed_segment() {
    let caret = Point::new(4, 8);

    assert_eq!(
        point_segment_distance_squared(Point::new(7, 12), caret, caret),
        Fixed64::from_int(25)
    );
}

#[test]
fn selection_pairs_stay_inside_one_visual_run() {
    let ltr = [
        textflow::shaping::CaretStop {
            text_offset: 0,
            position: FlowPoint { x: 0, y: 0 },
            bidi_level: 0,
        },
        textflow::shaping::CaretStop {
            text_offset: 2,
            position: FlowPoint { x: 2 << 8, y: 0 },
            bidi_level: 0,
        },
    ];
    assert!(selected_pair(
        &ltr,
        textflow::shaping::TextRange::new(0, 3),
        0,
        &(1..3)
    ));
    assert!(!selected_pair(
        &ltr,
        textflow::shaping::TextRange::new(1, 3),
        0,
        &(0..3)
    ));

    let rtl = [
        textflow::shaping::CaretStop {
            text_offset: 4,
            position: FlowPoint { x: 0, y: 0 },
            bidi_level: 1,
        },
        textflow::shaping::CaretStop {
            text_offset: 2,
            position: FlowPoint { x: 2 << 8, y: 0 },
            bidi_level: 1,
        },
    ];
    assert!(selected_pair(
        &rtl,
        textflow::shaping::TextRange::new(2, 4),
        1,
        &(2..4)
    ));
}

#[test]
fn selection_ribbon_uses_oriented_caret_edges() {
    let pair = SelectedCaretPair {
        start: textflow::shaping::CaretStop {
            text_offset: 0,
            position: FlowPoint::default(),
            bidi_level: 0,
        },
        start_frame: textflow::placement::CaretFrame {
            local_origin: FlowPoint {
                x: 10 << 8,
                y: 20 << 8,
            },
            unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
        },
        end: textflow::shaping::CaretStop {
            text_offset: 1,
            position: FlowPoint::default(),
            bidi_level: 0,
        },
        end_frame: textflow::placement::CaretFrame {
            local_origin: FlowPoint {
                x: 20 << 8,
                y: 20 << 8,
            },
            unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
        },
    };
    let ribbon = selection_ribbon(
        Rect::new(0, 0, 100, 40),
        Transform3D::IDENTITY,
        crate::render::font::FontMetrics {
            ascender: Fixed::from_int(7),
            descender: Fixed::from_int(-2),
            line_height: Fixed::from_int(10),
        },
        pair,
    )
    .unwrap();

    assert_eq!(
        ribbon.quad(),
        [
            Point::new(10, 13),
            Point::new(20, 13),
            Point::new(20, 23),
            Point::new(10, 23),
        ]
    );
    assert_eq!(ribbon.text_range(), 0..1);
}

#[test]
fn selection_ribbon_rejects_carets_behind_the_projective_plane() {
    let pair = SelectedCaretPair {
        start: textflow::shaping::CaretStop {
            text_offset: 0,
            position: FlowPoint::default(),
            bidi_level: 0,
        },
        start_frame: textflow::placement::CaretFrame {
            local_origin: FlowPoint {
                x: 10 << 8,
                y: 20 << 8,
            },
            unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
        },
        end: textflow::shaping::CaretStop {
            text_offset: 1,
            position: FlowPoint::default(),
            bidi_level: 0,
        },
        end_frame: textflow::placement::CaretFrame {
            local_origin: FlowPoint {
                x: 20 << 8,
                y: 20 << 8,
            },
            unit_tangent: FlowPoint { x: 1 << 8, y: 0 },
        },
    };
    let mut behind = Transform3D::IDENTITY;
    behind.m22 = -Fixed64::ONE;

    assert!(
        selection_ribbon(
            Rect::new(0, 0, 100, 40),
            behind,
            crate::render::font::FontMetrics {
                ascender: Fixed::from_int(7),
                descender: Fixed::from_int(-2),
                line_height: Fixed::from_int(10),
            },
            pair,
        )
        .is_none()
    );
}

#[test]
fn paragraph_request_preserves_overflow_policy() {
    let paragraph = ParagraphStyle {
        overflow: TextOverflow::Ellipsis,
        max_lines: Some(2),
        ..ParagraphStyle::default()
    };
    let request = paragraph.layout_request(
        "overflow",
        crate::render::font::FontMetrics {
            ascender: Fixed::from_int(7),
            descender: Fixed::from_int(-1),
            line_height: Fixed::from_int(8),
        },
        Some(Fixed::from_int(24)),
    );

    assert_eq!(request.overflow, textflow::layout::Overflow::Ellipsis);
    assert_eq!(request.max_lines, 2);
}

#[test]
fn feature_storage_is_bounded() {
    static FEATURES: [FontFeature; MAX_FONT_FEATURES] =
        [FontFeature::new(*b"kern", 1); MAX_FONT_FEATURES];
    let features = FontFeatures::borrowed(&FEATURES).unwrap();
    assert_eq!(features.as_slice().len(), MAX_FONT_FEATURES);
    assert!(features.is_borrowed());
    assert_eq!(
        FontFeatures::owned(alloc::vec![
            FontFeature::new(*b"liga", 1);
            MAX_FONT_FEATURES + 1
        ]),
        Err(FontFeatureCapacityError {
            count: MAX_FONT_FEATURES + 1
        })
    );
}

#[test]
fn language_tags_reject_ambiguous_separators() {
    assert_eq!(
        LanguageTag::parse("zh--Hans"),
        Err(LanguageTagError::EmptySubtag)
    );
    assert_eq!(LanguageTag::parse("ar-EG").unwrap().as_str(), "ar-EG");
}
