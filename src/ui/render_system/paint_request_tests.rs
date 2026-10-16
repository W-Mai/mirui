use core::cell::Cell;

use crate::core::reactive::flush_signal_dirty;
use crate::ecs::World;
use crate::prelude::*;
use crate::render::sw::SwRenderer;
use crate::render::texture::{ColorFormat, Texture};
use crate::render::{DrawCommand, RenderError};
use crate::surface::FramebufferAccess;
use crate::types::{Transform, Viewport};
use crate::ui::dirty::VisualDirty;
use crate::ui::{OffscreenRender, Outputs, WidgetTexture};

type TestApp = crate::app::App<
    crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
>;

const RED: Color = Color::rgb(229, 47, 23);
const BLUE: Color = Color::rgb(23, 47, 229);

#[crate::component]
struct RequestProbe {
    calls: Cell<usize>,
    requests: Cell<bool>,
    fail: Cell<bool>,
    fail_right: Cell<bool>,
    clips: Cell<[Rect; 2]>,
}

impl Default for RequestProbe {
    fn default() -> Self {
        Self {
            calls: Cell::new(0),
            requests: Cell::new(true),
            fail: Cell::new(false),
            fail_right: Cell::new(false),
            clips: Cell::new([Rect::ZERO; 2]),
        }
    }
}

fn painted_color(calls: usize) -> Color {
    if calls % 2 == 1 { RED } else { BLUE }
}

#[crate::view(component = RequestProbe)]
fn request_probe(probe: component!(), paint: paint!()) {
    let index = probe.calls.get();
    probe.calls.set(index + 1);
    if index < 2 {
        let mut clips = probe.clips.get();
        clips[index] = paint.clip();
        probe.clips.set(clips);
    }
    if probe.requests.get() {
        paint.request_frame();
        paint.request_frame();
    }
    paint.draw(&DrawCommand::Fill {
        area: paint.rect(),
        transform: Transform::IDENTITY,
        quad: None,
        color: painted_color(index + 1),
        radius: Fixed::ZERO,
        opa: 255,
    });
    if probe.fail.get() || (probe.fail_right.get() && paint.clip().x >= Fixed::from_int(60)) {
        paint.fail(RenderError::BackendFailure);
    }
}

#[crate::component]
#[derive(Default)]
struct FailureProbe {
    enabled: Cell<bool>,
    calls: Cell<usize>,
}

#[crate::view(component = FailureProbe, priority = 100)]
fn failure_probe(probe: component!(), paint: paint!()) {
    probe.calls.set(probe.calls.get() + 1);
    if probe.enabled.get() {
        paint.fail(RenderError::BackendFailure);
    }
}

fn app(width: u16, height: u16, outputs: bool) -> TestApp {
    let mut app = App::headless(width, height);
    app.with_widget(request_probe::view())
        .with_widget(failure_probe::view());
    if outputs {
        app.with_offscreen_pool_budget(256);
    }
    app
}

fn probe(world: &World, entity: Entity) -> &RequestProbe {
    world.get::<RequestProbe>(entity).unwrap()
}

fn version(world: &World, entity: Entity) -> u64 {
    world
        .resource::<Outputs<WidgetTexture>>()
        .unwrap()
        .state(entity)
        .unwrap()
        .current
        .unwrap()
        .version
}

fn assert_idle(app: &mut TestApp) {
    app.render_dirty().unwrap();
    assert!(
        app.world
            .resource::<super::LastDirtyRegions>()
            .unwrap()
            .0
            .is_empty()
    );
}

#[crate::compose]
fn inline_scene() {
    ui! {
        View (id: "requester", width: 4, height: 4) [
            RequestProbe::default(),
        ]
    };
}

#[test]
fn inline_requests_without_outputs_schedule_one_redraw_and_stop_when_unrequested() {
    let mut app = app(4, 4, false);
    let root = app.spawn_root().id();
    app.compose(root, inline_scene());
    app.world.remove_resource::<Outputs<WidgetTexture>>();
    let requester = app.world.find_by_id("requester").unwrap();
    assert!(app.world.resource::<Outputs<WidgetTexture>>().is_none());

    app.render().unwrap();
    assert_eq!(probe(&app.world, requester).calls.get(), 1);
    assert!(app.world.has::<VisualDirty>(requester));
    assert_eq!(app.backend.framebuffer().get_pixel(1, 1), RED);

    probe(&app.world, requester).requests.set(false);
    app.render_dirty().unwrap();
    assert_eq!(probe(&app.world, requester).calls.get(), 2);
    assert_eq!(app.backend.framebuffer().get_pixel(1, 1), BLUE);
    assert_idle(&mut app);
    assert_eq!(probe(&app.world, requester).calls.get(), 2);
    assert!(app.world.resource::<Outputs<WidgetTexture>>().is_none());
}

#[crate::compose]
fn failing_capture_scene() {
    ui! {
        Row (id: "capture", width: 8, height: 4) [
            OffscreenRender::new(),
        ] {
            View (id: "requester", width: 4, height: 4) [
                RequestProbe::default(),
                FailureProbe::default(),
            ]
            View (id: "successor", width: 4, height: 4) [
                FailureProbe::default(),
            ]
        }
    };
}

fn assert_failed_capture_discards_requests(fail_in_callback: bool) {
    let mut app = app(8, 4, true);
    let root = app.spawn_root().id();
    app.compose(root, failing_capture_scene());
    let capture = app.world.find_by_id("capture").unwrap();
    let requester = app.world.find_by_id("requester").unwrap();
    let successor = app.world.find_by_id("successor").unwrap();
    if fail_in_callback {
        probe(&app.world, requester).fail.set(true);
    } else {
        app.world
            .get::<FailureProbe>(successor)
            .unwrap()
            .enabled
            .set(true);
    }

    assert_eq!(app.render(), Err(RenderError::BackendFailure));
    assert_eq!(probe(&app.world, requester).calls.get(), 1);
    assert_eq!(
        app.world
            .get::<FailureProbe>(successor)
            .unwrap()
            .calls
            .get(),
        usize::from(!fail_in_callback)
    );
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(capture)
            .is_none()
    );

    // App restores retry damage after failure; only the deferred queue is under test.
    app.world.clear_subtree_dirty(root);
    flush_signal_dirty(&mut app.world);
    assert!(!app.world.has::<VisualDirty>(requester));
    assert!(!app.world.has::<VisualDirty>(capture));
}

#[test]
fn failed_callback_does_not_publish_its_frame_request() {
    assert_failed_capture_discards_requests(true);
}

#[test]
fn later_capture_failure_discards_requests_from_previously_successful_callbacks() {
    assert_failed_capture_discards_requests(false);
}

#[crate::compose]
fn nested_capture_scene() {
    ui! {
        Row (id: "outer", width: 8, height: 4) [
            OffscreenRender::new(),
        ] {
            View (id: "inner", width: 4, height: 4) [
                OffscreenRender::new(),
                RequestProbe::default(),
            ]
            View (id: "successor", width: 4, height: 4) [
                FailureProbe::default(),
            ]
        }
    };
}

#[test]
fn published_capture_requests_survive_parent_capture_and_main_target_failure() {
    for fail_parent_capture in [true, false] {
        let mut app = app(8, 4, true);
        let root = app.spawn_root().id();
        app.compose(root, nested_capture_scene());
        let inner = app.world.find_by_id("inner").unwrap();
        let outer = app.world.find_by_id("outer").unwrap();
        let successor = app.world.find_by_id("successor").unwrap();
        app.world.insert(root, FailureProbe::default());
        let failure = if fail_parent_capture { successor } else { root };
        app.world
            .get::<FailureProbe>(failure)
            .unwrap()
            .enabled
            .set(true);

        let viewport = Viewport::new(8, 4, Fixed::ONE);
        super::try_update_layout(&mut app.world, root, &viewport).unwrap();
        app.world.clear_subtree_dirty(root);
        let mut pixels = [0; 8 * 4 * 4];
        let texture =
            Texture::new(&mut pixels, 8, 4, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut renderer = SwRenderer::new(texture);
        assert_eq!(
            super::render(&app.world, root, &viewport, &mut renderer),
            Err(RenderError::BackendFailure)
        );
        let outputs = app.world.resource::<Outputs<WidgetTexture>>().unwrap();
        assert_eq!(outputs.get(inner).unwrap().texture().get_pixel(1, 1), RED);
        assert_eq!(outputs.get(outer).is_some(), !fail_parent_capture);
        assert_eq!(probe(&app.world, inner).calls.get(), 1);
        assert_eq!(
            app.world.get::<FailureProbe>(failure).unwrap().calls.get(),
            1
        );
        assert!(!app.world.has::<VisualDirty>(inner));
        flush_signal_dirty(&mut app.world);
        assert!(app.world.has::<VisualDirty>(inner));
        assert!(!app.world.has::<VisualDirty>(outer));
        assert!(!app.world.has::<VisualDirty>(root));
    }
}

#[crate::compose]
fn separated_damage_scene() {
    ui! {
        View (width: 64, height: 4) {
            View (
                id: "left",
                position: Position::Absolute,
                left: 0,
                top: 0,
                width: 4,
                height: 4,
                bg_color: RED
            )
            View (
                id: "right",
                position: Position::Absolute,
                left: 60,
                top: 0,
                width: 4,
                height: 4,
                bg_color: BLUE
            )
        }
    };
}

#[test]
fn split_dirty_regions_commit_main_requests_only_after_every_region_succeeds() {
    for fail_right in [true, false] {
        let mut app = app(64, 4, false);
        app.with_widget(crate::ui::style_view::view());
        let root = app.spawn_root().id();
        app.compose(root, separated_damage_scene());
        let left = app.world.find_by_id("left").unwrap();
        let right = app.world.find_by_id("right").unwrap();
        let initial = RequestProbe::default();
        initial.requests.set(false);
        app.world.insert(root, initial);
        app.render().unwrap();
        {
            let probe = probe(&app.world, root);
            probe.calls.set(0);
            probe.clips.set([Rect::ZERO; 2]);
            probe.requests.set(true);
            probe.fail_right.set(fail_right);
        }
        app.world.invalidate_visual(left);
        app.world.invalidate_visual(right);
        let result = app.render_dirty();
        assert_eq!(
            result,
            if fail_right {
                Err(RenderError::BackendFailure)
            } else {
                Ok(())
            }
        );
        let plan = &app.world.resource::<super::LastDirtyRegions>().unwrap().0;
        assert_eq!(plan.rects.len(), 2);
        assert!(plan.shifts.is_empty());
        assert!(plan.prefers_split_redraw());
        assert_eq!(probe(&app.world, root).calls.get(), 2);
        assert_eq!(
            probe(&app.world, root).clips.get(),
            [plan.rects[0], plan.rects[1]]
        );

        if fail_right {
            app.world.clear_subtree_dirty(root);
            flush_signal_dirty(&mut app.world);
        }
        assert_eq!(app.world.has::<VisualDirty>(root), !fail_right);

        probe(&app.world, root).requests.set(false);
        probe(&app.world, root).fail_right.set(false);
        app.render_dirty().unwrap();
        assert_eq!(probe(&app.world, root).calls.get(), 3);
        assert_idle(&mut app);
        assert_eq!(probe(&app.world, root).calls.get(), 3);
    }
}

#[test]
fn capture_requests_rebuild_each_enclosing_output_once_in_the_next_batch() {
    let mut app = app(8, 4, true);
    let root = app.spawn_root().id();
    app.compose(root, nested_capture_scene());
    let inner = app.world.find_by_id("inner").unwrap();
    let outer = app.world.find_by_id("outer").unwrap();
    app.render().unwrap();
    assert_eq!(probe(&app.world, inner).calls.get(), 1);
    let first_inner = version(&app.world, inner);
    let first_outer = version(&app.world, outer);

    probe(&app.world, inner).requests.set(false);
    app.render_dirty().unwrap();
    let second_inner = version(&app.world, inner);
    let second_outer = version(&app.world, outer);
    assert_eq!(probe(&app.world, inner).calls.get(), 2);
    assert_eq!(second_inner, first_outer + 1);
    assert_eq!(second_outer, second_inner + 1);
    assert!(second_inner > first_inner);
    assert_eq!(app.backend.framebuffer().get_pixel(1, 1), BLUE);

    assert_idle(&mut app);
    assert_eq!(probe(&app.world, inner).calls.get(), 2);
    assert_eq!(version(&app.world, inner), second_inner);
    assert_eq!(version(&app.world, outer), second_outer);
}

#[cfg(feature = "std")]
#[test]
fn request_driven_inline_redraw_is_allocation_free_after_warmup() {
    let mut app = app(4, 4, false);
    let root = app.spawn_root().id();
    app.compose(root, inline_scene());
    app.world.remove_resource::<Outputs<WidgetTexture>>();
    let requester = app.world.find_by_id("requester").unwrap();
    assert!(app.world.resource::<Outputs<WidgetTexture>>().is_none());
    app.render().unwrap();
    for _ in 0..4 {
        app.render_dirty().unwrap();
    }
    let warm_calls = probe(&app.world, requester).calls.get();

    let allocations = crate::test_support::tracked_allocations(|| {
        for frame in 1..=32 {
            app.render_dirty().unwrap();
            let expected_calls = warm_calls + frame;
            assert_eq!(probe(&app.world, requester).calls.get(), expected_calls);
            assert_eq!(
                app.backend.framebuffer().get_pixel(1, 1),
                painted_color(expected_calls)
            );
        }
    });
    assert_eq!(allocations, 0);
    assert!(app.world.resource::<Outputs<WidgetTexture>>().is_none());
    probe(&app.world, requester).requests.set(false);
    app.render_dirty().unwrap();
    assert_eq!(probe(&app.world, requester).calls.get(), warm_calls + 33);
    assert_idle(&mut app);
    assert_eq!(probe(&app.world, requester).calls.get(), warm_calls + 33);
}
