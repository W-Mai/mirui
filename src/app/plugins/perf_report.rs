//! Per-operation perf reporting.
//!
//! Drains span events from [`crate::core::perf`] each closed operation, optionally
//! streams them as chrome-JSON lines through a user-supplied sink
//! (open the captured stream in <https://ui.perfetto.dev>), and
//! periodically emits an aggregated `PerfReport` containing
//! per-name span totals plus per-system scheduler timings.

use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::core::perf::{Capture, CaptureStatus, DEFAULT_TRACE_BUDGET, StageStat};
use crate::ecs::World;
use crate::surface::Surface;

/// Snapshot of per-system perf counters written by `App::run` after
/// every `systems.run_all`. Plugins read this without needing direct
/// scheduler access.
#[derive(Default, Clone)]
pub struct SystemPerfSnapshot {
    pub entries: alloc::vec::Vec<SystemStat>,
}

#[derive(Clone, Copy)]
pub struct SystemStat {
    pub name: &'static str,
    pub priority: i32,
    pub last_us: u32,
    pub avg_us: u32,
    pub call_count: u32,
}

/// `App::run` consumes this each closed operation: when `true`, the scheduler's
/// per-system perf counters reset before the next window begins.
#[derive(Default, Clone, Copy)]
pub struct PerfResetFlag(pub bool);

#[derive(Clone)]
pub struct PerfReport<'a> {
    pub operations: u32,
    pub stage_stats: &'a [StageStat],
    pub systems: &'a [SystemStat],
    pub capture: CaptureStatus,
}

/// Per-operation perf reporter: accumulates span and per-system timings,
/// emits a `PerfReport` callback every `operations_per_report` operations,
/// optionally writes Chrome-trace JSON to a file.
///
/// **Inserts**
/// - resources: `SystemPerfSnapshot`, `PerfResetFlag`
/// - system: none
/// - view: none
/// - entity: none
/// - hooks: `on_timing` (drains `crate::core::perf` events; emits report at window boundary)
pub struct PerfReportPlugin {
    operations_per_report: u32,
    operation_count: u32,
    on_report: fn(report: &PerfReport),
    perfetto_line_sink: Option<PerfettoLineSink>,
    trace_budget: usize,
    capture: Option<Capture>,
    cpu: bool,
}

/// Writer for bounded batches of complete newline-terminated JSON records. One operation can produce multiple batches; sinks must preserve their order.
pub type PerfettoLineSink = alloc::boxed::Box<dyn FnMut(&str)>;

impl PerfReportPlugin {
    pub fn new(operations_per_report: u32) -> Self {
        Self {
            operations_per_report: operations_per_report.max(1),
            operation_count: 0,
            on_report: default_sink,
            perfetto_line_sink: None,
            trace_budget: DEFAULT_TRACE_BUDGET,
            capture: None,
            cpu: false,
        }
    }

    /// Set the total diagnostic bytes per registered producer, including export workspace.
    pub fn with_trace_budget(mut self, bytes: usize) -> Self {
        self.trace_budget = bytes;
        self
    }

    /// Read thread CPU clocks at operation, submission and wait boundaries when supported. Ordinary spans keep wall-only recording.
    pub fn with_thread_cpu_time(mut self, enabled: bool) -> Self {
        self.cpu = enabled;
        self
    }

    pub fn with_sink(mut self, sink: fn(&PerfReport)) -> Self {
        self.on_report = sink;
        self
    }

    /// Write bounded newline-delimited JSON batches through `sink`. Open the resulting NDJSON in <https://ui.perfetto.dev>. Works on `no_std`; ESP backends pass an `esp_println` shim. Sink-owned memory is outside the trace budget.
    pub fn with_perfetto_line_sink(mut self, sink: PerfettoLineSink) -> Self {
        self.perfetto_line_sink = Some(sink);
        self
    }

    /// Write bounded JSON batches to a freshly truncated file. File errors are logged and stop further writes to this file.
    #[cfg(feature = "std")]
    pub fn with_perfetto_writer(self, path: impl AsRef<std::path::Path>) -> Self {
        use std::io::Write as _;
        let mut f = std::fs::File::create(path).expect("failed to create perfetto trace file");
        let mut failed = false;
        self.with_perfetto_line_sink(alloc::boxed::Box::new(move |batch: &str| {
            if !failed && let Err(error) = f.write_all(batch.as_bytes()) {
                failed = true;
                crate::warn!(target: "mirui::perf", "trace write failed: {}", error);
            }
        }))
    }
}

impl Default for PerfReportPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B, F> Plugin<B, F> for PerfReportPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        match Capture::new(self.trace_budget) {
            Ok(mut capture) => {
                capture.set_thread_cpu_time(self.cpu);
                self.capture = Some(capture);
            }
            Err(error) => {
                crate::warn!(target: "mirui::perf", "trace capture not installed: {:?}", error);
                return;
            }
        }
        app.enable_timing_reports();
        if self.cpu {
            app.enable_cpu_timing();
        }
        app.world.insert_resource(SystemPerfSnapshot::default());
        app.world.insert_resource(PerfResetFlag(false));
    }

    fn on_timing(&mut self, world: &mut World, _sample: &crate::ecs::TimingSample<'_>) {
        let Some(capture) = &mut self.capture else {
            return;
        };
        capture.drain(
            self.perfetto_line_sink
                .as_mut()
                .map(|sink| &mut **sink as &mut dyn FnMut(&str)),
        );
        self.operation_count += 1;
        if self.operation_count < self.operations_per_report {
            return;
        }
        capture.sort_stage_stats();
        crate::core::perf::observe_report(|| {
            (self.on_report)(&PerfReport {
                operations: self.operation_count,
                stage_stats: capture.stage_stats(),
                systems: world
                    .resource::<SystemPerfSnapshot>()
                    .map_or(&[], |s| &s.entries),
                capture: capture.status(),
            })
        });
        self.operation_count = 0;
        capture.clear_stage_stats();
        if let Some(snapshot) = world.resource_mut::<SystemPerfSnapshot>() {
            snapshot.entries.clear();
        }
        if let Some(reset) = world.resource_mut::<PerfResetFlag>() {
            reset.0 = true;
        }
    }
}

impl Drop for PerfReportPlugin {
    fn drop(&mut self) {
        if let Some(capture) = &mut self.capture {
            capture.stop();
            capture.drain(
                self.perfetto_line_sink
                    .as_mut()
                    .map(|sink| &mut **sink as &mut dyn FnMut(&str)),
            );
        }
    }
}

fn default_sink(report: &PerfReport) {
    crate::info!(target: "mirui::perf", "{} operations", report.operations);
    crate::info!(target: "mirui::perf", "inclusive wall totals (overlap); capture {:?}", report.capture);
    for s in report.stage_stats {
        let avg = if s.count == 0 {
            0
        } else {
            s.total_ns / s.count as u64
        };
        crate::info!(
            target: "mirui::perf",
            "  {:>26}  n={:<4} total {:>7}µs  avg {:>5}µs  min {:>5}µs  max {:>5}µs",
            s.name,
            s.count,
            s.total_ns / 1_000,
            avg / 1_000,
            s.min_ns / 1_000,
            s.max_ns / 1_000,
        );
    }
    for s in report.systems {
        if s.avg_us == 0 && s.last_us == 0 {
            continue;
        }
        crate::info!(
            target: "mirui::perf",
            "  {:>26}  last {:>5}µs  avg {:>5}µs  n={}",
            s.name, s.last_us, s.avg_us, s.call_count
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn reports_borrow_reused_storage_and_duplicate_consumers_do_not_reset_it() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let mut app = App::headless(8, 8);
        app.with_default_widgets();
        app.spawn_root().id();
        app.add_plugin(PerfReportPlugin::new(1).with_sink(|report| {
            assert_eq!(report.operations, 1);
            assert!(report.stage_stats.iter().any(|s| s.name == "frame.full"));
        }));
        app.world.resource_mut::<PerfResetFlag>().unwrap().0 = true;
        app.add_plugin(PerfReportPlugin::new(1).with_sink(|_| panic!("second consumer ran")));
        assert!(app.world.resource::<PerfResetFlag>().unwrap().0);
        app.render().unwrap();
        let allocations = crate::test_support::tracked_allocations(|| app.render().unwrap());
        assert_eq!(allocations, 0);
    }
}
