#![cfg(feature = "std")]

static CAPTURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn capture_start_inside_business_hook_waits_for_the_next_operation() {
    use mirui::app::{App, RendererFactory, plugin::Plugin};
    use mirui::core::perf;
    use mirui::ecs::{FrameStats, FrameTimings, World};
    use mirui::surface::Surface;
    use std::{cell::RefCell, rc::Rc};

    let _capture = CAPTURE.lock().unwrap();
    struct StartCapture {
        started: bool,
        capture: Rc<RefCell<Option<perf::Capture>>>,
    }
    impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for StartCapture {
        fn build(&mut self, _: &mut App<B, F>) {}
        fn pre_render(&mut self, _: &mut World) {
            if !self.started {
                self.started = true;
                *self.capture.borrow_mut() =
                    Some(perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap());
            }
        }
    }
    let mut app = App::headless(16, 16);
    app.with_default_widgets();
    app.spawn_root().id();
    let capture = Rc::new(RefCell::new(None));
    app.add_plugin(StartCapture {
        started: false,
        capture: capture.clone(),
    });
    app.render().unwrap();
    assert!(app.world.resource::<FrameTimings>().is_none());
    app.render().unwrap();
    assert!(
        app.world
            .resource::<FrameTimings>()
            .unwrap()
            .phase_wall_nanos()
            .is_some()
    );
    drop(capture.borrow_mut().take());
    app.render().unwrap();
    assert!(
        app.world
            .resource::<FrameTimings>()
            .unwrap()
            .phase_wall_nanos()
            .is_none()
    );
    assert!(app.world.resource::<FrameStats>().unwrap().is_empty());
}

#[test]
fn render_span_ends_before_surface_submission_and_reporting() {
    let _capture = CAPTURE.lock().unwrap();
    use mirui::core::perf;

    let mut app = mirui::app::App::headless(16, 16);
    app.with_default_widgets();
    app.spawn_root().id();
    let mut capture = perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap();
    let result = app.render_dirty();
    capture.stop();
    result.unwrap();
    let mut events = Vec::new();
    capture.drain_events(|_, batch| events.extend(batch.iter().filter_map(|r| r.span().copied())));
    let span = |name| events.iter().find(|event| event.name == name).unwrap();
    let render = span("frame.render_region");
    let flush = span("frame.flush");
    let report = span("frame.post_render");
    assert!(render.end_ns <= flush.start_ns);
    assert!(flush.end_ns <= report.start_ns);
    let render_index = events
        .iter()
        .position(|event| event.name == render.name)
        .unwrap();
    let flush_index = events
        .iter()
        .position(|event| event.name == flush.name)
        .unwrap();
    assert!(render_index < flush_index);
}

#[test]
fn direct_capture_controls_operation_cpu_without_a_report_plugin() {
    use mirui::core::perf::{self, RecordData, ScopeKind};

    let _capture = CAPTURE.lock().unwrap();
    let mut app = mirui::app::App::headless(16, 16);
    app.with_default_widgets();
    app.spawn_root().id();
    let mut capture = perf::Capture::new(perf::DEFAULT_TRACE_BUDGET).unwrap();
    for enabled in [false, true, false] {
        capture.set_thread_cpu_time(enabled);
        app.render().unwrap();
        let mut records = Vec::new();
        capture.drain_events(|_, batch| records.extend_from_slice(batch));
        let operation = records
            .iter()
            .find_map(|record| match record.data {
                RecordData::Scope {
                    id,
                    kind: ScopeKind::Operation(..),
                    ..
                } => Some(id),
                _ => None,
            })
            .unwrap();
        let cpu = records.iter().find_map(|record| match record.data {
            RecordData::ThreadCpu { scope, nanos } if scope == operation => Some(nanos),
            _ => None,
        });
        let supported = cfg!(any(
            target_os = "linux",
            target_os = "android",
            target_os = "macos",
            target_os = "ios",
            target_os = "windows"
        ));
        assert_eq!(cpu.is_some(), enabled && supported);
        assert_eq!(
            records
                .iter()
                .filter(|r| matches!(r.data,
            RecordData::ThreadCpu { scope, .. } if scope == operation))
                .count(),
            usize::from(enabled && supported)
        );
    }
    capture.stop();
    capture.drain_events(|_, _| {});
    assert_eq!(capture.status().dropped, 0);
    assert_eq!(capture.status().open_scopes, 0);
    app.render().unwrap();
    assert!(
        app.world
            .resource::<mirui::ecs::FrameTimings>()
            .unwrap()
            .phase_wall_nanos()
            .is_none()
    );
    assert!(
        app.world
            .resource::<mirui::ecs::FrameStats>()
            .unwrap()
            .is_empty()
    );
}
