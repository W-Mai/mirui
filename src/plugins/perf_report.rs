//! Per-frame perf reporting.
//!
//! Drains span events from [`crate::perf`] each frame, optionally
//! streams them as chrome-JSON lines through a user-supplied sink
//! (open the captured stream in <https://ui.perfetto.dev>), and
//! periodically emits an aggregated `PerfReport` containing
//! per-name span totals plus per-system scheduler timings.

use crate::app::{App, RendererFactory};
use crate::core::perf::StageStat;
use crate::ecs::World;
use crate::plugin::Plugin;
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

/// `App::run` consumes this each frame: when `true`, the scheduler's
/// per-system perf counters reset before the next window begins.
#[derive(Default, Clone, Copy)]
pub struct PerfResetFlag(pub bool);

#[derive(Clone)]
pub struct PerfReport {
    pub frames: u32,
    pub stage_stats: alloc::vec::Vec<StageStat>,
    pub systems: alloc::vec::Vec<SystemStat>,
}

/// Per-frame perf reporter: accumulates span and per-system timings,
/// emits a `PerfReport` callback every `frames_per_report` frames,
/// optionally writes Chrome-trace JSON to a file.
///
/// **Inserts**
/// - resource: `PerfAccum` (private)
/// - system: none
/// - view: none
/// - entity: none
/// - hooks: `post_render` (drains `crate::perf` events; emits report at window boundary)
pub struct PerfReportPlugin {
    frames_per_report: u32,
    frame_count: u32,
    on_report: fn(report: &PerfReport),
    perfetto_line_sink: Option<PerfettoLineSink>,
}

/// Writer for the perfetto line sink: one JSON event per call, no
/// trailing newline (the sink decides how to terminate).
pub type PerfettoLineSink = alloc::boxed::Box<dyn FnMut(&str)>;

impl PerfReportPlugin {
    pub fn new(frames_per_report: u32) -> Self {
        Self {
            frames_per_report,
            frame_count: 0,
            on_report: default_sink,
            perfetto_line_sink: None,
        }
    }

    pub fn with_sink(mut self, sink: fn(&PerfReport)) -> Self {
        self.on_report = sink;
        self
    }

    /// Write each drained event as one chrome-JSON line through `sink`.
    /// Open the resulting NDJSON in <https://ui.perfetto.dev>. Works
    /// on `no_std`; ESP backends pass an `esp_println` shim.
    pub fn with_perfetto_line_sink(mut self, sink: PerfettoLineSink) -> Self {
        self.perfetto_line_sink = Some(sink);
        self
    }

    /// `with_perfetto_line_sink` over a freshly-truncated file. The
    /// sink receives one `\n`-joined batch per frame, written through
    /// as-is.
    #[cfg(feature = "std")]
    pub fn with_perfetto_writer(self, path: impl AsRef<std::path::Path>) -> Self {
        use std::io::Write as _;
        let mut f = std::fs::File::create(path).expect("failed to create perfetto trace file");
        self.with_perfetto_line_sink(alloc::boxed::Box::new(move |batch: &str| {
            let _ = f.write_all(batch.as_bytes());
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
        app.world.insert_resource(PerfAccum::default());
        // Opt-in flag for App::snapshot_system_perf.
        app.world.insert_resource(SystemPerfSnapshot::default());
        // Opens the trace_span!/trace_fn recording path.
        crate::core::perf::set_enabled(true);
    }

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        let events = crate::core::perf::drain_events();

        if let Some(sink) = self.perfetto_line_sink.as_mut() {
            // The sink may be expensive per call on `no_std` targets
            // (critical section, FIFO flush, ...); batch the frame's
            // events into one buffer so that overhead is paid once.
            let mut frame_buf = alloc::string::String::with_capacity(events.len() * 96 + 16);
            let mut line = alloc::string::String::with_capacity(128);
            for ev in &events {
                line.clear();
                if crate::core::perf::format_chrome_event(ev, &mut line).is_ok() {
                    frame_buf.push_str(&line);
                    frame_buf.push('\n');
                }
            }
            if !frame_buf.is_empty() {
                sink(&frame_buf);
            }
        }

        if let Some(acc) = world.resource_mut::<PerfAccum>() {
            // Aggregate per-frame so events don't need to be retained
            // across the whole report window.
            for ev in &events {
                let dur = ev.end_ns.saturating_sub(ev.start_ns);
                if let Some(s) = acc.stage_stats.iter_mut().find(|s| s.name == ev.name) {
                    s.count += 1;
                    s.total_ns += dur;
                    s.last_ns = dur;
                    if dur < s.min_ns {
                        s.min_ns = dur;
                    }
                    if dur > s.max_ns {
                        s.max_ns = dur;
                    }
                } else {
                    acc.stage_stats.push(StageStat {
                        name: ev.name,
                        count: 1,
                        total_ns: dur,
                        last_ns: dur,
                        min_ns: dur,
                        max_ns: dur,
                    });
                }
            }
        }

        self.frame_count += 1;
        if self.frame_count < self.frames_per_report {
            return;
        }

        let stage_stats = world
            .resource::<PerfAccum>()
            .map(|a| a.stage_stats.clone())
            .unwrap_or_default();
        let report = PerfReport {
            frames: self.frame_count,
            stage_stats,
            systems: collect_system_stats(world),
        };
        (self.on_report)(&report);

        self.frame_count = 0;
        if let Some(a) = world.resource_mut::<PerfAccum>() {
            a.stage_stats.clear();
        }
        if let Some(snap) = world.resource_mut::<SystemPerfSnapshot>() {
            snap.entries.clear();
        }
        world.insert_resource(PerfResetFlag(true));
    }
}

#[derive(Default, Clone)]
struct PerfAccum {
    stage_stats: alloc::vec::Vec<StageStat>,
}

fn collect_system_stats(world: &World) -> alloc::vec::Vec<SystemStat> {
    world
        .resource::<SystemPerfSnapshot>()
        .map(|s| s.entries.clone())
        .unwrap_or_default()
}

#[cfg(feature = "std")]
fn default_sink(report: &PerfReport) {
    eprintln!("[perf] {} frames", report.frames);
    let mut sorted: alloc::vec::Vec<&StageStat> = report.stage_stats.iter().collect();
    sorted.sort_by_key(|s| core::cmp::Reverse(s.total_ns));
    for s in &sorted {
        let avg = if s.count == 0 {
            0
        } else {
            s.total_ns / s.count as u64
        };
        eprintln!(
            "[perf]   {:>26}  n={:<4} total {:>7}µs  avg {:>5}µs  min {:>5}µs  max {:>5}µs",
            s.name,
            s.count,
            s.total_ns / 1_000,
            avg / 1_000,
            s.min_ns / 1_000,
            s.max_ns / 1_000,
        );
    }
    for s in &report.systems {
        if s.avg_us == 0 && s.last_us == 0 {
            continue;
        }
        eprintln!(
            "[perf]   {:>26}  last {:>5}µs  avg {:>5}µs  n={}",
            s.name, s.last_us, s.avg_us, s.call_count
        );
    }
}

#[cfg(not(feature = "std"))]
fn default_sink(_report: &PerfReport) {}
