extern crate std;
use super::*;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Color, Dimension, Viewport};
use crate::ui::layout::LayoutStyle;
use crate::ui::offscreen::BufferKey;
use crate::ui::{Children, OffscreenRender, Outputs, Parent, Style, Widget, WidgetTexture};

fn spawn(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
    let e = world.spawn_empty();
    world.insert(e, Widget);
    world.insert(e, style);
    if let Some(p) = parent {
        world.insert(e, Parent(p));
        if let Some(c) = world.get_mut::<Children>(p) {
            c.0.push(e);
        } else {
            world.insert(p, Children(std::vec![e]));
        }
    }
    e
}

fn make_world() -> World {
    let mut app = crate::app::App::headless(64, 64);
    // Tests in this module exercise the offscreen-render path,
    // which needs an actual cache. App ctor leaves the pool
    // disabled (budget = 0) so the production default doesn't
    // pretend to know the right size for any given target.
    app.with_default_widgets()
        .with_offscreen_pool_budget(64 * 1024);
    app.world
}

#[test]
fn output_subtree_uses_supplied_renderer_without_committing_or_replaying() {
    struct Target {
        fills: alloc::vec::Vec<(Rect, Rect)>,
        fail: bool,
    }
    impl Renderer for Target {
        type TargetState = ();

        fn route(
            &self,
            _: &::mirui::render::DrawRequest<'_, '_>,
        ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
            Ok(::mirui::render::RenderRoute::Native)
        }

        fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            let request = &draw.request();
            let DrawCommand::Fill { area, .. } = request.command else {
                panic!("unexpected output command");
            };
            self.fills.push((*area, request.clip));
            if self.fail {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        }

        fn flush(&mut self) -> Result<(), RenderError> {
            panic!("the producer, not the subtree painter, commits the target");
        }
    }

    let mut world = make_world();
    let root = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(1, 2, 3).into()),
            layout: LayoutStyle {
                width: Dimension::px(8),
                height: Dimension::px(8),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    spawn(
        &mut world,
        Some(root),
        Style {
            bg_color: Some(Color::rgb(4, 5, 6).into()),
            layout: LayoutStyle {
                width: Dimension::px(4),
                height: Dimension::px(4),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(root, OffscreenRender::default());
    try_update_layout(&mut world, root, &Viewport::new(8, 8, Fixed::ONE)).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    let rect = Rect::new(0, 0, 8, 8);
    let key = BufferKey::for_widget(
        root,
        rect,
        Rect::new(-2, -1, 12, 10),
        OffscreenRender::default(),
        1,
    );
    for fail in [false, true] {
        let mut renderer = Target {
            fills: alloc::vec::Vec::with_capacity(2),
            fail,
        };
        let mut index = 0;
        let requests = PaintRequestScope::new(&world);
        let result = draw_output_subtree(
            &snapshot.layout_tree,
            &world,
            &snapshot.entities,
            &mut index,
            &mut renderer,
            &rect,
            key,
            None,
            &requests,
        );
        assert_eq!(
            result,
            if fail {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        );
        assert_eq!(index, if fail { 0 } else { 2 });
        assert_eq!(renderer.fills.len(), if fail { 1 } else { 2 });
        assert_eq!(
            renderer.fills[0],
            (Rect::new(2, 1, 8, 8), Rect::new(0, 0, 12, 10))
        );
        if !fail {
            assert_eq!(renderer.fills[1].0, Rect::new(2, 1, 4, 4));
        }
        assert!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(root)
                .is_none()
        );
    }
}

#[crate::component]
struct OffscreenReadProbe {
    #[depends_on(WidgetTexture)]
    source: Entity,
    fail: bool,
    calls: core::cell::Cell<usize>,
}

fn probe_offscreen_reads(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let probe = world.get::<OffscreenReadProbe>(entity).unwrap();
    if !probe.fail {
        assert!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(entity)
                .is_none(),
            "a subtree under construction must not be readable"
        );
    }
    let source = world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(probe.source)
        .expect("a completed source stays readable while another subtree renders");
    assert_eq!(
        source.texture().read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(64, 128, 255)
    );
    probe.calls.set(probe.calls.get() + 1);
    if probe.fail {
        paint.fail(RenderError::BackendFailure);
    }
}

fn offscreen_read_probe_world(fail: bool) -> (World, Entity, Entity, Entity) {
    let mut world = make_world();
    world.resource_mut::<ViewRegistry>().unwrap().insert(
        crate::ui::View::new("OffscreenReadProbe", 100, probe_offscreen_reads)
            .with_filter::<OffscreenReadProbe>(),
    );
    let panel_style = Style {
        bg_color: Some(Color::rgb(64, 128, 255).into()),
        layout: LayoutStyle {
            width: Dimension::Px(Fixed::from_int(8)),
            height: Dimension::Px(Fixed::from_int(8)),
            ..Default::default()
        },
        ..Default::default()
    };
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::px(16),
                height: Dimension::px(16),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let source = spawn(&mut world, Some(root), panel_style.clone());
    let consumer = spawn(&mut world, Some(root), panel_style);
    for entity in [source, consumer] {
        world.insert(entity, OffscreenRender::default());
    }
    world.insert(
        consumer,
        OffscreenReadProbe {
            source,
            fail,
            calls: core::cell::Cell::new(0),
        },
    );
    ViewRegistry::reconcile_observations(&mut world);
    (world, root, source, consumer)
}

#[test]
fn offscreen_subtree_reads_only_completed_sources() {
    let (mut world, root, _, consumer) = offscreen_read_probe_world(false);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut pixels = [0; 64 * 64 * 4];
    let texture =
        Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);
    let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
    super::render(&world, root, &viewport, &mut renderer).unwrap();
    assert_eq!(
        world
            .get::<OffscreenReadProbe>(consumer)
            .unwrap()
            .calls
            .get(),
        1
    );
    assert_eq!(
        world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(consumer)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        Color::rgb(64, 128, 255)
    );
}

#[test]
fn offscreen_subtree_failure_preserves_completed_sources() {
    let (mut world, root, source, consumer) = offscreen_read_probe_world(false);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut pixels = [0; 64 * 64 * 4];
    let texture =
        Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);
    let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
    super::render(&world, root, &viewport, &mut renderer).unwrap();
    super::seed_prev_rects(&mut world, root, &viewport);
    let revision = world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(source)
        .unwrap()
        .texture()
        .cache_revision;
    let probe = world.get_mut::<OffscreenReadProbe>(consumer).unwrap();
    probe.fail = true;
    probe.calls.set(0);
    world.invalidate_visual(consumer);
    let _ = super::collect_dirty_regions(&mut world, root, &viewport).unwrap();
    assert_eq!(
        super::render(&world, root, &viewport, &mut renderer),
        Err(RenderError::BackendFailure)
    );
    assert_eq!(
        world
            .get::<OffscreenReadProbe>(consumer)
            .unwrap()
            .calls
            .get(),
        1
    );
    assert!(
        world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(consumer)
            .is_none()
    );
    let source = world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .get(source)
        .expect("completed source remains cached");
    let texture = source.texture();
    assert_eq!(texture.cache_revision, revision);
    assert_eq!(
        texture.read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(64, 128, 255)
    );
}

#[test]
fn offscreen_prerender_does_not_submit_to_parent_target() {
    struct PreparationTarget;

    impl Renderer for PreparationTarget {
        type TargetState = ();

        fn route(
            &self,
            _: &::mirui::render::DrawRequest<'_, '_>,
        ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
            Ok(::mirui::render::RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            panic!("source preparation must not write to the parent target");
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }

        fn offscreen_format(&self) -> Option<ColorFormat> {
            Some(ColorFormat::RGBA8888)
        }
    }

    let (mut world, root, source, _) = offscreen_read_probe_world(false);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::try_update_layout(&mut world, root, &viewport).unwrap();
    let snapshot = world.resource::<LayoutSnapshot>().unwrap();
    for _ in 0..2 {
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let _batch = outputs.begin_batch();
        let _prepared = super::output_plan::prepare(
            &world,
            &snapshot.layout_tree,
            &snapshot.entities,
            &mut PreparationTarget,
        )
        .unwrap();
        assert_eq!(
            world
                .resource::<Outputs<WidgetTexture>>()
                .unwrap()
                .get(source)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(1, 1),
            Color::rgb(64, 128, 255)
        );
    }
}

#[test]
fn offscreen_render_publishes_completed_subtree() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    let output = outputs.get(panel).expect("completed subtree output");
    assert_eq!((output.texture().width, output.texture().height), (32, 32));
    assert_eq!(
        output.texture().read_pixels().unwrap().get_pixel(1, 1),
        Color::rgb(64, 128, 255)
    );
}

#[test]
fn offscreen_production_does_not_read_parent_framebuffer() {
    struct FailingRead;

    impl Renderer for FailingRead {
        type TargetState = ();

        fn route(
            &self,
            _: &::mirui::render::DrawRequest<'_, '_>,
        ) -> Result<::mirui::render::RenderRoute, ::mirui::render::RenderError> {
            Ok(::mirui::render::RenderRoute::Native)
        }

        fn submit(&mut self, _: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
            Ok(())
        }

        fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
            Ok(())
        }

        fn supports_offscreen(&self) -> bool {
            true
        }

        fn offscreen_format(&self) -> Option<ColorFormat> {
            Some(ColorFormat::RGBA8888)
        }

        fn read_target_region(&self, _src: &Rect, _dst: &mut Texture) -> Result<(), RenderError> {
            Err(RenderError::BackendFailure)
        }
    }

    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut FailingRead).unwrap();
    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(outputs.get(panel).is_some());

    let mut pixels = std::vec![0; 64 * 64 * 4];
    let texture =
        Texture::new(&mut pixels, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(texture);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();
    assert!(outputs.get(panel).is_some());
}

#[test]
fn offscreen_render_pixels_match_inline_render() {
    // The same panel rendered with and without OffscreenRender at
    // scale=1.0 should produce identical pixels (up to ±1 alpha
    // for blend rounding). 32×32 solid blue panel.
    let blue = Color::rgb(64, 128, 255);

    // (a) Inline: no OffscreenRender.
    let mut buf_inline = std::vec![0u8; 64 * 64 * 4];
    {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(blue.into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
    }

    // (b) Offscreen at scale=1.0.
    let mut buf_off = std::vec![0u8; 64 * 64 * 4];
    {
        let mut world = make_world();
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(blue.into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(32)),
                    height: Dimension::Px(Fixed::from_int(32)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, OffscreenRender::default());
        let tex =
            Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
    }

    // Compare a center pixel of the panel; both should be blue.
    let pi = (16 * 64 + 16) * 4;
    let po = (16 * 64 + 16) * 4;
    for c in 0..3 {
        let d = (buf_inline[pi + c] as i32 - buf_off[po + c] as i32).abs();
        assert!(
            d <= 2,
            "channel {c}: inline {} vs offscreen {} differ by {d}",
            buf_inline[pi + c],
            buf_off[po + c]
        );
    }
}

#[test]
fn offscreen_render_with_scale_half_creates_smaller_buffer() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    let output = outputs.get(panel).unwrap();
    assert_eq!((output.texture().width, output.texture().height), (16, 16));
}

/// Children inside an OffscreenRender + scale=0.5 subtree must
/// land at upscaled coordinates after blit. A 40×20 child painted
/// at logical (5, 5) inside a 128×114 panel should appear on the
/// outer framebuffer at physical (5, 5) → (45, 25) — same as
/// inline rendering. If the inner viewport.scale is not threaded
/// through, the child paints at full physical 40×20 inside a 64×57
/// buffer, then 2× upscale doubles its on-screen size to 80×40.
#[test]
fn offscreen_scale_half_child_size_matches_inline() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 255, 255).into()), // white
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(128)),
                height: Dimension::Px(Fixed::from_int(114)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

    let _child = spawn(
        &mut world,
        Some(panel),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()), // red
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(40)),
                height: Dimension::Px(Fixed::from_int(20)),
                position: crate::ui::layout::Position::Absolute,
                left: Dimension::Px(Fixed::from_int(5)),
                top: Dimension::Px(Fixed::from_int(5)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let mut buf = std::vec![0u8; 128 * 128 * 4];
    let tex = Texture::new(&mut buf, 128, 128, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let read_rgb = |x: usize, y: usize| -> (u8, u8, u8) {
        let i = (y * 128 + x) * 4;
        (buf[i], buf[i + 1], buf[i + 2])
    };
    assert_eq!(read_rgb(15, 15), (255, 0, 0), "child interior");
    assert_eq!(
        read_rgb(60, 30),
        (255, 255, 255),
        "(60, 30) should be panel white; if it's red the child \
             rendered at 2× and the inner viewport scale is broken"
    );
    assert_eq!(
        read_rgb(80, 40),
        (255, 255, 255),
        "(80, 40) should be panel white"
    );
}

/// Same fill invariant as the 32×32 RGBA8888 case but with a
/// 128×114 panel + scale=0.5 + RGB565Swapped, exercising the
/// 565 byte-swap path that the RGBA test never hits.
#[test]
fn offscreen_scale_half_form_sized_panel_fills_correctly() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(128)),
                height: Dimension::Px(Fixed::from_int(114)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

    let mut buf = std::vec![0u8; 128 * 128 * 2];
    let tex =
        Texture::new(&mut buf, 128, 128, ColorFormat::RGB565Swapped).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(128, 128, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    // RGB565Swapped: blue (0, 0, 255) packs to 565 = 0x001F. The
    // "swapped" variant byte-swaps to 0x1F00 and stores it
    // little-endian → bytes [0x00, 0x1F].
    let read_px = |x: usize, y: usize| -> [u8; 2] {
        let i = (y * 128 + x) * 2;
        [buf[i], buf[i + 1]]
    };
    for (x, y) in [(0, 0), (63, 56), (127, 0), (0, 113), (127, 113), (64, 60)] {
        let px = read_px(x, y);
        assert_eq!(
            px,
            [0x00, 0x1F],
            "panel pixel ({x}, {y}) is {px:02x?}; expected blue (RGB565Swapped little-endian)"
        );
    }
}

/// Pixel-equivalence: rendering a panel + child subtree through
/// the OffscreenRender path must produce the same framebuffer
/// pixels as rendering inline. Holds across cold cache (frame 1),
/// warm cache hit (frame 2), and generation bump (frame 3 after
/// child Dirty). Uses RGB565Swapped on a 128×128 buffer — the
/// format pairing where integer-rounding drift between inline and
/// offscreen raster paths is most likely to surface.
#[test]
fn offscreen_render_red_panel_with_child_matches_inline_across_frames() {
    use crate::ui::dirty::Dirty;
    use crate::ui::{Children, Parent, Theme};

    const FB_W: u16 = 128;
    const FB_H: u16 = 128;

    fn build_world(with_offscreen: bool) -> (World, Entity, Entity) {
        let mut world = World::default();
        // 64 KiB is plenty for a 40×20 panel + child buffer; the
        // pool default is 0 (disabled) so the test must opt in.
        world.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
        world.insert_resource(ViewRegistry::with_builtins());
        world.insert_resource(Theme::dark());

        let panel = world.spawn_empty();
        world.insert(panel, Widget);
        world.insert(
            panel,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        if with_offscreen {
            world.insert(panel, OffscreenRender::default());
        }

        let child = world.spawn_empty();
        world.insert(child, Widget);
        world.insert(child, Parent(panel));
        world.insert(panel, Children(std::vec![child]));
        world.insert(
            child,
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    position: crate::ui::layout::Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(4)),
                    top: Dimension::Px(Fixed::from_int(4)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        (world, panel, child)
    }

    let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
    let render_into = |world: &World, panel: Entity| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 2];
        let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(world, panel, &viewport, &mut renderer).unwrap();
        fb
    };

    let (mut w_inline, p_inline, c_inline) = build_world(false);
    let (mut w_off, p_off, c_off) = build_world(true);

    let fb_inline_f1 = render_into(&w_inline, p_inline);
    let fb_off_f1 = render_into(&w_off, p_off);
    assert_eq!(fb_inline_f1, fb_off_f1, "frame 1 (cold) inline ≠ offscreen");

    // Inline re-rasters every frame; offscreen path serves the
    // cached buffer. The framebuffer must still come out the same.
    let fb_inline_f2 = render_into(&w_inline, p_inline);
    let fb_off_f2 = render_into(&w_off, p_off);
    assert_eq!(fb_inline_f2, fb_off_f2, "frame 2 (warm) inline ≠ offscreen");

    // Inline ignores Dirty (always full render); offscreen path
    // walks dirty + bumps generation. Both must converge.
    w_inline.insert(c_inline, Dirty);
    w_off.insert(c_off, Dirty);
    let fb_inline_f3 = render_into(&w_inline, p_inline);
    let fb_off_f3 = render_into(&w_off, p_off);
    assert_eq!(
        fb_inline_f3, fb_off_f3,
        "frame 3 (gen bump) inline ≠ offscreen"
    );
}

/// `WidgetTransform` on the OffscreenRender entity itself must
/// reach the outer blit so the buffer ends up positioned /
/// rotated / scaled the same way as the inline render's output.
/// Three variants — pure translate, scale, rotate — each
/// rendered both inline and offscreen, asserted byte-equal.
#[test]
fn offscreen_render_with_widget_transform_matches_inline() {
    use crate::ui::widgets::transform::WidgetTransform;
    use crate::ui::{Children, Parent, Theme};

    const FB_W: u16 = 128;
    const FB_H: u16 = 128;

    fn build_world(with_offscreen: bool, tf: Transform) -> (World, Entity) {
        let mut world = World::default();
        world.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
        world.insert_resource(ViewRegistry::with_builtins());
        world.insert_resource(Theme::dark());

        let panel = world.spawn_empty();
        world.insert(panel, Widget);
        world.insert(
            panel,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(panel, WidgetTransform(tf));
        if with_offscreen {
            world.insert(panel, OffscreenRender::default());
        }

        let child = world.spawn_empty();
        world.insert(child, Widget);
        world.insert(child, Parent(panel));
        world.insert(panel, Children(std::vec![child]));
        world.insert(
            child,
            Style {
                bg_color: Some(Color::rgb(0, 0, 255).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(8)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    position: crate::ui::layout::Position::Absolute,
                    left: Dimension::Px(Fixed::from_int(4)),
                    top: Dimension::Px(Fixed::from_int(4)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        (world, panel)
    }

    let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
    let render_into = |world: &World, panel: Entity| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        let tex =
            Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(world, panel, &viewport, &mut renderer).unwrap();
        fb
    };

    // Translate / scale paths must be byte-identical: the inner
    // raster output then a transformed blit produces the same
    // pixels as raster-with-transform-applied directly. Rotate
    // is sampled twice — once when the inner SwRenderer rasters
    // into the buffer, once when the outer blit rotates the
    // buffer onto the framebuffer — so a few AA edge pixels
    // legitimately differ.
    let exact_cases = [
        (
            "translate(20, 30)",
            Transform::translate(Fixed::from_int(20), Fixed::from_int(30)),
        ),
        (
            "scale(2, 1.5)",
            Transform::scale(Fixed::from_int(2), Fixed::ONE + Fixed::ONE / 2),
        ),
    ];
    for (name, tf) in exact_cases {
        let (w_inline, p_inline) = build_world(false, tf);
        let (w_off, p_off) = build_world(true, tf);
        let fb_inline = render_into(&w_inline, p_inline);
        let fb_off = render_into(&w_off, p_off);
        let diff = fb_inline
            .iter()
            .zip(&fb_off)
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            diff, 0,
            "WidgetTransform({name}) on OffscreenRender ≠ inline ({diff} bytes diff)"
        );
    }

    let rotate_tf = Transform::rotate_deg(Fixed::from_int(15));
    let (w_inline, p_inline) = build_world(false, rotate_tf);
    let (w_off, p_off) = build_world(true, rotate_tf);
    let fb_inline = render_into(&w_inline, p_inline);
    let fb_off = render_into(&w_off, p_off);
    let diff = fb_inline
        .iter()
        .zip(&fb_off)
        .filter(|(a, b)| a != b)
        .count();
    // 40×20 panel rotated 15° has ~50 AA edge pixels × 4 bytes
    // each = ~200 bytes that legitimately resample differently.
    // 256 leaves headroom; if the fix regresses the diff jumps
    // into the thousands.
    assert!(
        diff < 256,
        "WidgetTransform(rotate(15deg)) on OffscreenRender drifts {diff} bytes from inline"
    );
}

/// Transparent capture and direct RGB565 drawing differ by at most one channel quantization step in this fixture.
#[test]
fn offscreen_render_form_page_subtree_matches_inline() {
    use crate::ui::layout::{AlignItems, FlexDirection, Padding};
    use crate::ui::theme::ColorToken;
    use crate::ui::widgets::{ProgressBar, Slider, Switch, Text};
    use crate::ui::{Children, Parent, Theme};

    const FB_W: u16 = 128;
    const FB_H: u16 = 128;

    fn add_child(world: &mut World, parent: Entity, child: Entity) {
        world.insert(child, Parent(parent));
        if let Some(c) = world.get_mut::<Children>(parent) {
            c.0.push(child);
        } else {
            world.insert(parent, Children(std::vec![child]));
        }
    }

    fn spawn_styled(world: &mut World, parent: Option<Entity>, style: Style) -> Entity {
        let e = world.spawn_empty();
        world.insert(e, Widget);
        world.insert(e, style);
        if let Some(p) = parent {
            add_child(world, p, e);
        }
        e
    }

    #[derive(Clone, Copy)]
    enum Mark {
        None,
        OnPanel,
        OnSwitch,
    }

    fn build(mark: Mark) -> (World, Entity, Entity, Entity) {
        let mut w = World::default();
        // 64 KiB covers the form_page + Switch buffer working
        // set; the pool default is 0 (disabled) so the test
        // must opt in.
        w.insert_resource(Outputs::<WidgetTexture>::with_budget(64 * 1024));
        w.insert_resource(ViewRegistry::with_builtins());
        w.insert_resource(Theme::dark());

        let fp = spawn_styled(
            &mut w,
            None,
            Style {
                bg_color: Some(ColorToken::Surface.into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(128)),
                    height: Dimension::Px(Fixed::from_int(114)),
                    direction: FlexDirection::Column,
                    padding: Padding::all(Dimension::Px(Fixed::from_int(10))),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        if matches!(mark, Mark::OnPanel) {
            w.insert(fp, OffscreenRender::default());
        }
        let er = spawn_styled(
            &mut w,
            Some(fp),
            Style {
                layout: LayoutStyle {
                    direction: FlexDirection::Row,
                    height: Dimension::Px(Fixed::from_int(28)),
                    align: AlignItems::Center,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let el = spawn_styled(
            &mut w,
            Some(er),
            Style {
                text_color: Some(ColorToken::OnSurface.into()),
                layout: LayoutStyle {
                    grow: Fixed::ONE,
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        w.insert(el, Text::from("Enable"));
        let sw = spawn_styled(
            &mut w,
            Some(er),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        w.insert(sw, Switch::new());
        if matches!(mark, Mark::OnSwitch) {
            w.insert(sw, OffscreenRender::default());
        }
        let sr = spawn_styled(
            &mut w,
            Some(fp),
            Style {
                layout: LayoutStyle {
                    height: Dimension::Px(Fixed::from_int(14)),
                    padding: Padding {
                        top: Dimension::Px(Fixed::from_int(6)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let sl = spawn_styled(
            &mut w,
            Some(sr),
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(108)),
                    height: Dimension::Px(Fixed::from_int(14)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        w.insert(sl, Slider::new(Fixed::ZERO, Fixed::from_int(100)));
        let pr = spawn_styled(
            &mut w,
            Some(fp),
            Style {
                layout: LayoutStyle {
                    height: Dimension::Px(Fixed::from_int(10)),
                    padding: Padding {
                        top: Dimension::Px(Fixed::from_int(8)),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let pb = spawn_styled(
            &mut w,
            Some(pr),
            Style {
                border_radius: Fixed::from_int(4),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(108)),
                    height: Dimension::Px(Fixed::from_int(8)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        w.insert(pb, ProgressBar::new());
        (w, fp, sw, sl)
    }

    fn assert_frame(a: Vec<u8>, b: Vec<u8>, label: &str) {
        assert_eq!(a.len(), b.len());
        for (pixel, (a, b)) in a.chunks_exact(2).zip(b.chunks_exact(2)).enumerate() {
            let a = u16::from_be_bytes([a[0], a[1]]);
            let b = u16::from_be_bytes([b[0], b[1]]);
            for (shift, mask) in [(11, 31), (5, 63), (0, 31)] {
                let error = ((a >> shift) & mask).abs_diff((b >> shift) & mask);
                assert!(
                    error <= 1,
                    "{label}: pixel {pixel}, shift {shift}, error {error}"
                );
            }
        }
    }

    let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
    // Mirror render_dirty so the walker bumps OffscreenGeneration
    // when the subtree mutates between frames.
    let render_into = |world: &mut World, root: Entity| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 2];
        let tex = Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let dirty = super::collect_dirty_region(world, root, &viewport)
            .unwrap()
            .unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(FB_W as i32),
                h: Fixed::from_int(FB_H as i32),
            });
        super::render_region(world, root, &viewport, &dirty, &mut renderer).unwrap();
        fb
    };

    let (mut w_ref, fp_ref, sw_ref, sl_ref) = build(Mark::None);
    let (mut w_a, fp_a, sw_a, sl_a) = build(Mark::OnPanel);
    let (mut w_b, fp_b, sw_b, sl_b) = build(Mark::OnSwitch);

    w_ref.mark_subtree_dirty(fp_ref);
    w_a.mark_subtree_dirty(fp_a);
    w_b.mark_subtree_dirty(fp_b);

    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_a, fp_a),
        "form_page OffscreenRender frame 1 ≠ inline",
    );
    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_b, fp_b),
        "Switch OffscreenRender frame 1 ≠ inline",
    );

    // Mark the whole subtree so the inline path's dirty bound
    // equals the offscreen path's (which always promotes to the
    // OffscreenRender entity's full rect).
    let flip_on = |w: &mut World, fp: Entity, sw: Entity| {
        if let Some(s) = w.get_mut::<Switch>(sw) {
            s.on = true;
        }
        w.mark_subtree_dirty(fp);
    };
    flip_on(&mut w_ref, fp_ref, sw_ref);
    flip_on(&mut w_a, fp_a, sw_a);
    flip_on(&mut w_b, fp_b, sw_b);
    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_a, fp_a),
        "form_page OffscreenRender frame 2 (switch on) ≠ inline",
    );
    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_b, fp_b),
        "Switch OffscreenRender frame 2 (switch on) ≠ inline",
    );

    let slider_max = |w: &mut World, fp: Entity, sl: Entity| {
        if let Some(s) = w.get_mut::<Slider>(sl) {
            s.value = s.max;
        }
        w.mark_subtree_dirty(fp);
    };
    slider_max(&mut w_ref, fp_ref, sl_ref);
    slider_max(&mut w_a, fp_a, sl_a);
    slider_max(&mut w_b, fp_b, sl_b);
    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_a, fp_a),
        "form_page OffscreenRender frame 3 (slider max) ≠ inline",
    );
    assert_frame(
        render_into(&mut w_ref, fp_ref),
        render_into(&mut w_b, fp_b),
        "Switch OffscreenRender frame 3 (slider max) ≠ inline",
    );
}

/// Same single-Switch reproducer but on RGB565Swapped instead of
/// RGBA8888 — exercises the format-specific paths the RGBA test
/// can't reach.
#[test]
fn offscreen_render_on_single_switch_widget_matches_inline_rgb565sw() {
    use crate::ui::widgets::Switch;

    let pix_at = |buf: &[u8], x: usize, y: usize| -> [u8; 2] {
        let i = (y * 128 + x) * 2;
        [buf[i], buf[i + 1]]
    };

    // (a) Inline Switch on RGB565Swapped 128×128 framebuffer.
    let mut buf_inline = std::vec![0u8; 128 * 128 * 2];
    {
        let mut world = make_world();
        let switch = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(switch, Switch::new());
        let tex = Texture::new(&mut buf_inline, 128, 128, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, switch, &viewport, &mut renderer).unwrap();
    }

    let mut buf_off = std::vec![0u8; 128 * 128 * 2];
    {
        let mut world = make_world();
        let switch = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(switch, Switch::new());
        world.insert(switch, OffscreenRender::default());
        let tex = Texture::new(&mut buf_off, 128, 128, ColorFormat::RGB565Swapped)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(128, 128, Fixed::ONE);
        super::render(&world, switch, &viewport, &mut renderer).unwrap();
    }

    for &(x, y) in [(0, 0), (10, 5), (20, 10), (30, 15), (35, 18)].iter() {
        let i = pix_at(&buf_inline, x, y);
        let o = pix_at(&buf_off, x, y);
        assert_eq!(
            i, o,
            "pixel ({x}, {y}): inline {i:02x?} vs offscreen {o:02x?}"
        );
    }
}

/// Single Switch widget marked OffscreenRender::default(). Inline
/// rendering paints the switch at the widget's logical rect;
/// offscreen rendering must produce a pixel-identical result on
/// the outer framebuffer.
#[test]
fn offscreen_render_on_single_switch_widget_matches_inline() {
    use crate::ui::widgets::Switch;

    let blue_at = |buf: &[u8], x: usize, y: usize| -> (u8, u8, u8) {
        let i = (y * 64 + x) * 4;
        (buf[i], buf[i + 1], buf[i + 2])
    };

    // (a) Inline Switch.
    let mut buf_inline = std::vec![0u8; 64 * 64 * 4];
    {
        let mut world = make_world();
        let switch = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(switch, Switch::new());
        let tex = Texture::new(&mut buf_inline, 64, 64, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, switch, &viewport, &mut renderer).unwrap();
    }

    // (b) Offscreen Switch.
    let mut buf_off = std::vec![0u8; 64 * 64 * 4];
    {
        let mut world = make_world();
        let switch = spawn(
            &mut world,
            None,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(40)),
                    height: Dimension::Px(Fixed::from_int(20)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(switch, Switch::new());
        world.insert(switch, OffscreenRender::default());
        let tex =
            Texture::new(&mut buf_off, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        let viewport = Viewport::new(64, 64, Fixed::ONE);
        super::render(&world, switch, &viewport, &mut renderer).unwrap();
    }

    // Sample a handful of pixels across the 40×20 switch rect.
    // Both paths must produce the same colours within blending
    // rounding.
    for &(x, y) in [(0, 0), (10, 5), (20, 10), (30, 15), (35, 18)].iter() {
        let i = blue_at(&buf_inline, x, y);
        let o = blue_at(&buf_off, x, y);
        for ch in 0..3 {
            let inline_c = [i.0, i.1, i.2][ch];
            let off_c = [o.0, o.1, o.2][ch];
            let d = inline_c.abs_diff(off_c);
            assert!(
                d <= 2,
                "pixel ({x}, {y}) ch {ch}: inline {inline_c} vs offscreen {off_c} differ by {d}\n\
                     full pixel: inline {i:?}, offscreen {o:?}",
            );
        }
    }
}

/// Half-scale capture covers the full logical panel before composition.
#[test]
fn offscreen_scale_half_fills_buffer_and_blits_upscaled() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::with_scale(Fixed::ONE / 2));

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    // After render: outer 64×64 framebuffer's (0..32, 0..32) region
    // should be solid red. The buffer is internally 16×16 but the
    // blit upscales 2×.
    let read_px = |x: usize, y: usize| -> (u8, u8, u8, u8) {
        let i = (y * 64 + x) * 4;
        (buf[i], buf[i + 1], buf[i + 2], buf[i + 3])
    };
    for (x, y) in [(0, 0), (15, 15), (16, 0), (0, 16), (31, 31), (10, 20)] {
        let (r, g, b, _) = read_px(x, y);
        assert_eq!(
            (r, g, b),
            (255, 0, 0),
            "panel pixel ({x}, {y}) is ({r}, {g}, {b}); expected red"
        );
    }
    // Outside the panel's logical rect must still be untouched
    // (zeroed-out backing buffer).
    for (x, y) in [(32, 0), (0, 32), (50, 50), (63, 63)] {
        let (r, g, b, _) = read_px(x, y);
        assert_eq!(
            (r, g, b),
            (0, 0, 0),
            "outside-panel pixel ({x}, {y}) leaked colour ({r}, {g}, {b})"
        );
    }
}

// Threading test: validates off.opacity reaches the outer blit
// cmd's opa byte. Half-red panel proves the plumb without a
// per-backend integration harness.
#[test]
fn offscreen_opacity_dims_outer_blit() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::new().with_opacity(128));

    let mut buf = std::vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let read_r = |x: usize, y: usize| buf[(y * 32 + x) * 4];
    for (x, y) in [(0, 0), (7, 7), (15, 15), (3, 12)] {
        let r = read_r(x, y);
        assert!(
            (120..=135).contains(&r),
            "panel pixel ({x},{y}) red = {r}; expected ~128 for opacity=128",
        );
    }
    // Outside the panel must stay zero — opacity must NOT bleed
    // into untouched pixels.
    for (x, y) in [(16, 0), (0, 16), (20, 20), (31, 31)] {
        let r = read_r(x, y);
        assert_eq!(r, 0, "outside-panel ({x},{y}) leaked red={r}");
    }
}

#[test]
fn offscreen_reuses_current_until_input_generation_changes() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 255, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);

    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let first_version = world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .state(panel)
        .unwrap()
        .current
        .unwrap()
        .version;

    let mut buf2 = std::vec![0u8; 64 * 64 * 4];
    let tex2 = Texture::new(&mut buf2, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer2 = SwRenderer::new(tex2);
    super::render(&world, panel, &viewport, &mut renderer2).unwrap();

    {
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert_eq!(
            outputs.state(panel).unwrap().current.unwrap().version,
            first_version
        );
        assert_eq!(
            outputs
                .get(panel)
                .unwrap()
                .texture()
                .read_pixels()
                .unwrap()
                .get_pixel(1, 1),
            Color::rgb(0, 255, 0)
        );
    }
    world.get_mut::<Style>(panel).unwrap().bg_color = Some(Color::rgb(255, 0, 255).into());
    world.insert(panel, super::super::OffscreenGeneration(1));
    super::render(&world, panel, &viewport, &mut renderer2).unwrap();
    assert!(
        world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .state(panel)
            .unwrap()
            .current
            .unwrap()
            .version
            > first_version
    );
    assert_eq!((buf2[0], buf2[1], buf2[2]), (255, 0, 255));
}

/// Composition culling uses the transformed output bounds.
#[test]
fn offscreen_render_with_in_bounds_transform_still_renders() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    const FB_W: u16 = 64;
    const FB_H: u16 = 64;

    let mut app = crate::app::App::headless(FB_W, FB_H);
    app.with_default_widgets()
        .with_offscreen_pool_budget(64 * 1024);
    let mut world = app.world;

    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 255, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
    let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
    {
        let tex =
            Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(&world, panel, &viewport, &mut renderer).unwrap();
    }
    super::seed_prev_rects(&mut world, panel, &viewport);

    world.insert(
        panel,
        WidgetTransform(Transform::translate(
            Fixed::from_int(20),
            Fixed::from_int(10),
        )),
    );
    world.insert(panel, Dirty);

    let dirty = super::collect_dirty_region(&mut world, panel, &viewport)
        .unwrap()
        .expect("dirty region");
    let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
    {
        let tex =
            Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render_region(&world, panel, &viewport, &dirty, &mut renderer).unwrap();
    }

    let i = (15 * FB_W as usize + 25) * 4;
    assert_eq!(
        (fb[i], fb[i + 1], fb[i + 2]),
        (0, 255, 0),
        "panel translated to (20,10) must show green at sample (25,15), got ({},{},{})",
        fb[i],
        fb[i + 1],
        fb[i + 2],
    );
}

/// A partially offscreen output still paints its visible transformed region.
#[test]
fn offscreen_render_with_transform_translating_partially_off_screen_still_renders() {
    use crate::types::Transform;
    use crate::ui::dirty::Dirty;
    use crate::ui::widgets::transform::WidgetTransform;

    const FB_W: u16 = 64;
    const FB_H: u16 = 64;

    let mut app = crate::app::App::headless(FB_W, FB_H);
    app.with_default_widgets()
        .with_offscreen_pool_budget(64 * 1024);
    let mut world = app.world;

    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    let viewport = Viewport::new(FB_W, FB_H, Fixed::ONE);
    let render_full = |world: &World| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        let tex =
            Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(world, panel, &viewport, &mut renderer).unwrap();
        fb
    };
    let render_dirty = |world: &mut World| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; FB_W as usize * FB_H as usize * 4];
        let dirty = super::collect_dirty_region(world, panel, &viewport)
            .unwrap()
            .unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(FB_W as i32),
                h: Fixed::from_int(FB_H as i32),
            });
        let tex =
            Texture::new(&mut fb, FB_W, FB_H, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render_region(world, panel, &viewport, &dirty, &mut renderer).unwrap();
        fb
    };

    let _ = render_full(&world);
    super::seed_prev_rects(&mut world, panel, &viewport);

    world.insert(
        panel,
        WidgetTransform(Transform::translate(Fixed::from_int(-16), Fixed::ZERO)),
    );
    world.insert(panel, Dirty);
    let fb = render_dirty(&mut world);

    let i = (10 * FB_W as usize + 4) * 4;
    assert_eq!(
        (fb[i], fb[i + 1], fb[i + 2]),
        (0, 0, 255),
        "panel at translated position must show blue, got ({},{},{})",
        fb[i],
        fb[i + 1],
        fb[i + 2],
    );
}

/// Oversized panel + tiny pool budget rejects the buffer; caller
/// must fall through to inline raster instead of leaving the entity
/// unpainted. Compare byte-byte against an inline-only render.
#[test]
fn offscreen_render_falls_through_to_inline_when_buffer_oversized() {
    fn build(with_offscreen: bool, budget: usize) -> (World, Entity) {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets()
            .with_offscreen_pool_budget(budget);
        let mut world = app.world;
        let panel = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 200, 100).into()),
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(48)),
                    height: Dimension::Px(Fixed::from_int(48)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        if with_offscreen {
            world.insert(panel, OffscreenRender::default());
        }
        (world, panel)
    }

    let (w_inline, p_inline) = build(false, 64 * 1024);
    // 48×48×4 = 9216 bytes; 1 KiB budget can't host it.
    let (w_off, p_off) = build(true, 1024);

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let render = |world: &World, root: Entity| -> alloc::vec::Vec<u8> {
        let mut fb = std::vec![0u8; 64 * 64 * 4];
        let tex =
            Texture::new(&mut fb, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render(world, root, &viewport, &mut renderer).unwrap();
        fb
    };

    assert_eq!(
        render(&w_inline, p_inline),
        render(&w_off, p_off),
        "oversized rejection must fall through to inline raster"
    );
}

#[test]
fn offscreen_render_reuses_buffer_across_frames_when_unchanged() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 255, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    super::render(&world, panel, &viewport, &mut renderer).unwrap();
    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    let version = outputs.state(panel).unwrap().current.unwrap().version;
    let pixels = outputs
        .get(panel)
        .unwrap()
        .texture()
        .read_pixels()
        .unwrap()
        .bytes()
        .as_ptr();
    super::render(&world, panel, &viewport, &mut renderer).unwrap();
    assert_eq!(
        outputs.state(panel).unwrap().current.unwrap().version,
        version
    );
    assert_eq!(
        outputs
            .get(panel)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .bytes()
            .as_ptr(),
        pixels
    );
}

/// `App::snapshot_widget` returns an owned RGBA8888 texture without leaving a temporary output request behind.
#[test]
fn snapshot_widget_does_not_pollute_source_components() {
    use crate::app::App;
    use crate::types::{Color, Dimension};
    use crate::ui::{OffscreenRender, Widget};

    let mut app = App::headless(64, 64);
    app.with_default_widgets()
        .with_default_systems()
        .with_offscreen_pool_budget(64 * 1024);

    let panel = app.world.spawn_empty();
    app.world.insert(panel, Widget);
    app.world.insert(
        panel,
        Style {
            bg_color: Some(Color::rgb(0, 200, 100).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    app.set_root(panel);

    assert!(app.world.get::<OffscreenRender>(panel).is_none());

    let snap = app
        .snapshot_widget(panel)
        .unwrap()
        .expect("snapshot should produce an owned texture");
    assert_eq!(snap.width, 16);
    assert_eq!(snap.height, 16);

    assert!(app.world.get::<OffscreenRender>(panel).is_none());
    assert!(crate::ui::offscreen::config(&app.world, panel).is_none());
}

#[test]
fn offscreen_sources_retain_transparent_outputs_across_backend_formats() {
    use super::super::offscreen::OffscreenGeneration;

    for (transparent_first, backend_format) in [
        (false, ColorFormat::RGB565),
        (true, ColorFormat::RGB565),
        (false, ColorFormat::RGB565Swapped),
        (true, ColorFormat::RGB565Swapped),
        (false, ColorFormat::RGBA8888),
        (true, ColorFormat::RGBA8888),
    ] {
        let mut world = make_world();
        let transparent = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(255, 0, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(8),
                    height: Dimension::px(8),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let opaque = spawn(
            &mut world,
            None,
            Style {
                bg_color: Some(Color::rgb(0, 255, 0).into()),
                layout: LayoutStyle {
                    width: Dimension::px(8),
                    height: Dimension::px(8),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(transparent, OffscreenRender::default());
        world.insert(opaque, OffscreenRender::default());
        let order = if transparent_first {
            [transparent, opaque]
        } else {
            [opaque, transparent]
        };
        {
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            outputs.reserve(2).unwrap();
            for entity in order {
                outputs.set_retained(entity, true).unwrap();
            }
        }
        let viewport = Viewport::new(8, 8, Fixed::ONE);
        let mut target = [0_u8; 8 * 8 * 4];
        let texture = Texture::new(&mut target, 8, 8, backend_format).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(texture);
        for generation in 0..2 {
            for entity in order {
                world.insert(entity, OffscreenGeneration(generation));
            }
            let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
            let _batch = outputs.begin_batch();
            for entity in order {
                super::render(&world, entity, &viewport, &mut renderer).unwrap();
            }
            for (entity, format) in [
                (transparent, ColorFormat::RGBA8888),
                (opaque, ColorFormat::RGBA8888),
            ] {
                let current = world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .get(entity)
                    .expect("source-specific current buffer");
                assert_eq!(current.texture().format, format);
                assert_eq!(
                    current.texture().read_pixels().unwrap().get_pixel(1, 1),
                    if entity == transparent {
                        Color::rgb(255, 0, 0)
                    } else {
                        Color::rgb(0, 255, 0)
                    }
                );
                let previous = world
                    .resource::<Outputs<WidgetTexture>>()
                    .unwrap()
                    .previous(entity);
                assert_eq!(previous.is_some(), generation != 0);
                if let Some(previous) = previous {
                    assert_eq!(previous.texture().format, format);
                }
            }
        }
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for entity in order {
                    assert!(
                        world
                            .resource::<Outputs<WidgetTexture>>()
                            .unwrap()
                            .get(entity)
                            .is_some()
                    );
                    assert!(
                        world
                            .resource::<Outputs<WidgetTexture>>()
                            .unwrap()
                            .previous(entity)
                            .is_some()
                    );
                }
            }),
            0
        );
    }
}

/// Successful retained outputs use two slots while failed or skipped production does not advance history.
#[test]
fn retained_history_reads_successful_outputs_and_reuses_two_slots() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(8)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());

    {
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        outputs.reserve(1).unwrap();
        outputs.set_retained(panel, true).unwrap();
    }

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    let render_frame = |world: &mut World, buf: &mut [u8]| {
        world.mark_subtree_dirty(panel);
        let dirty = super::collect_dirty_region(world, panel, &viewport)
            .unwrap()
            .unwrap_or(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(64),
                h: Fixed::from_int(64),
            });
        let tex = Texture::new(buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(tex);
        super::render_region(world, panel, &viewport, &dirty, &mut renderer).unwrap();
    };

    let mut first_pixels = core::ptr::null();
    let mut prior_revision = 0;
    let colors = [
        Color::rgb(255, 0, 0),
        Color::rgb(0, 255, 0),
        Color::rgb(0, 0, 255),
    ];
    for (index, color) in colors.into_iter().enumerate() {
        world.get_mut::<Style>(panel).unwrap().bg_color = Some(color.into());
        render_frame(&mut world, &mut buf);
        let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
        let current = outputs.get(panel).unwrap();
        assert_eq!(
            current.texture().read_pixels().unwrap().get_pixel(1, 1),
            color
        );
        assert!(current.texture().cache_revision > prior_revision);
        assert!(!current.texture().transient);
        prior_revision = current.texture().cache_revision;
        if index == 0 {
            first_pixels = current.texture().read_pixels().unwrap().bytes().as_ptr();
            assert!(outputs.previous(panel).is_none());
        } else {
            let previous = outputs.previous(panel).unwrap();
            assert_eq!(
                previous.texture().read_pixels().unwrap().get_pixel(1, 1),
                colors[index - 1]
            );
            if index == 1 {
                assert_eq!(
                    previous.texture().read_pixels().unwrap().bytes().as_ptr(),
                    first_pixels
                );
                assert_ne!(
                    current.texture().read_pixels().unwrap().bytes().as_ptr(),
                    first_pixels
                );
            } else {
                assert_eq!(
                    current.texture().read_pixels().unwrap().bytes().as_ptr(),
                    first_pixels
                );
            }
        }
    }
}

#[test]
fn dirty_walker_promotes_subtree_dirty_to_offscreen_entity() {
    use super::super::dirty::Dirty;
    use super::super::offscreen::OffscreenGeneration;

    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    let child = spawn(
        &mut world,
        Some(panel),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(8)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    // Marking the child Dirty must (1) clear the child's marker,
    // (2) bump the offscreen entity's generation, (3) end up with
    // the offscreen entity's full rect in the dirty bounds — verify
    // that last bit indirectly via the buffer cache: a fresh render
    // after the bump should miss cache and re-insert.
    world.insert(child, Dirty);
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    // Run the dirty walker and grab the bounds.
    let bounds = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
    assert!(bounds.is_some(), "dirty walker should report some bounds");

    // Child's Dirty marker is cleared.
    assert!(
        world.get::<Dirty>(child).is_none(),
        "child Dirty should be cleared after offscreen subtree scan"
    );

    // Offscreen entity's generation has advanced.
    let g = world.get::<OffscreenGeneration>(panel).map(|gn| gn.0);
    assert_eq!(g, Some(1), "generation should bump from 0 to 1");
}

/// Self-Dirty case: the OffscreenRender entity itself goes Dirty
/// (e.g. theme rotation calls `world.mark_subtree_dirty(root)` which
/// stamps the entire tree). For an entity without children — a
/// single-widget marker, like a Switch — the subtree scan finds
/// no Dirty descendants and the old code skipped the
/// generation bump entirely, so the cache kept handing out the
/// stale buffer in the previous theme's colours.
#[test]
fn dirty_walker_bumps_generation_when_only_offscreen_entity_itself_is_dirty() {
    use super::super::dirty::Dirty;
    use super::super::offscreen::OffscreenGeneration;

    let mut world = make_world();
    let leaf = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 200, 100).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(20)),
                height: Dimension::Px(Fixed::from_int(10)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(leaf, OffscreenRender::default());
    let viewport = Viewport::new(64, 64, Fixed::ONE);

    // Frame 1 cold — first dirty walk (no PaintedCoverage yet).
    world.insert(leaf, Dirty);
    let _ = super::collect_dirty_region(&mut world, leaf, &viewport).unwrap();
    let g1 = world
        .get::<OffscreenGeneration>(leaf)
        .map(|g| g.0)
        .unwrap_or(0);
    assert_eq!(g1, 1, "frame 1 must bump from 0 → 1");

    // Frame 2 — only the offscreen entity itself is Dirty (no
    // children). The cache buffer is stale; generation must bump.
    world.insert(leaf, Dirty);
    let bounds = super::collect_dirty_region(&mut world, leaf, &viewport).unwrap();
    let g2 = world
        .get::<OffscreenGeneration>(leaf)
        .map(|g| g.0)
        .unwrap_or(0);
    assert_eq!(
        g2, 2,
        "frame 2 self-Dirty must bump from 1 → 2 so the buffer cache misses"
    );
    assert!(bounds.is_some(), "frame 2 self-Dirty must report bounds");
}

/// Multi-frame regression: the OffscreenRender entity's bounds
/// must end up in the dirty region every time a descendant goes
/// Dirty, not just the first time. The previous code path made
/// the rect-promotion conditional on `PaintedCoverage.is_none()` — true
/// only on the very first dirty walk for a given entity — so the
/// second time a child went Dirty the offscreen entity dropped
/// out of the dirty bounds entirely, render_region had nothing
/// to paint, and the offscreen subtree visibly froze on the
/// previous frame's pixels.
#[test]
fn dirty_walker_promotes_subtree_dirty_on_every_frame() {
    use super::super::dirty::{Dirty, PaintedCoverage};
    use super::super::offscreen::OffscreenGeneration;

    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    let child = spawn(
        &mut world,
        Some(panel),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(8)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let viewport = Viewport::new(64, 64, Fixed::ONE);

    // Frame 1: collection promotes the panel; only paint confirmation records its bounds.
    world.insert(child, Dirty);
    let b1 = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
    assert!(b1.is_some(), "frame 1 must report bounds");
    let g1 = world
        .get::<OffscreenGeneration>(panel)
        .map(|g| g.0)
        .unwrap_or(0);
    assert_eq!(g1, 1, "frame 1 generation = 1");
    assert!(world.get::<PaintedCoverage>(panel).is_none());
    super::seed_prev_rects(&mut world, panel, &viewport);
    assert!(
        world.get::<PaintedCoverage>(panel).is_some(),
        "panel PaintedCoverage set"
    );

    // Frame 2: child goes Dirty again. Walker must bump generation
    // a second time AND include the panel rect in bounds. If it
    // doesn't, render_region won't repaint the offscreen entity
    // and the screen freezes.
    world.insert(child, Dirty);
    let b2 = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();
    let g2 = world
        .get::<OffscreenGeneration>(panel)
        .map(|g| g.0)
        .unwrap_or(0);
    assert_eq!(g2, 2, "frame 2 generation = 2");
    assert!(
        b2.is_some(),
        "frame 2 must still report bounds covering the offscreen entity"
    );
    let b2 = b2.unwrap();
    // Panel logical rect is (0, 0, 32, 32) — bounds must contain it.
    assert!(
        b2.x.to_int() <= 0
            && b2.y.to_int() <= 0
            && (b2.x + b2.w).to_int() >= 32
            && (b2.y + b2.h).to_int() >= 32,
        "frame 2 bounds {b2:?} must cover the panel rect (0, 0, 32, 32)"
    );
}

#[test]
fn dirty_walker_no_bump_when_subtree_clean() {
    use super::super::offscreen::OffscreenGeneration;

    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    spawn(
        &mut world,
        Some(panel),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(8)),
                height: Dimension::Px(Fixed::from_int(8)),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let _ = super::collect_dirty_region(&mut world, panel, &viewport).unwrap();

    let generation = world.get::<OffscreenGeneration>(panel).unwrap().0;
    assert!(
        super::collect_dirty_region(&mut world, panel, &viewport)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        world.get::<OffscreenGeneration>(panel).unwrap().0,
        generation
    );
    super::seed_prev_rects(&mut world, panel, &viewport);
    assert!(
        super::collect_dirty_region(&mut world, panel, &viewport)
            .unwrap()
            .is_none()
    );
}

#[test]
fn offscreen_render_accepts_projective_composition() {
    use crate::types::Transform3D;
    use crate::ui::widgets::transform_3d::WidgetTransform3D;
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 0, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    world.insert(panel, WidgetTransform3D(Transform3D::IDENTITY));

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();
}

#[test]
fn offscreen_render_composes_nested_outputs() {
    let mut world = make_world();
    let outer = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(outer, OffscreenRender::default());
    let inner = spawn(
        &mut world,
        Some(outer),
        Style {
            bg_color: Some(Color::rgb(255, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(inner, OffscreenRender::default());

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, outer, &viewport, &mut renderer).unwrap();
}

#[test]
fn buffer_key_struct_layout() {
    // Sanity that BufferKey parts compose as expected for stable hashing.
    let k = BufferKey {
        entity: Entity {
            id: 1,
            generation: 0,
        },
        w: 32,
        h: 32,
        format: ColorFormat::RGBA8888,
        generation: 0,
        layout_rect: Rect::new(0, 0, 32, 32),
        local_bounds: Rect::new(0, 0, 32, 32),
        scale: Fixed::ONE,
    };
    let same = BufferKey {
        entity: Entity {
            id: 1,
            generation: 0,
        },
        w: 32,
        h: 32,
        format: ColorFormat::RGBA8888,
        generation: 0,
        layout_rect: Rect::new(0, 0, 32, 32),
        local_bounds: Rect::new(0, 0, 32, 32),
        scale: Fixed::ONE,
    };
    assert_eq!(k, same);
}

#[test]
fn offscreen_render_fractional_scale_rounds_buffer_dims() {
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(255, 255, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    // Fixed::ONE * 7 / 10 = 0.7 in Q24.8.
    world.insert(panel, OffscreenRender::with_scale(Fixed::ONE * 7 / 10));

    let mut buf = std::vec![0u8; 64 * 64 * 4];
    let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    let output = outputs.get(panel).unwrap();
    assert_eq!((output.texture().width, output.texture().height), (23, 23));
}

#[test]
fn hidden_entity_skips_offscreen_path() {
    // An OffscreenRender entity that's also Hidden should not
    // create a buffer — collect_entities_preorder skips Hidden
    // entirely so it never requests a pixel slot.
    use crate::ui::Hidden;
    let mut world = make_world();
    let panel = spawn(
        &mut world,
        None,
        Style {
            bg_color: Some(Color::rgb(0, 0, 0).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(16)),
                height: Dimension::Px(Fixed::from_int(16)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, OffscreenRender::default());
    world.insert(panel, Hidden);

    let mut buf = std::vec![0u8; 32 * 32 * 4];
    let tex = Texture::new(&mut buf, 32, 32, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(32, 32, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    assert!(outputs.get(panel).is_none());
    assert!(outputs.state(panel).is_none());
}

/// A mirror reads a later sibling's declared source during the first composition.
#[test]
fn mirror_of_paints_into_its_own_rect() {
    use crate::prelude::*;
    use crate::surface::FramebufferAccess;
    use crate::ui::widgets::MirrorOf;

    #[crate::compose]
    fn mirrored_panel() {
        ui! {
            View (width: 32, height: 32, bg_color: Color::rgb(0, 0, 0)) {
                View (
                    id: "mirror",
                    position: Position::Absolute,
                    left: 0,
                    top: 16,
                    width: 16,
                    height: 16
                ) [
                    MirrorOf::new(id("mirror_source")),
                ]
                View (
                    id: "mirror_source",
                    position: Position::Absolute,
                    left: 0,
                    top: 0,
                    width: 16,
                    height: 16,
                    bg_color: Color::rgb(255, 0, 0)
                )
            }
        };
    }

    let mut app = App::headless(32, 32);
    app.with_default_widgets()
        .with_offscreen_pool_budget(64 * 1024);
    let root = app.spawn_root().id();
    app.compose(root, mirrored_panel());
    app.render().unwrap();

    let source = app.world.find_by_id("mirror_source").unwrap();
    assert!(crate::ui::offscreen::texture_requested(&app.world, source));
    assert!(app.world.get::<OffscreenRender>(source).is_none());
    assert_eq!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .unwrap()
            .texture()
            .read_pixels()
            .unwrap()
            .get_pixel(1, 1),
        Color::rgb(255, 0, 0)
    );
    let mut backend = app.into_backend();
    let pixels = backend.framebuffer();
    assert_eq!(
        pixels.read_pixels().unwrap().get_pixel(1, 17),
        Color::rgb(255, 0, 0)
    );
}

#[test]
fn offscreen_transparent_pixels_do_not_capture_parent_background() {
    let mut world = make_world();
    // Source is a 6×6 widget centered in a 12×12 buffer area
    // (it's only 6×6, so the buffer is sized 6×6 and the whole
    // buffer is "inside" the widget). To get a real "outside",
    // give the source a transparent area: just make sure the
    // alpha channel reflects what was written, not seeded.
    let panel = spawn(
        &mut world,
        None,
        Style {
            // No bg_color — source draws nothing into the buffer,
            // so every pixel should remain at the cleared default
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(6)),
                height: Dimension::Px(Fixed::from_int(6)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(panel, super::super::OffscreenRender::default());
    world.insert_resource(super::super::Outputs::<WidgetTexture>::with_budget(
        64 * 1024,
    ));

    let mut buf = std::vec![0xFFu8; 12 * 12 * 4];
    let tex = Texture::new(&mut buf, 12, 12, ColorFormat::RGBA8888).expect("valid pixel storage");
    let mut renderer = SwRenderer::new(tex);
    let viewport = Viewport::new(12, 12, Fixed::ONE);
    super::render(&world, panel, &viewport, &mut renderer).unwrap();

    let outputs = world.resource::<Outputs<WidgetTexture>>().unwrap();
    let output = outputs.get(panel).unwrap();
    for px in output
        .texture()
        .read_pixels()
        .unwrap()
        .bytes()
        .chunks_exact(4)
    {
        assert_eq!(px[3], 0, "alpha-clear buffer pixel must stay alpha=0");
    }
}

/// Theme swap while a subtree is `Hidden`, then unhide.
/// `World::mark_subtree_dirty` skips Hidden, so the offscreen descendant
/// inside the hidden subtree never receives Dirty. When the
/// subtree is later unhidden, the dirty walker must still see
/// enough Dirty markers in the freshly-revealed subtree to bump
/// every OffscreenGeneration inside it; otherwise the cached
/// buffer keeps painting in the previous theme's colours.
#[test]
fn unhide_after_global_event_invalidates_offscreen_descendants() {
    use crate::ui::Hidden;
    use crate::ui::dirty::Dirty;
    use crate::ui::offscreen::OffscreenGeneration;

    let mut world = make_world();
    let root = spawn(
        &mut world,
        None,
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let tab_content = spawn(
        &mut world,
        Some(root),
        Style {
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(64)),
                height: Dimension::Px(Fixed::from_int(64)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let offscreen_grandchild = spawn(
        &mut world,
        Some(tab_content),
        Style {
            bg_color: Some(Color::rgb(64, 128, 255).into()),
            layout: LayoutStyle {
                width: Dimension::Px(Fixed::from_int(32)),
                height: Dimension::Px(Fixed::from_int(32)),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    world.insert(offscreen_grandchild, OffscreenRender::default());

    // Frame 1 — initial render, generation gets to 1.
    world.insert(offscreen_grandchild, Dirty);
    let viewport = Viewport::new(64, 64, Fixed::ONE);
    let _ = super::collect_dirty_region(&mut world, root, &viewport).unwrap();
    let g0 = world
        .get::<OffscreenGeneration>(offscreen_grandchild)
        .map(|g| g.0)
        .unwrap_or(0);
    assert_eq!(g0, 1, "first dirty walk must bump generation 0 → 1");

    // Hide the tab and clear any leftover Dirty in the subtree
    // (mirrors `tab_pages_system`'s hide branch).
    world.insert(tab_content, Hidden);
    world.clear_subtree_dirty(tab_content);

    // Global event: theme swap walks from `root`, hitting Hidden
    // along the way. The offscreen grandchild does not get Dirty.
    world.mark_subtree_dirty(root);
    assert!(
        world.get::<Dirty>(offscreen_grandchild).is_none(),
        "Hidden subtree's descendants must not be Dirty after \
             World::mark_subtree_dirty (this part is the existing optimisation)"
    );

    // Unhide: emulate the (false, true) branch of
    // `tab_pages_system::apply_visibility`. Marking the whole
    // subtree (rather than only `tab_content`) is what makes
    // descendants' caches invalidate.
    world.remove::<Hidden>(tab_content);
    world.mark_subtree_dirty(tab_content);

    // Run the walker and check the offscreen generation has
    // advanced — the cached buffer must be invalidated so the
    // next render misses cache and repaints in the new theme.
    let _ = super::collect_dirty_region(&mut world, root, &viewport).unwrap();
    let g1 = world
        .get::<OffscreenGeneration>(offscreen_grandchild)
        .map(|g| g.0)
        .unwrap_or(0);
    assert!(
        g1 > g0,
        "OffscreenGeneration must bump on unhide so the cache \
             misses; got {g1}, frame-1 was {g0}"
    );
}
