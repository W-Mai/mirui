use super::*;
use crate::input::event::hit_test::hit_test;
use crate::types::{Dimension, Fixed};
use crate::ui::builder::WidgetBuilder;
use crate::ui::dirty::Dirty;
use crate::ui::layout::LayoutNode;
use crate::ui::layout::LayoutStyle;
use crate::ui::render_system::{LastDirtyRegions, LayoutSnapshot};
use crate::ui::render_system::{TextLayoutFailureKind, TextLayoutStage};
use crate::ui::widgets::text::Text;
use crate::ui::{Children, HitTarget, Parent};

#[test]
fn completed_operation_statistics_recover_after_removal_without_hot_allocation() {
    use crate::ecs::{FrameStats, FrameTimings};
    let mut app = App::headless(8, 8);
    let finish = |app: &mut App<_, _>, value| {
        let mut phases = FrameTimings::new(FrameOperation::Tick, true);
        phases.record(FramePhase::Input, Some(0), Some(value));
        app.publish_timings(&phases, TimingOutcome::Completed);
    };
    finish(&mut app, 1);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            for value in 2..=300 {
                finish(&mut app, value);
            }
        }),
        0
    );
    let phases = crate::ecs::FramePhases::default().with(FramePhase::Input);
    let summary = app
        .world
        .resource::<FrameStats>()
        .unwrap()
        .summary(FrameOperation::Tick, phases)
        .unwrap();
    assert_eq!(
        (
            summary.count,
            summary.min_nanos,
            summary.max_nanos,
            summary.p99_nanos
        ),
        (256, 45, 300, 298)
    );
    app.world.remove_resource::<FrameStats>();
    finish(&mut app, 400);
    let summary = app
        .world
        .resource::<FrameStats>()
        .unwrap()
        .summary(FrameOperation::Tick, phases)
        .unwrap();
    assert_eq!((summary.count, summary.avg_nanos), (1, 400));
}

#[test]
fn frame_cache_registry_includes_factory_owned_caches() {
    use crate::core::cache::{CacheInspect, CacheRegistry, InspectCaches, LruCache};
    use crate::types::Viewport;

    struct CachedFactory {
        software: SwRendererFactory,
        cache: LruCache<u32, u32>,
    }
    impl InspectCaches for CachedFactory {
        fn inspect_caches(&self) -> impl Iterator<Item = (&'static str, &dyn CacheInspect)> {
            core::iter::once(("factory/test", &self.cache as &dyn CacheInspect))
        }
    }
    impl<B: FramebufferAccess> RendererFactory<B> for CachedFactory {
        type Renderer<'a>
            = crate::render::SwRenderer<'a>
        where
            B: 'a;

        fn make<'a>(&'a mut self, backend: &'a mut B, viewport: &Viewport) -> Self::Renderer<'a> {
            self.software.make(backend, viewport)
        }
    }
    let mut factory = CachedFactory {
        software: SwRendererFactory::new(),
        cache: LruCache::builder()
            .max_size(crate::core::cache::MaxSize::Count(1))
            .name("factory/test")
            .build(),
    };
    factory.cache.entry(1).or_insert_with(|| 7);
    let mut app = App::with_factory(App::headless(8, 8).backend, factory);
    app.publish_cache_stats();
    let registry = app.world.resource::<CacheRegistry>().unwrap();
    assert_eq!(registry.snapshots().len(), 1);
    assert_eq!(registry.snapshots()[0].name, "factory/test");
    assert_eq!(registry.snapshots()[0].len, 1);
    assert_eq!(registry.snapshots()[0].stats.insert_count, 1);
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..32 {
            app.publish_cache_stats();
        }
    });
    assert_eq!(allocations, 0);
}

#[crate::compose]
fn painted_bounds_scene() {
    crate::ui! {
        View (id: "scroll", width: 48, height: 32, clip_children: true) [
            crate::input::event::scroll::ScrollOffset::default(),
        ] {
            View (
                id: "marker",
                position: crate::ui::layout::Position::Absolute,
                left: 8,
                top: 16,
                width: 8,
                height: 8,
                bg_color: crate::types::Color::rgb(20, 220, 80)
            )
        }
    };
}

#[test]
fn paint_bounds_advance_after_scroll_render_without_a_descendant_redraw() {
    use crate::input::event::scroll::ScrollOffset;
    use crate::ui::dirty::PaintedCoverage;

    let mut app = App::headless(48, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    let scroll = app.world.find_by_id("scroll").unwrap();
    let marker = app.world.find_by_id("marker").unwrap();
    app.render().unwrap();
    let viewport = app.backend.viewport();
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            render_system::seed_prev_rects(&mut app.world, root, &viewport);
        }),
        0
    );
    for offset in [8, 12, 4] {
        app.world.get_mut::<ScrollOffset>(scroll).unwrap().y = Fixed::from_int(offset);
        app.world.invalidate_visual(scroll);
        assert!(!app.world.has::<Dirty>(marker));
        app.render_dirty().unwrap();
        assert_eq!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .shifts
                .len(),
            1
        );
        assert_eq!(
            app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
            Rect::new(8, 16 - offset, 8, 8)
        );
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            dirty
        );
    }
}

#[crate::compose]
fn nested_painted_scroll_scene() {
    crate::ui! {
        View (
            id: "outer",
            width: 64,
            height: 64,
            clip_children: true,
            bg_color: crate::types::Color::rgb(25, 40, 80)
        ) [
            crate::input::event::scroll::ScrollOffset::default(),
        ] {
            View (
                id: "inner",
                position: crate::ui::layout::Position::Absolute,
                left: 4,
                top: 12,
                width: 48,
                height: 32,
                clip_children: true,
                bg_color: crate::types::Color::rgb(80, 40, 25)
            ) [
                crate::input::event::scroll::ScrollOffset::default(),
            ] {
                View (
                    id: "nested-marker",
                    position: crate::ui::layout::Position::Absolute,
                    left: 8,
                    top: 10,
                    width: 8,
                    height: 18,
                    bg_color: crate::types::Color::rgb(20, 220, 80)
                )
            }
        }
    };
}

#[test]
fn nested_scroll_plan_uses_painted_sources_and_repaints_final_exposure() {
    use crate::input::event::scroll::ScrollOffset;
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, nested_painted_scroll_scene());
    let outer = app.world.find_by_id("outer").unwrap();
    let inner = app.world.find_by_id("inner").unwrap();
    app.render().unwrap();
    for (outer_y, inner_y) in [(-2, 5), (1, 8), (4, 2)] {
        app.world.get_mut::<ScrollOffset>(outer).unwrap().y = outer_y.into();
        app.world.get_mut::<ScrollOffset>(inner).unwrap().y = inner_y.into();
        app.world.invalidate_visual(outer);
        app.world.invalidate_visual(inner);
        app.render_dirty().unwrap();
        assert_eq!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .shifts
                .len(),
            2
        );
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            dirty
        );
    }
}

#[test]
fn positioning_and_collection_preserve_painted_bounds() {
    use crate::ui::dirty::{PaintInflate, PaintedCoverage};

    let mut app = App::headless(48, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    let marker = app.world.find_by_id("marker").unwrap();
    app.world.insert(
        marker,
        PaintInflate {
            left: Fixed::ONE,
            ..Default::default()
        },
    );
    app.render().unwrap();
    let painted = Rect::new(7, 16, 9, 8);
    assert_eq!(
        app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
        painted
    );
    for left in [32, 2, 20] {
        crate::ui::set_position(&mut app.world, marker, left, 16);
        assert_eq!(
            app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
            painted
        );
        app.dirty_region().unwrap();
        assert_eq!(
            app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
            painted
        );
    }
    app.render().unwrap();
    assert_eq!(
        app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
        Rect::new(19, 16, 9, 8)
    );
}

#[crate::component]
#[derive(Default)]
struct FailedFrame {
    enabled: core::cell::Cell<bool>,
}

#[test]
fn queries_preserve_pending_pixels_and_reuse_the_plan() {
    let mut app = App::headless(48, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    let marker = app.world.find_by_id("marker").unwrap();
    for _ in 0..2 {
        assert_eq!(app.dirty_region().unwrap(), Some(Rect::new(0, 0, 48, 32)));
    }
    app.render_dirty().unwrap();
    let pixels = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    crate::ui::set_position(&mut app.world, marker, 24, 16);
    let first = app.dirty_regions().unwrap().bounding_rect();
    let plan_ptr = app.dirty_regions().unwrap().rects.as_ptr();
    let allocations = crate::test_support::tracked_allocations(|| {
        for _ in 0..16 {
            assert_eq!(app.dirty_region().unwrap(), first);
            assert_eq!(app.dirty_regions().unwrap().rects.as_ptr(), plan_ptr);
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        pixels
    );
    crate::ui::set_position(&mut app.world, marker, 12, 16);
    assert_eq!(app.dirty_region().unwrap(), Some(Rect::new(8, 16, 12, 8)));
    app.render_dirty().unwrap();
    assert!(app.dirty_regions().unwrap().is_empty());
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        dirty
    );
}

#[test]
fn repeated_queries_preserve_nested_scroll_transfers() {
    use crate::input::event::scroll::ScrollOffset;
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, nested_painted_scroll_scene());
    let outer = app.world.find_by_id("outer").unwrap();
    let inner = app.world.find_by_id("inner").unwrap();
    app.render().unwrap();
    for (outer_y, inner_y) in [(-2, 5), (1, 8), (4, 2)] {
        app.world.get_mut::<ScrollOffset>(outer).unwrap().y = outer_y.into();
        app.world.get_mut::<ScrollOffset>(inner).unwrap().y = inner_y.into();
        app.world.invalidate_visual(outer);
        app.world.invalidate_visual(inner);
        let expected = app.dirty_regions().unwrap().shifts.clone();
        assert_eq!(expected.len(), 2);
        for _ in 0..3 {
            let plan = app.dirty_regions().unwrap();
            assert_eq!(plan.shifts.len(), expected.len());
            for (actual, expected) in plan.shifts.iter().zip(&expected) {
                assert_eq!(
                    (actual.area, actual.dx, actual.dy),
                    (expected.area, expected.dx, expected.dy)
                );
            }
        }
    }
    app.render_dirty().unwrap();
    assert_eq!(
        app.world
            .resource::<LastDirtyRegions>()
            .unwrap()
            .0
            .shifts
            .len(),
        2
    );
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    app.render().unwrap();
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        dirty
    );
    assert!(app.dirty_regions().unwrap().is_empty());
}

#[test]
fn nested_scroll_repaints_moved_and_shrunken_content_from_painted_bounds() {
    use crate::input::event::scroll::ScrollOffset;
    let mut app = App::headless(64, 64);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, nested_painted_scroll_scene());
    let outer = app.world.find_by_id("outer").unwrap();
    let inner = app.world.find_by_id("inner").unwrap();
    let marker = app.world.find_by_id("nested-marker").unwrap();
    app.render().unwrap();
    for (outer_y, inner_y, x) in [(-2, 5, 24), (1, 8, 12), (4, 2, 30)] {
        app.world.get_mut::<ScrollOffset>(outer).unwrap().y = outer_y.into();
        app.world.get_mut::<ScrollOffset>(inner).unwrap().y = inner_y.into();
        app.world.invalidate_visual(outer);
        app.world.invalidate_visual(inner);
        app.dirty_regions().unwrap();
        let style = app.world.get_mut::<crate::ui::Style>(marker).unwrap();
        style.layout.left = Dimension::px(x);
        style.layout.height = Dimension::px(6);
        app.world.invalidate(marker);
        assert_eq!(app.dirty_regions().unwrap().shifts.len(), 2);
        app.render_dirty().unwrap();
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert!(
            app.backend.framebuffer().read_pixels().unwrap().bytes() == dirty,
            "offsets {outer_y}/{inner_y}"
        );
    }
}

#[test]
fn painted_bounds_record_only_pixels_inside_the_committed_clip() {
    use crate::input::event::scroll::ScrollOffset;
    use crate::ui::dirty::PaintedCoverage;
    let mut app = App::headless(48, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    let scroll = app.world.find_by_id("scroll").unwrap();
    let marker = app.world.find_by_id("marker").unwrap();
    app.render().unwrap();
    for offset in [20, 24, 8] {
        app.world.get_mut::<ScrollOffset>(scroll).unwrap().y = offset.into();
        app.world.invalidate_visual(scroll);
        app.render_dirty().unwrap();
        let expected = Rect::new(8, 16 - offset, 8, 8)
            .intersect(&Rect::new(0, 0, 48, 32))
            .unwrap_or(Rect::ZERO);
        assert_eq!(
            app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
            expected
        );
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert!(app.backend.framebuffer().read_pixels().unwrap().bytes() == dirty);
    }
}

#[test]
fn nested_scroll_erases_hidden_and_retired_content_after_a_query() {
    use crate::input::event::scroll::ScrollOffset;
    for retire in [false, true] {
        let mut app = App::headless(64, 64);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        app.compose(root, nested_painted_scroll_scene());
        let outer = app.world.find_by_id("outer").unwrap();
        let inner = app.world.find_by_id("inner").unwrap();
        let marker = app.world.find_by_id("nested-marker").unwrap();
        let branches = [alloc::vec![marker]];
        crate::ui::branch::prepare(&mut app.world, inner, &branches);
        crate::ui::branch::select(&mut app.world, &branches, None, Some(0));
        app.render().unwrap();
        app.world.get_mut::<ScrollOffset>(outer).unwrap().y = (-2).into();
        app.world.get_mut::<ScrollOffset>(inner).unwrap().y = 5.into();
        app.world.invalidate_visual(outer);
        app.world.invalidate_visual(inner);
        app.dirty_regions().unwrap();
        if retire {
            crate::ui::retirement::despawn_subtree(&mut app.world, marker);
        } else {
            crate::ui::branch::select(&mut app.world, &branches, Some(0), None);
        }
        let plan = app.dirty_regions().unwrap();
        assert_eq!(plan.shifts.len(), 2);
        assert!(!plan.rects.contains(&Rect::new(0, 0, 64, 64)));
        app.render_dirty().unwrap();
        let dirty = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert!(
            app.backend.framebuffer().read_pixels().unwrap().bytes() == dirty,
            "retire={retire}"
        );
        assert!(app.dirty_regions().unwrap().is_empty());
    }
}

#[crate::view(component = FailedFrame)]
fn failed_frame(probe: component!(), paint: paint!()) {
    if probe.enabled.get() {
        paint.fail(crate::render::RenderError::BackendFailure);
    }
}

#[test]
fn interrupted_drawing_requires_a_complete_frame_before_reusing_pixels() {
    use crate::input::event::scroll::ScrollOffset;
    use crate::ui::dirty::PaintedCoverage;

    for full in [false, true] {
        let mut app = App::headless(48, 32);
        app.with_default_widgets().with_widget(failed_frame::view());
        let root = app.spawn_root().id();
        app.compose(root, painted_bounds_scene());
        app.world.insert(root, FailedFrame::default());
        let scroll = app.world.find_by_id("scroll").unwrap();
        let marker = app.world.find_by_id("marker").unwrap();
        app.render().unwrap();
        let before = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        let painted = app.world.get::<PaintedCoverage>(marker).unwrap().0.outer;

        app.world
            .get::<FailedFrame>(root)
            .unwrap()
            .enabled
            .set(true);
        app.world.get_mut::<ScrollOffset>(scroll).unwrap().y = 8.into();
        app.world.invalidate_visual(scroll);
        let result = if full {
            app.render()
        } else {
            app.render_dirty()
        };
        assert_eq!(result, Err(crate::render::RenderError::BackendFailure));
        assert_ne!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            before
        );
        assert_eq!(
            app.world.get::<PaintedCoverage>(marker).unwrap().0.outer,
            painted
        );

        app.world
            .get::<FailedFrame>(root)
            .unwrap()
            .enabled
            .set(false);
        app.world.get_mut::<ScrollOffset>(scroll).unwrap().y = 12.into();
        app.world.invalidate_visual(scroll);
        app.render_dirty().unwrap();
        let plan = &app.world.resource::<LastDirtyRegions>().unwrap().0;
        assert!(
            plan.shifts.is_empty(),
            "failed drawing leaves no reusable pixel baseline"
        );
        assert_eq!(plan.rects, [Rect::new(0, 0, 48, 32)]);
        let recovered = app
            .backend
            .framebuffer()
            .read_pixels()
            .unwrap()
            .bytes()
            .to_vec();
        app.render().unwrap();
        assert_eq!(
            app.backend.framebuffer().read_pixels().unwrap().bytes(),
            recovered
        );
        app.render_dirty().unwrap();
        assert!(
            app.world
                .resource::<LastDirtyRegions>()
                .unwrap()
                .0
                .is_empty()
        );
    }
}

struct RotatingBuffers {
    pixels: [[u8; 48 * 32 * 4]; 2],
    active: usize,
    advances: usize,
    persistence: crate::surface::BackbufferPersistence,
}

impl crate::core::cache::InspectCaches for RotatingBuffers {}

impl Surface for RotatingBuffers {
    fn begin_flush(&mut self) -> Result<(), crate::render::RenderError> {
        self.active ^= 1;
        self.advances += 1;
        Ok(())
    }
    fn display_info(&self) -> crate::surface::DisplayInfo {
        crate::surface::DisplayInfo {
            width: 48,
            height: 32,
            scale: Fixed::ONE,
            format: crate::render::texture::ColorFormat::RGBA8888,
        }
    }

    fn flush(&mut self, _: PhysicalRect) -> Result<(), crate::render::RenderError> {
        Ok(())
    }

    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }

    fn buffer_count(&self) -> usize {
        2
    }

    fn persistence(&self) -> crate::surface::BackbufferPersistence {
        self.persistence
    }
}

impl FramebufferAccess for RotatingBuffers {
    fn framebuffer(&mut self) -> crate::render::texture::Texture<'_> {
        crate::render::texture::Texture::new(
            &mut self.pixels[self.active],
            48,
            32,
            crate::render::texture::ColorFormat::RGBA8888,
        )
        .expect("valid pixel storage")
    }

    fn all_buffers(&mut self) -> Vec<crate::render::texture::Texture<'_>> {
        let mut textures: Vec<_> = self
            .pixels
            .iter_mut()
            .map(|pixels| {
                crate::render::texture::Texture::new(
                    pixels,
                    48,
                    32,
                    crate::render::texture::ColorFormat::RGBA8888,
                )
                .expect("valid pixel storage")
            })
            .collect();
        textures.rotate_left(self.active);
        textures
    }
}

#[test]
fn full_and_dirty_frames_share_the_same_rotating_buffer_baseline() {
    let mut app = App::new(RotatingBuffers {
        pixels: [[0; 48 * 32 * 4]; 2],
        active: 0,
        advances: 0,
        persistence: crate::surface::BackbufferPersistence::Persistent,
    });
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    let marker = app.world.find_by_id("marker").unwrap();

    for (frame, x) in [8, 16, 24, 12].into_iter().enumerate() {
        let _ = app.dirty_regions().unwrap();
        crate::ui::set_position(&mut app.world, marker, x, 16);
        for _ in 0..3 {
            assert!(!app.dirty_regions().unwrap().is_empty());
            assert_eq!(app.backend.advances, frame);
        }
        if frame % 2 == 0 {
            app.render().unwrap();
        } else {
            app.render_dirty().unwrap();
        }
        assert_eq!(app.backend.advances, frame + 1);
        assert_eq!(app.backend.pixels[0], app.backend.pixels[1]);
        assert_eq!(
            app.world
                .get::<crate::ui::dirty::PaintedCoverage>(marker)
                .unwrap()
                .0
                .outer,
            Rect::new(x, 16, 8, 8)
        );
    }
    app.render_dirty().unwrap();
    assert_eq!(
        app.backend.advances, 4,
        "idle frames must not rotate buffers"
    );
}

#[test]
fn transient_queries_preserve_full_redraw_without_advancing_buffers() {
    let mut app = App::new(RotatingBuffers {
        pixels: [[0; 48 * 32 * 4]; 2],
        active: 0,
        advances: 0,
        persistence: crate::surface::BackbufferPersistence::Transient,
    });
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, painted_bounds_scene());
    app.render().unwrap();
    let painted = app.backend.pixels[0];
    for frame in 1..=3 {
        assert!(app.dirty_regions().unwrap().is_empty());
        app.backend.pixels[app.backend.active].fill(0);
        app.world.invalidate_visual(root);
        for _ in 0..3 {
            let plan = app.dirty_regions().unwrap();
            assert_eq!(plan.rects, [Rect::new(0, 0, 48, 32)]);
            assert!(plan.shifts.is_empty());
            assert_eq!(app.backend.advances, frame);
            assert!(
                app.backend.pixels[app.backend.active]
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
        app.render_dirty().unwrap();
        assert_eq!(app.backend.advances, frame + 1);
        assert_eq!(app.backend.pixels[0], painted);
        assert_eq!(app.backend.pixels[1], painted);
    }
}

#[test]
fn text_raster_scratch_hints_share_one_maximum_extent() {
    let mut app = App::headless(8, 8);
    assert!(matches!(
        app.prefer_text_raster_scratch(0, 8),
        Err(crate::render::RenderError::InvalidGeometry)
    ));
    assert_eq!(app.text_raster_scratch_extent, None);

    app.prefer_text_raster_scratch(12, 4).unwrap();
    app.prefer_text_raster_scratch(8, 6).unwrap();
    assert_eq!(app.text_raster_scratch_extent, Some((12, 6)));
}

struct FailingScratchFactory {
    software: SwRendererFactory,
    fail: bool,
    requested_runs: usize,
}

impl crate::core::cache::InspectCaches for FailingScratchFactory {}

impl<B: FramebufferAccess> RendererFactory<B> for FailingScratchFactory {
    type Renderer<'a>
        = crate::render::backends::sw::SwRenderer<'a>
    where
        Self: 'a,
        B: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut B,
        viewport: &crate::types::Viewport,
    ) -> Self::Renderer<'a> {
        self.software.make(backend, viewport)
    }

    fn prepare_text_raster_scratch(
        &mut self,
        _viewport: &crate::types::Viewport,
        _width: u16,
        _height: u16,
        retained_runs: usize,
    ) -> Result<(), crate::render::RenderError> {
        self.requested_runs = retained_runs;
        if self.fail {
            Err(crate::render::RenderError::ResourceLimit(
                crate::render::RenderResource::Target,
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn optional_text_raster_scratch_failure_does_not_abort_frames() {
    let surface =
        crate::surface::framebuf::FramebufSurface::new(32, 32, (|_, _| {}) as HeadlessFlush);
    let mut app = App::with_factory(
        surface,
        FailingScratchFactory {
            software: SwRendererFactory::new(),
            fail: true,
            requested_runs: 0,
        },
    );
    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("A", id: "scratch_a", capacity: 1)
        }
    };
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("B", id: "scratch_b", capacity: 1)
        }
    };
    app.prefer_text_raster_scratch(8, 8).unwrap();

    app.render().unwrap();
    assert_eq!(app.factory.requested_runs, 2);
    assert_eq!(
        app.last_text_raster_scratch_preparation_error(),
        Some(crate::render::RenderError::ResourceLimit(
            crate::render::RenderResource::Target,
        )),
    );
    app.render_dirty().unwrap();
    app.factory.fail = false;
    app.render_dirty().unwrap();
    assert_eq!(app.last_text_raster_scratch_preparation_error(), None);
}

fn text_capacity(slots: usize, workspace_glyphs: usize) -> crate::text::TextLayoutCapacity {
    crate::text::TextLayoutCapacity {
        layout_slots: slots,
        measurements: 8,
        lines: 8,
        runs: 8,
        glyphs: 48,
        carets: 48,
        workspace: crate::text::WorkspaceCapacity {
            runs: 8,
            glyphs: workspace_glyphs,
            scratch_glyphs: 24,
            lines: 8,
        },
    }
}

#[test]
fn text_requirements_merge_across_named_subtrees_before_layout() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let left = text_capacity(3, 16);
    let right = text_capacity(4, 24);
    app.require_text_layout_capacity(left).unwrap();
    app.require_text_layout_capacity(right).unwrap();
    let combined = crate::text::TextLayoutCapacity {
        layout_slots: 7,
        measurements: 16,
        lines: 16,
        runs: 16,
        glyphs: 96,
        carets: 96,
        workspace: crate::text::WorkspaceCapacity {
            runs: 8,
            glyphs: 24,
            scratch_glyphs: 24,
            lines: 8,
        },
    };
    assert_eq!(
        app.text_layout_reservation,
        TextLayoutReservation::Pending(combined)
    );

    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Column () {
                Column (id: "left_tree") {
                    Text ("LEFT", id: "left_label")
                }
                Column (id: "right_tree") {
                    Text ("RIGHT", id: "right_label")
                }
            }
        }
    };
    assert_ne!(
        app.world.find_by_id("left_tree"),
        app.world.find_by_id("right_tree")
    );
    app.prepare_text_layout().unwrap();
    assert_eq!(app.text_layout_reservation, TextLayoutReservation::Prepared);
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .bounded_capacity(),
        Some(combined)
    );
    app.render().unwrap();
    assert_eq!(app.last_text_layout_preparation_error(), None);
    assert_eq!(app.last_text_layout_failure(), None);
    assert_eq!(
        app.require_text_layout_capacity(right).err(),
        Some(crate::text::TextLayoutError::InUse)
    );
}

#[test]
fn text_requirement_overflow_and_failed_preparation_are_atomic() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let first = text_capacity(3, 16);
    app.require_text_layout_capacity(first).unwrap();
    let overflow = crate::text::TextLayoutCapacity {
        layout_slots: usize::MAX,
        ..first
    };
    assert_eq!(
        app.require_text_layout_capacity(overflow).err(),
        Some(crate::text::TextLayoutError::DimensionOverflow)
    );
    assert_eq!(
        app.text_layout_reservation,
        TextLayoutReservation::Pending(first)
    );

    app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST.with_cache_bytes(1024));
    assert_eq!(
        app.text_layout_reservation,
        TextLayoutReservation::Pending(first)
    );
    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("STORAGE", id: "storage_label")
        }
    };
    let error = app.prepare_text_layout().err().unwrap();
    assert!(matches!(
        error,
        crate::text::TextLayoutError::CacheBudget { .. }
    ));
    assert_eq!(app.last_text_layout_preparation_error(), Some(error));
    assert_eq!(app.last_text_layout_failure(), None);
    assert!(crate::input::event::hit_test::preparation_blocked(
        &app.world
    ));
    crate::input::event::deliver_input(
        &mut app.world,
        Some(root),
        &InputEvent::PointerDown {
            kind: crate::input::event::input::PointerKind::Mouse,
            id: 0,
            x: 1.into(),
            y: 1.into(),
        },
        0,
        64,
        32,
    );
    assert!(
        app.world
            .resource::<render_system::LayoutSnapshot>()
            .is_none()
    );
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .bounded_capacity(),
        None
    );
    assert_eq!(
        app.render(),
        Err(crate::render::RenderError::TextPreparation)
    );
    assert_eq!(
        app.render_dirty(),
        Err(crate::render::RenderError::TextPreparation)
    );
    assert_eq!(
        app.text_layout_reservation,
        TextLayoutReservation::Pending(first)
    );

    app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
    app.prepare_text_layout().unwrap();
    assert_eq!(app.last_text_layout_preparation_error(), None);
    assert!(!crate::input::event::hit_test::preparation_blocked(
        &app.world
    ));
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .bounded_capacity(),
        Some(first)
    );
    assert!(matches!(
        app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST.with_cache_bytes(1024)),
        Err(crate::text::TextLayoutError::CacheBudget { .. })
    ));
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .bounded_capacity(),
        Some(first)
    );
    app.render_dirty().unwrap();
}

#[test]
fn text_absolute_capacity_and_requirements_cannot_be_mixed() {
    let mut app = App::headless(64, 32);
    let capacity = text_capacity(4, 24);
    app.with_text_layout_capacity(capacity).unwrap();
    app.with_text_layout_capacity(capacity).unwrap();
    assert_eq!(
        app.require_text_layout_capacity(capacity).err(),
        Some(crate::text::TextLayoutError::ConfigurationConflict)
    );
    let mut required = App::headless(64, 32);
    required.require_text_layout_capacity(capacity).unwrap();
    assert_eq!(
        required.with_text_layout_capacity(capacity).err(),
        Some(crate::text::TextLayoutError::ConfigurationConflict)
    );
    assert_eq!(
        required.text_layout_reservation,
        TextLayoutReservation::Pending(capacity)
    );
    required.prepare_text_layout().unwrap();
    assert_eq!(
        required.with_text_layout_capacity(capacity).err(),
        Some(crate::text::TextLayoutError::ConfigurationConflict)
    );
    assert_eq!(
        required.require_text_layout_capacity(capacity).err(),
        Some(crate::text::TextLayoutError::InUse)
    );
    let root = app.spawn_root().id();
    app.world.insert(root, crate::ui::dirty::Dirty);
    app.render().unwrap();
    assert_eq!(
        app.with_text_layout_capacity(capacity).err(),
        Some(crate::text::TextLayoutError::InUse)
    );
}

#[test]
fn text_requirement_cannot_follow_unbounded_layout_even_after_limit_replacement() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    app.spawn_root().id();
    app.render().unwrap();
    app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
    assert_eq!(
        app.require_text_layout_capacity(text_capacity(4, 24)).err(),
        Some(crate::text::TextLayoutError::InUse)
    );
    assert_eq!(
        app.with_text_layout_capacity(text_capacity(4, 24)).err(),
        Some(crate::text::TextLayoutError::InUse)
    );
}

#[test]
fn layout_limit_replacement_resets_path_frames_only_after_success() {
    use crate::text::baseline::{DEFAULT_TOLERANCE, PlacementKey};
    use crate::text::{TextLayoutHandle, TextPath};

    fn frame_counts(world: &World, key: PlacementKey) -> (Option<usize>, Option<usize>) {
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        let baselines = &storage.paths;
        (
            baselines.glyph_frames(key, <[_]>::len),
            baselines.caret_frames(key, <[_]>::len),
        )
    }

    for bounded in [false, true] {
        let mut app = App::headless(64, 32);
        app.with_default_widgets();
        if bounded {
            app.with_text_layout_capacity(text_capacity(2, 8)).unwrap();
        }
        let mut baseline = crate::render::path::Path::new();
        baseline
            .move_to(crate::types::Point::new(0, 16))
            .line_to(crate::types::Point::new(64, 16));
        let path = app.paths().insert(baseline).unwrap();
        let root = app.spawn_root().id();
        let label = app.compose(root, |cx| cx.spawn(Text::build("A").path(path)));
        app.render().unwrap();
        let handle = *app.world.get::<TextLayoutHandle>(label).unwrap();
        let key = PlacementKey::new(
            label,
            handle,
            TextPath::new(path),
            app.paths().revision(path).unwrap(),
            DEFAULT_TOLERANCE,
        );
        assert_eq!(frame_counts(&app.world, key), (Some(1), Some(2)));

        if bounded {
            assert_eq!(
                app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                    .err(),
                Some(crate::text::TextLayoutError::InUse)
            );
            assert_eq!(frame_counts(&app.world, key), (Some(1), Some(2)));
        } else {
            app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
                .unwrap();
            assert_eq!(frame_counts(&app.world, key), (None, None));
            app.world
                .get_mut::<Text>(label)
                .unwrap()
                .set_content("ABCD");
            app.render_dirty().unwrap();
            assert_eq!(app.world.get::<TextLayoutHandle>(label), Some(&handle));
            assert_eq!(frame_counts(&app.world, key), (Some(4), Some(5)));
        }
    }
}

#[test]
fn input_layout_is_prepared_after_cache_replacement_without_manual_invalidation() {
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

    let glyph_count = |world: &World, handle| {
        world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .get(handle)
            .map(|layout| layout.glyphs().len())
    };
    let mut app = App::headless(64, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let input = app.compose(root, |cx| {
        cx.spawn(
            TextInput::build()
                .placeholder("ABC")
                .style(crate::ui::Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(48)),
                        height: Dimension::Px(Fixed::from_int(16)),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        )
    });
    app.render().unwrap();
    app.render_dirty().unwrap();
    let handle = app.world.get::<TextEditingLayoutHandle>(input).unwrap().0;
    assert_eq!(glyph_count(&app.world, handle), Some(3));

    app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
        .unwrap();
    assert_eq!(glyph_count(&app.world, handle), None);
    app.render_dirty().unwrap();
    let replacement = app.world.get::<TextEditingLayoutHandle>(input).unwrap().0;
    assert_eq!(replacement, handle);
    assert_eq!(glyph_count(&app.world, replacement), Some(3));

    app.handle_memory_warning();
    assert_eq!(glyph_count(&app.world, replacement), None);
    app.render_dirty().unwrap();
    let restored = app.world.get::<TextEditingLayoutHandle>(input).unwrap().0;
    assert_eq!(glyph_count(&app.world, restored), Some(3));
}

#[test]
fn font_manager_replacement_refreshes_input_layout_in_a_dirty_frame() {
    use crate::render::font::{
        Font, FontFaceId, FontMetrics, FontProvider, FontToken, GlyphId, RasterGlyph,
        default_font_manager,
    };
    use crate::ui::widgets::TextInput;
    use crate::ui::widgets::text_editing::geometry::TextEditingLayoutHandle;

    struct SizedFont;
    impl FontProvider for SizedFont {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(404)
        }
        fn map_char(&self, ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(ch as u16))
        }
        fn glyph_advance(&self, _: GlyphId, ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(i32::from(ppem)))
        }
        fn raster(&self, _: GlyphId, _: u16, _: u16) -> Option<RasterGlyph<'_>> {
            None
        }
        fn metrics(&self, ppem: u16) -> FontMetrics {
            FontMetrics {
                ascender: Fixed::from_int(i32::from(ppem)),
                descender: Fixed::ZERO,
                line_height: Fixed::from_int(i32::from(ppem)),
            }
        }
    }
    let provider = alloc::rc::Rc::new(SizedFont);
    let fonts_at_size = |size| {
        let fonts = default_font_manager();
        fonts.add_static(
            FontToken::Default.cache_key(),
            Font {
                family: "input-metrics",
                size,
                provider: provider.clone(),
            },
        );
        fonts
    };
    let mut app = App::headless(64, 32);
    app.with_default_widgets().with_fonts(fonts_at_size(8));
    let root = app.spawn_root().id();
    let input = app.compose(root, |cx| {
        cx.spawn(
            TextInput::build()
                .placeholder("AB")
                .style(crate::ui::Style {
                    layout: LayoutStyle {
                        width: Dimension::Px(Fixed::from_int(56)),
                        height: Dimension::Px(Fixed::from_int(24)),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        )
    });
    let input_layout = |world: &World| {
        let handle = world.get::<TextEditingLayoutHandle>(input).unwrap().0;
        let storage = world.resource::<Outputs<TextGeometry>>().unwrap().storage();
        (handle, storage.layouts.get(handle).unwrap().measure())
    };
    app.render().unwrap();
    app.render_dirty().unwrap();
    let (original, before) = input_layout(&app.world);
    assert_eq!(before.width, 16 << 8);

    app.with_fonts(fonts_at_size(16));
    app.render_dirty().unwrap();
    let (replacement, after) = input_layout(&app.world);
    assert_ne!(replacement, original);
    assert_eq!(after.width, before.width * 2);
    assert_eq!(after.height, before.height * 2);
}

#[test]
fn pending_text_requirement_cannot_discard_a_low_level_layout() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let capacity = text_capacity(4, 24);
    app.require_text_layout_capacity(capacity).unwrap();
    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("LOW LEVEL", id: "low_level")
        }
    };
    crate::ui::render_system::try_update_layout(&mut app.world, root, &app.backend.viewport())
        .unwrap();
    assert_eq!(
        app.try_with_text_layout_limits(crate::text::TextLayoutLimits::HOST)
            .err(),
        Some(crate::text::TextLayoutError::InUse)
    );
    assert_eq!(
        app.prepare_text_layout().err(),
        Some(crate::text::TextLayoutError::InUse)
    );
    assert_eq!(
        app.render(),
        Err(crate::render::RenderError::TextPreparation)
    );
    assert_eq!(
        app.text_layout_reservation,
        TextLayoutReservation::Pending(capacity)
    );
}

#[test]
fn direct_render_prepares_pending_text_capacity_before_layout() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    app.require_text_layout_capacity(text_capacity(4, 24))
        .unwrap();
    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("READY", id: "auto_prepared")
        }
    };
    app.render().unwrap();
    assert_eq!(app.text_layout_reservation, TextLayoutReservation::Prepared);
    assert_eq!(app.last_text_layout_preparation_error(), None);
    assert_eq!(app.last_text_layout_failure(), None);
}

#[test]
fn zero_text_requirement_is_bounded_and_rejects_later_text() {
    let mut app = App::headless(64, 32);
    app.with_default_widgets();
    let capacity = crate::text::TextLayoutCapacity::default();
    app.require_text_layout_capacity(capacity).unwrap();
    let root = app.spawn_root().id();
    {
        let mut cx = ::mirui::ui::UiScope::new(&mut app.world, root);
        crate::ui! {
            Text ("LATE", id: "late_text")
        }
    };
    assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
    assert_eq!(app.last_text_layout_preparation_error(), None);
    assert!(matches!(
        app.last_text_layout_failure().unwrap().kind,
        TextLayoutFailureKind::Text(crate::text::TextLayoutError::Capacity { .. })
    ));
    assert_eq!(
        app.world
            .resource::<Outputs<TextGeometry>>()
            .unwrap()
            .storage()
            .layouts
            .bounded_capacity(),
        Some(capacity)
    );
}

#[test]
fn text_limit_failure_keeps_the_last_frame_and_recovers_in_full_and_dirty_render() {
    let mut app = App::headless(48, 24);
    app.with_default_widgets();
    app.with_text_layout_limits(crate::text::TextLayoutLimits {
        text_bytes: 2,
        ..crate::text::TextLayoutLimits::HOST
    });
    let root = app.spawn_root().id();
    let label = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::Content,
            height: Dimension::Content,
            ..LayoutStyle::default()
        })
        .id();
    app.world.insert(label, Text::from("OK"));
    app.world.insert(label, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(label);

    app.render().unwrap();
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    let snapshot_ptr = app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

    app.world
        .get_mut::<Text>(label)
        .unwrap()
        .set_content("LONG");
    assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );
    assert_eq!(
        app.last_text_layout_failure(),
        Some(crate::ui::render_system::TextLayoutFailure {
            entity: label,
            stage: TextLayoutStage::Measure,
            kind: TextLayoutFailureKind::Text(crate::text::TextLayoutError::TextLimit {
                required: 4,
                limit: 2,
            }),
        })
    );
    assert_eq!(
        app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
        snapshot_ptr,
    );
    assert!(!crate::input::event::hit_test::geometry_matches(
        &app.world, root, 48, 24
    ));

    app.world.get_mut::<Text>(label).unwrap().set_content("OK");
    app.render().unwrap();
    assert_eq!(app.last_text_layout_failure(), None);

    app.world
        .get_mut::<Text>(label)
        .unwrap()
        .set_content("LONG");
    app.world.insert(root, Dirty);
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_eq!(
        app.render_dirty(),
        Err(crate::render::RenderError::TextLayout)
    );
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );
    assert!(app.world.has::<Dirty>(root));
    assert_eq!(app.last_text_layout_failure().unwrap().entity, label);

    app.world.get_mut::<Text>(label).unwrap().set_content("OK");
    app.render_dirty().unwrap();
    assert_eq!(app.last_text_layout_failure(), None);
    assert!(crate::input::event::hit_test::geometry_matches(
        &app.world, root, 48, 24
    ));

    let mut baseline = crate::render::path::Path::new();
    baseline
        .move_to(crate::types::Point::new(0, 16))
        .line_to(crate::types::Point::new(40, 16));
    let path = app
        .world
        .resource_mut::<crate::render::path::PathStore>()
        .unwrap()
        .insert(baseline)
        .unwrap();
    app.world.widget_mut(label).unwrap().text_path(path);
    app.world
        .get_mut::<Text>(label)
        .unwrap()
        .set_content("LONG");
    app.world.insert(root, Dirty);
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_eq!(
        app.render_dirty(),
        Err(crate::render::RenderError::TextLayout)
    );
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );
    assert_eq!(app.last_text_layout_failure().unwrap().entity, label);

    app.world.get_mut::<Text>(label).unwrap().set_content("OK");
    app.render_dirty().unwrap();
    assert_eq!(app.last_text_layout_failure(), None);
}

#[test]
fn dirty_queries_distinguish_idle_damage_and_layout_failure() {
    let mut app = App::headless(48, 24);
    assert_eq!(app.dirty_region(), Ok(None));
    app.with_default_widgets();
    app.with_text_layout_limits(crate::text::TextLayoutLimits {
        text_bytes: 2,
        ..crate::text::TextLayoutLimits::HOST
    });
    let root = app.spawn_root().id();
    let label = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::Content,
            height: Dimension::Content,
            ..LayoutStyle::default()
        })
        .id();
    app.world.insert(label, Text::from("OK"));
    app.world.insert(label, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(label);
    app.render().unwrap();
    assert_eq!(app.dirty_region(), Ok(None));
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();

    app.world
        .get_mut::<Text>(label)
        .unwrap()
        .set_content("LONG");
    app.world.insert(root, Dirty);
    let viewport = app.backend.viewport();
    let failure = render_system::TextLayoutFailure {
        entity: label,
        stage: TextLayoutStage::Measure,
        kind: TextLayoutFailureKind::Text(crate::text::TextLayoutError::TextLimit {
            required: 4,
            limit: 2,
        }),
    };
    assert_eq!(
        render_system::collect_dirty_regions(&mut app.world, root, &viewport).err(),
        Some(failure)
    );
    assert_eq!(
        render_system::collect_dirty_region(&mut app.world, root, &viewport),
        Err(failure)
    );
    assert_eq!(
        app.dirty_region(),
        Err(crate::render::RenderError::TextLayout)
    );
    assert_eq!(app.last_text_layout_failure(), Some(failure));
    assert!(app.world.has::<Dirty>(root));
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );

    app.world.get_mut::<Text>(label).unwrap().set_content("OK");
    assert!(app.dirty_region().unwrap().is_some());
    assert_eq!(app.last_text_layout_failure(), None);
    assert!(!app.world.has::<Dirty>(root));
    assert!(app.dirty_region().unwrap().is_some());
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );
    app.render_dirty().unwrap();
    assert_eq!(app.dirty_region(), Ok(None));
    assert_eq!(
        render_system::collect_dirty_region(&mut app.world, root, &viewport),
        Ok(None)
    );
    assert!(
        render_system::collect_dirty_regions(&mut app.world, root, &viewport)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        before
    );
}

#[test]
fn text_layout_budget_failure_reports_the_layout_stage_and_recovers() {
    let mut app = App::headless(48, 24);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let label = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::Content,
            height: Dimension::Content,
            ..LayoutStyle::default()
        })
        .id();
    app.world.insert(label, Text::from("LABEL"));
    app.world.insert(label, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(label);

    let mut node = LayoutNode::new(LayoutStyle::default());
    crate::ui::render_system::apply_text_intrinsic(&app.world, label, &mut node).unwrap();
    let measure_budget = app
        .world
        .resource::<Outputs<TextGeometry>>()
        .unwrap()
        .storage()
        .layouts
        .resident_bytes();
    app.with_text_layout_limits(
        crate::text::TextLayoutLimits::HOST.with_cache_bytes(measure_budget),
    );

    assert_eq!(app.render(), Err(crate::render::RenderError::TextLayout));
    let failure = app.last_text_layout_failure().unwrap();
    assert_eq!(failure.entity, label);
    assert_eq!(failure.stage, TextLayoutStage::Layout);
    assert!(matches!(
        failure.kind,
        TextLayoutFailureKind::Text(crate::text::TextLayoutError::CacheBudget { .. })
    ));

    app.with_text_layout_limits(crate::text::TextLayoutLimits::HOST);
    app.render().unwrap();
    assert_eq!(app.last_text_layout_failure(), None);
}

#[test]
fn active_and_idle_frames_retain_plan_storage() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let target = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::px(16),
            height: Dimension::px(16),
            ..LayoutStyle::default()
        })
        .id();
    app.world.insert(target, HitTarget);
    app.world.insert(target, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(target);

    app.world.insert(root, Dirty);
    app.render_dirty().unwrap();
    assert!(crate::input::event::hit_test::geometry_matches(
        &app.world, root, 32, 32,
    ));
    assert_eq!(
        app.world.get::<crate::ui::ComputedRect>(root).unwrap().0,
        Rect::new(0, 0, 32, 32),
    );
    assert_eq!(
        hit_test(
            &app.world,
            root,
            Fixed::from_int(4),
            Fixed::from_int(4),
            32,
            32,
        ),
        Some(target),
        "the persistent render path must publish input geometry",
    );
    let scratch_ptr = app.dirty_plan.rects.as_ptr();
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    let last_ptr = last as *const LastDirtyRegions;
    let last_rects_ptr = last.0.rects.as_ptr();
    assert_eq!(last.0.rects.len(), 1);

    app.world.insert(root, Dirty);
    app.render_dirty().unwrap();
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert_eq!(app.dirty_plan.rects.as_ptr(), scratch_ptr);
    assert_eq!(last as *const LastDirtyRegions, last_ptr);
    assert_eq!(last.0.rects.as_ptr(), last_rects_ptr);

    app.render_dirty().unwrap();
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert!(last.0.is_empty());
    assert_eq!(app.dirty_plan.rects.as_ptr(), scratch_ptr);
    assert_eq!(last.0.rects.as_ptr(), last_rects_ptr);
}

#[test]
fn viewport_change_invalidates_retained_layout_snapshot() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.world.insert(root, Dirty);
    app.render().unwrap();
    let snapshot_ptr = app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot;

    let current = app.world.resource::<RenderViewport>().unwrap().0;
    app.prepare_render(crate::types::Viewport::new(64, 32, current.scale()));

    assert_eq!(
        app.world.resource::<LayoutSnapshot>().unwrap() as *const LayoutSnapshot,
        snapshot_ptr
    );
    assert!(app.world.has::<Dirty>(root));
}

#[test]
fn resized_app_reuses_snapshot_and_updates_pixels_and_hit_geometry() {
    use crate::surface::framebuf::FramebufSurface;
    use crate::types::Color;

    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let target = WidgetBuilder::new(&mut app.world)
        .layout(LayoutStyle {
            width: Dimension::percent(50),
            height: Dimension::px(16),
            ..LayoutStyle::default()
        })
        .id();
    app.world
        .get_mut::<crate::ui::Style>(target)
        .unwrap()
        .set_bg_color(Color::rgb(240, 20, 30));
    app.world.insert(target, HitTarget);
    app.world.insert(target, Parent(root));
    app.world.get_mut::<Children>(root).unwrap().0.push(target);

    app.render().unwrap();
    let snapshot = app.world.resource::<LayoutSnapshot>().unwrap();
    let snapshot_ptr = snapshot as *const LayoutSnapshot;
    let entities_ptr = snapshot.entities.as_ptr();
    assert_eq!(
        hit_test(
            &app.world,
            root,
            Fixed::from_int(24),
            Fixed::from_int(4),
            32,
            32
        ),
        None
    );

    let cb: HeadlessFlush = |_buf, _area| {};
    app.backend = FramebufSurface::new(64, 32, cb);
    for _ in 0..3 {
        let plan = app.dirty_regions().unwrap();
        assert_eq!(plan.rects, [Rect::new(0, 0, 64, 32)]);
        assert!(plan.shifts.is_empty());
    }
    app.world
        .get_mut::<crate::ui::Style>(target)
        .unwrap()
        .set_bg_color(Color::rgb(20, 200, 80));
    app.world.invalidate_visual(target);
    app.render_dirty().unwrap();

    let resized = app.world.resource::<LayoutSnapshot>().unwrap();
    assert_eq!(resized as *const LayoutSnapshot, snapshot_ptr);
    assert_eq!(resized.entities.as_ptr(), entities_ptr);
    assert_eq!(
        app.world.get::<crate::ui::ComputedRect>(root).unwrap().0,
        Rect::new(0, 0, 64, 32)
    );
    assert_eq!(
        app.world
            .get::<crate::ui::ComputedRect>(target)
            .unwrap()
            .0
            .w,
        Fixed::from_int(32)
    );
    assert_eq!(
        hit_test(
            &app.world,
            root,
            Fixed::from_int(24),
            Fixed::from_int(4),
            64,
            32
        ),
        Some(target)
    );
    let texture = app.backend.framebuffer();
    let pixel = (4 * 64 + 24) * 4;
    assert_eq!(
        &texture.read_pixels().unwrap().bytes()[pixel..pixel + 3],
        &[20, 200, 80]
    );
    assert!(app.dirty_regions().unwrap().is_empty());
}

#[test]
fn hiding_root_clears_previous_pixels_once() {
    use crate::types::Color;
    use crate::ui::Hidden;

    let mut app = App::headless(16, 16);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.world
        .get_mut::<crate::ui::Style>(root)
        .unwrap()
        .set_bg_color(Color::rgb(240, 20, 30));

    app.world.insert(root, Dirty);
    app.render_dirty().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &[240, 20, 30, 255]
    );

    app.world.insert(root, Hidden);
    app.render_dirty().unwrap();
    let surface = app
        .world
        .resource::<Theme>()
        .unwrap()
        .resolve(crate::ui::theme::ColorToken::Surface);
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &[surface.r, surface.g, surface.b, surface.a]
    );
    assert_eq!(
        app.world.resource::<LastDirtyRegions>().unwrap().0.rects,
        [Rect::new(0, 0, 16, 16)]
    );

    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );

    app.world.remove::<Hidden>(root);
    app.world.insert(root, Dirty);
    app.render_dirty().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..3],
        &[240, 20, 30]
    );

    app.world.insert(root, Hidden);
    app.render().unwrap();
    assert_eq!(
        &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
        &[surface.r, surface.g, surface.b, surface.a]
    );
}
