use super::*;
use crate::render::RenderError;
use crate::surface::{DisplayInfo, framebuf::FramebufSurface};
use crate::types::{Color, Fixed};
use crate::ui::dirty::PaintedCoverage;

type Pixels = FramebufSurface<fn(&[u8], PhysicalRect)>;

struct CheckedSurface {
    pixels: Pixels,
    fail: Option<&'static str>,
    trace: Vec<&'static str>,
    invalid: bool,
    transient: bool,
}

impl CheckedSurface {
    fn step(&mut self, name: &'static str) -> Result<(), RenderError> {
        self.trace.push(name);
        if self.fail == Some(name) {
            Err(if name == "begin" {
                RenderError::SurfaceUnavailable
            } else {
                RenderError::BackendFailure
            })
        } else {
            Ok(())
        }
    }
}

impl crate::core::cache::InspectCaches for CheckedSurface {}

impl Surface for CheckedSurface {
    fn display_info(&self) -> DisplayInfo {
        self.pixels.display_info()
    }
    fn poll_event(&mut self) -> Option<InputEvent> {
        None
    }
    fn begin_flush(&mut self) -> Result<(), RenderError> {
        self.step("begin")
    }
    fn flush(&mut self, _: PhysicalRect) -> Result<(), RenderError> {
        self.step("flush")
    }
    fn end_flush(&mut self) -> Result<(), RenderError> {
        self.step("end")?;
        self.invalid = false;
        Ok(())
    }
    fn needs_redraw(&self) -> bool {
        self.invalid
    }
    fn persistence(&self) -> crate::surface::BackbufferPersistence {
        if self.transient {
            crate::surface::BackbufferPersistence::Transient
        } else {
            crate::surface::BackbufferPersistence::Persistent
        }
    }
    fn abort_flush(&mut self) {
        self.trace.push("abort");
    }
    fn frame_end(&mut self) -> Result<(), RenderError> {
        self.step("maintain")
    }
}

impl FramebufferAccess for CheckedSurface {
    fn framebuffer(&mut self) -> crate::render::texture::Texture<'_> {
        self.trace.push("draw");
        self.pixels.framebuffer()
    }
}

#[crate::compose]
fn marker() {
    crate::ui! {
        View (
            id: "marker",
            position: crate::ui::layout::Position::Absolute,
            left: 0,
            top: 0,
            width: 8,
            height: 8,
            bg_color: Color::rgb(255, 0, 0)
        )
    };
}

fn fixture() -> (App<CheckedSurface>, Entity) {
    let mut app = App::new(CheckedSurface {
        pixels: FramebufSurface::new(40, 30, |_, _| {}),
        fail: None,
        trace: Vec::new(),
        invalid: true,
        transient: false,
    });
    app.with_default_widgets();
    let root = app.spawn_root().id();
    app.compose(root, marker());
    let node = app.world.find_by_id("marker").unwrap();
    app.render().unwrap();
    app.backend.trace.clear();
    (app, node)
}

#[derive(Default)]
struct ObservedTimings(Vec<(crate::ecs::FrameTimings, TimingOutcome)>);

struct ObserveTimings;
impl Plugin<CheckedSurface, SwRendererFactory> for ObserveTimings {
    fn build(&mut self, app: &mut App<CheckedSurface>) {
        app.enable_timing_reports();
        app.world.insert_resource(ObservedTimings::default());
    }

    fn on_timing(&mut self, world: &mut World, sample: &crate::ecs::TimingSample<'_>) {
        world
            .resource_mut::<ObservedTimings>()
            .unwrap()
            .0
            .push((*sample.phases, sample.outcome));
    }
}

#[test]
fn operation_samples_do_not_join_separate_host_callbacks_or_repeat_nested_draws() {
    let (mut app, node) = fixture();
    app.add_plugin(ObserveTimings);
    app.update_tick();
    crate::ui::set_position(&mut app.world, node, 12, 0);
    app.draw_tick();
    crate::ui::set_position(&mut app.world, node, 16, 0);
    app.tick();
    let samples = &app.world.resource::<ObservedTimings>().unwrap().0;
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].0.operation(), FrameOperation::Update);
    assert!(samples[0].0.has_phase(FramePhase::Systems));
    assert!(!samples[0].0.has_phase(FramePhase::Render));
    assert_eq!(samples[1].0.operation(), FrameOperation::Draw);
    assert!(!samples[1].0.has_phase(FramePhase::Input));
    assert!(samples[1].0.has_phase(FramePhase::Flush));
    assert_eq!(samples[2].0.operation(), FrameOperation::Tick);
    assert!(samples[2].0.has_phase(FramePhase::Input));
    assert!(samples[2].0.has_phase(FramePhase::Flush));
    assert!(
        samples
            .iter()
            .all(|(_, outcome)| *outcome == TimingOutcome::Completed)
    );
}

#[test]
fn failed_and_idle_attempts_do_not_reuse_previous_phases_or_grow_completed_statistics() {
    let (mut app, _) = fixture();
    app.add_plugin(ObserveTimings);
    app.render().unwrap();
    app.world
        .resource_mut::<ObservedTimings>()
        .unwrap()
        .0
        .clear();
    let count = app
        .world
        .resource::<crate::ecs::FrameStats>()
        .unwrap()
        .len();
    app.render_dirty().unwrap();
    app.backend.invalid = true;
    app.backend.fail = Some("end");
    assert!(app.render_dirty().is_err());
    assert_eq!(
        app.world
            .resource::<crate::ecs::FrameStats>()
            .unwrap()
            .len(),
        count
    );
    let samples = &app.world.resource::<ObservedTimings>().unwrap().0;
    assert_eq!(samples[0].1, TimingOutcome::Idle);
    assert!(!samples[0].0.has_phase(FramePhase::Flush));
    assert_eq!(samples[1].1, TimingOutcome::Failed);
    assert!(samples[1].0.has_phase(FramePhase::Flush));
    assert!(!samples[1].0.has_phase(FramePhase::SeedPrev));
    app.backend.fail = None;
    app.render_dirty().unwrap();
    assert_eq!(
        app.world
            .resource::<crate::ecs::FrameStats>()
            .unwrap()
            .len(),
        count + 1
    );
}

#[test]
fn suspended_quit_and_maintenance_errors_close_the_owning_operation() {
    let (mut app, _) = fixture();
    app.add_plugin(ObserveTimings);
    app.backend.fail = Some("maintain");
    app.draw_tick();
    assert_eq!(
        app.world
            .resource::<ObservedTimings>()
            .unwrap()
            .0
            .last()
            .unwrap()
            .1,
        TimingOutcome::Failed
    );
    app.backend.fail = None;
    app.suspend();
    app.update_tick();
    assert_eq!(
        app.world
            .resource::<ObservedTimings>()
            .unwrap()
            .0
            .last()
            .unwrap()
            .1,
        TimingOutcome::Suspended
    );
    app.quitting = true;
    assert!(app.tick());
    assert_eq!(
        app.world
            .resource::<ObservedTimings>()
            .unwrap()
            .0
            .last()
            .unwrap()
            .1,
        TimingOutcome::Quit
    );
}

#[test]
fn operation_wall_includes_business_hooks_but_excludes_reporting_and_pacing() {
    std::thread_local! { static NOW: core::cell::Cell<u64> = const { core::cell::Cell::new(0) }; }
    struct AdvanceClock;
    impl Plugin<CheckedSurface, SwRendererFactory> for AdvanceClock {
        fn build(&mut self, app: &mut App<CheckedSurface>) {
            app.enable_timing_reports();
            app.world
                .insert_resource(crate::ecs::MonoClock::new(|| NOW.get()));
        }
        fn post_render(&mut self, _: &mut World, _: Option<u64>) {
            NOW.set(NOW.get() + 5_000);
        }
        fn on_timing(&mut self, _: &mut World, sample: &crate::ecs::TimingSample<'_>) {
            assert_eq!(sample.wall_nanos, Some(5_000));
            NOW.set(NOW.get() + 100_000);
        }
    }
    NOW.set(0);
    let (mut app, _) = fixture();
    app.add_plugin(AdvanceClock);
    app.add_plugin(plugins::FrameRateCapPlugin::with_sleep_fn(60, |millis| {
        NOW.set(NOW.get() + millis * 1_000_000);
    }));
    for _ in 0..2 {
        app.backend.invalid = true;
        app.tick();
    }
    assert!(NOW.get() > 210_000);
}

fn assert_submission(trace: &[&str], finish: &[&str]) {
    assert_eq!(&trace[..2], &["begin", "draw"]);
    let end = trace.len() - finish.len();
    assert!(end > 2);
    assert!(trace[2..end].iter().all(|&step| step == "flush"));
    assert_eq!(&trace[end..], finish);
}

#[test]
fn surface_failures_preserve_the_accepted_baseline_and_retry_latest_state() {
    for entry in 0..3 {
        for failure in ["begin", "flush", "end"] {
            let (mut app, node) = fixture();
            let baseline = app.world.get::<PaintedCoverage>(node).unwrap().0.outer;
            let pixels = app
                .backend
                .pixels
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            crate::ui::set_position(&mut app.world, node, 16, 0);
            app.backend.fail = Some(failure);
            for _ in 0..3 {
                assert!(!app.dirty_regions().unwrap().is_empty());
            }
            assert!(
                app.backend.trace.is_empty(),
                "queries must not acquire a surface"
            );
            let result = match entry {
                0 => app.render(),
                1 => app.render_dirty(),
                _ => app.snapshot().map(|_| ()),
            };
            assert!(result.is_err());
            assert_eq!(
                app.world.get::<PaintedCoverage>(node).unwrap().0.outer,
                baseline
            );
            assert_eq!(app.backend.trace.last(), Some(&"abort"));
            assert_eq!(app.needs_full_redraw, failure != "begin");
            if failure == "begin" {
                assert_eq!(&app.backend.trace, &["begin", "abort"]);
                assert_eq!(
                    app.backend
                        .pixels
                        .framebuffer()
                        .read_pixels()
                        .unwrap()
                        .bytes(),
                    pixels
                );
            }
            crate::ui::set_position(&mut app.world, node, 20, 0);
            app.backend.fail = None;
            app.backend.trace.clear();
            app.render_dirty().unwrap();
            assert_submission(&app.backend.trace, &["end"]);
            assert_eq!(
                app.world.get::<PaintedCoverage>(node).unwrap().0.outer.x,
                Fixed::from_int(20)
            );
            assert!(app.dirty_regions().unwrap().is_empty());
            let accepted = app
                .backend
                .pixels
                .framebuffer()
                .read_pixels()
                .unwrap()
                .bytes()
                .to_vec();
            app.render().unwrap();
            assert_eq!(
                app.backend
                    .pixels
                    .framebuffer()
                    .read_pixels()
                    .unwrap()
                    .bytes(),
                accepted
            );
        }
    }
}

#[test]
fn idle_and_failed_ticks_still_run_surface_maintenance() {
    let (mut app, node) = fixture();
    assert!(!app.tick());
    assert_eq!(&app.backend.trace, &["maintain"]);
    app.backend.trace.clear();
    app.backend.fail = Some("end");
    crate::ui::set_position(&mut app.world, node, 16, 0);
    assert!(!app.tick());
    assert_submission(&app.backend.trace, &["end", "abort", "maintain"]);
}

std::thread_local! {
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
    static NOW: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    #[cfg_attr(target_os = "android", expect(clippy::missing_const_for_thread_local, reason = "Rust 1.97 Android TLS expansion flags an explicit const initializer"))]
    static SLEEPS: core::cell::Cell<u32> = const { core::cell::Cell::new(0) };
}

#[derive(Default)]
struct TickCount(u32);

#[crate::system]
fn count_ticks(count: res!(mut TickCount)) {
    count.0 += 1;
}

#[test]
fn pacing_covers_idle_failed_and_successful_ticks_without_stopping_systems() {
    let (mut app, node) = fixture();
    NOW.set(0);
    SLEEPS.set(0);
    app.world
        .insert_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    app.world.insert_resource(TickCount::default());
    app.add_system(count_ticks::system());
    app.add_plugin(plugins::FrameRateCapPlugin::with_sleep_fn(100, |ns| {
        NOW.set(NOW.get() + ns);
        SLEEPS.set(SLEEPS.get() + 1);
    }));
    app.tick();
    assert_eq!(SLEEPS.get(), 0);
    app.tick();
    assert_eq!(SLEEPS.get(), 1);
    crate::ui::set_position(&mut app.world, node, 16, 0);
    app.backend.fail = Some("begin");
    app.tick();
    assert_eq!(SLEEPS.get(), 2);
    app.backend.fail = None;
    app.tick();
    assert_eq!(SLEEPS.get(), 3);
    assert_eq!(app.world.resource::<TickCount>().unwrap().0, 4);
    assert!(app.dirty_regions().unwrap().is_empty());
}

#[test]
fn host_updates_and_platform_redraws_do_not_advance_each_other() {
    let (mut app, _) = fixture();
    NOW.set(0);
    app.world.insert_resource(TickCount::default());
    app.add_system(count_ticks::system());
    assert_eq!(app.tick_wait_ns(), None);
    app.add_plugin(plugins::FrameRateCapPlugin::with_sleep_fn(100, |_| {
        panic!("host must not sleep")
    }));
    assert_eq!(app.tick_wait_ns(), None);
    app.world
        .insert_resource(crate::ecs::MonoClock::new(|| NOW.get()));
    assert_eq!(app.tick_wait_ns(), Some(0));
    assert!(!app.update_tick());
    assert_eq!(app.tick_wait_ns(), Some(10_000_000));
    for _ in 0..3 {
        app.backend.invalid = true;
        app.draw_tick();
        assert_eq!(app.world.resource::<TickCount>().unwrap().0, 1);
        assert_eq!(app.tick_wait_ns(), Some(10_000_000));
    }
    NOW.set(10_000_000);
    assert_eq!(app.tick_wait_ns(), Some(0));
    assert!(!app.update_tick());
    assert_eq!(app.world.resource::<TickCount>().unwrap().0, 2);
    assert_eq!(app.tick_wait_ns(), Some(10_000_000));
}

#[test]
fn redraw_queries_do_not_consume_requests_and_transient_frames_can_idle() {
    for transient in [false, true] {
        let (mut app, node) = fixture();
        app.backend.transient = transient;
        for _ in 0..3 {
            assert!(app.dirty_regions().unwrap().is_empty());
            app.tick();
        }
        assert_eq!(&app.backend.trace, &["maintain"; 3]);
        app.backend.trace.clear();
        app.backend.invalid = true;
        for _ in 0..3 {
            assert_eq!(
                app.dirty_regions().unwrap().rects.as_slice(),
                &[Rect::new(0, 0, 40, 30)]
            );
            assert!(app.backend.invalid);
        }
        assert!(app.backend.trace.is_empty());
        app.backend.fail = Some("end");
        assert!(app.render_dirty().is_err());
        assert!(app.backend.invalid);
        app.backend.fail = None;
        crate::ui::set_position(&mut app.world, node, 20, 0);
        app.render_dirty().unwrap();
        assert!(!app.backend.invalid);
        assert!(app.dirty_regions().unwrap().is_empty());
        crate::ui::set_position(&mut app.world, node, 24, 0);
        if transient {
            assert_eq!(
                app.dirty_regions().unwrap().rects.as_slice(),
                &[Rect::new(0, 0, 40, 30)]
            );
        }
        app.render_dirty().unwrap();
        assert!(app.dirty_regions().unwrap().is_empty());
    }
}
