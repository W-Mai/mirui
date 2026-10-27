#![cfg(feature = "std")]

#[test]
fn render_span_ends_before_surface_submission_and_reporting() {
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
