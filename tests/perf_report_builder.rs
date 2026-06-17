//! `PerfReportPlugin` builders compose: `with_sink` and a perfetto
//! line/file sink can both be set on the same plugin without one
//! erasing the other.

use std::cell::Cell;
use std::sync::atomic::{AtomicU32, Ordering};

use mirui::app::App;
use mirui::app::plugins::{PerfReport, PerfReportPlugin};
use mirui::ecs::Entity;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Rect;

fn noop_flush(_: &[u8], _: &Rect) {}

static REPORT_CALLS_LINE: AtomicU32 = AtomicU32::new(0);
static REPORT_CALLS_WRITER: AtomicU32 = AtomicU32::new(0);

fn record_line(_r: &PerfReport) {
    REPORT_CALLS_LINE.fetch_add(1, Ordering::Relaxed);
}

fn record_writer(_r: &PerfReport) {
    REPORT_CALLS_WRITER.fetch_add(1, Ordering::Relaxed);
}

#[test]
fn report_sink_and_perfetto_line_sink_both_run() {
    REPORT_CALLS_LINE.store(0, Ordering::Relaxed);
    let backend = FramebufSurface::new(32, 32, noop_flush);
    let mut app = App::new(backend);
    app.with_default_widgets();

    let line_count: std::rc::Rc<Cell<u32>> = Default::default();
    let line_count_for_sink = std::rc::Rc::clone(&line_count);
    let line_sink: mirui::app::plugins::PerfettoLineSink =
        Box::new(move |_line: &str| line_count_for_sink.set(line_count_for_sink.get() + 1));

    app.add_plugin(
        PerfReportPlugin::new(1)
            .with_sink(record_line)
            .with_perfetto_line_sink(line_sink),
    );

    let root = Entity {
        id: 0,
        generation: 0,
    };
    app.set_root(root);

    {
        mirui::trace_span!("test.builder_compose");
    }

    app.render();

    assert_eq!(REPORT_CALLS_LINE.load(Ordering::Relaxed), 1);
    assert!(line_count.get() >= 1, "perfetto sink saw no events");
}

#[test]
fn with_perfetto_writer_keeps_report_sink() {
    REPORT_CALLS_WRITER.store(0, Ordering::Relaxed);
    let backend = FramebufSurface::new(32, 32, noop_flush);
    let mut app = App::new(backend);
    app.with_default_widgets();

    let path = std::env::temp_dir().join("mirui_perf_report_builder.ndjson");
    let _ = std::fs::remove_file(&path);

    app.add_plugin(
        PerfReportPlugin::new(1)
            .with_sink(record_writer)
            .with_perfetto_writer(&path),
    );

    let root = Entity {
        id: 0,
        generation: 0,
    };
    app.set_root(root);
    {
        mirui::trace_span!("test.writer_compose");
    }
    app.render();

    assert_eq!(REPORT_CALLS_WRITER.load(Ordering::Relaxed), 1);
    let body = std::fs::read_to_string(&path).expect("writer must create file");
    assert!(body.contains(r#""name":"#), "expected at least one event");
    let _ = std::fs::remove_file(&path);
}
