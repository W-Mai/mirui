use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::plugin::Plugin;
use crate::surface::Surface;

/// Periodic console summary of `FrameTimings` averages every N frames.
/// Reads `FrameTimings`, `FrameStats`, and the `crate::perf` event
/// stream so a single plugin can serve both desktop (default `eprintln`
/// sink) and embedded targets (custom sink writing to UART / LCD /
/// log). Without a clock plugin installed, all timings stay zero and
/// the line is harmless noise.
///
/// **Inserts**
/// - resource: none (reads `FrameTimings` / `FrameStats` written by `App::run`)
/// - system: none
/// - view: none
/// - entity: none
/// - hooks: `post_render` (per-frame accumulation + periodic summary)
pub struct FpsSummaryPlugin {
    frames_per_summary: u32,
    frame_count: u32,
    totals: StageTotals,
    on_summary: fn(report: FpsSummary<'_>),
}

#[derive(Default)]
struct StageTotals {
    frame_ns: u64,
    event_poll_ns: u64,
    systems_ns: u64,
    layout_ns: u64,
    render_ns: u64,
    flush_ns: u64,
    seed_prev_ns: u64,
}

impl StageTotals {
    fn add(&mut self, t: &crate::ecs::FrameTimings) {
        self.frame_ns += t.frame_nanos;
        self.event_poll_ns += t.event_poll_nanos;
        self.systems_ns += t.systems_nanos;
        self.layout_ns += t.layout_nanos;
        self.render_ns += t.render_nanos;
        self.flush_ns += t.flush_nanos;
        self.seed_prev_ns += t.seed_prev_nanos;
    }
}

/// Snapshot handed to the [`FpsSummaryPlugin`] sink each window.
/// `avg_*_ns` values are per-frame averages over `frames`. `stats`
/// is the 256-frame sliding window for jitter / p99. `perf_events`
/// is the drained `crate::perf` event stream covering this window;
/// callers run [`crate::perf::aggregate`] on it for per-span totals.
pub struct FpsSummary<'a> {
    pub frames: u32,
    pub avg_frame_ns: u64,
    pub avg_event_poll_ns: u64,
    pub avg_systems_ns: u64,
    pub avg_layout_ns: u64,
    pub avg_render_ns: u64,
    pub avg_flush_ns: u64,
    pub avg_seed_prev_ns: u64,
    pub stats: Option<&'a crate::ecs::FrameStats>,
    pub perf_events: alloc::vec::Vec<crate::perf::PerfEvent>,
}

impl FpsSummaryPlugin {
    pub fn new(frames_per_summary: u32) -> Self {
        Self {
            frames_per_summary,
            frame_count: 0,
            totals: StageTotals::default(),
            on_summary: default_summary,
        }
    }

    /// Override where the summary line goes — log file, LCD overlay, etc.
    pub fn with_sink(mut self, sink: fn(FpsSummary<'_>)) -> Self {
        self.on_summary = sink;
        self
    }
}

impl Default for FpsSummaryPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B, F> Plugin<B, F> for FpsSummaryPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        // Read FrameTimings (written by App::run); skip the very first
        // post_render that fires before the run loop has populated it.
        let Some(t) = world.resource::<crate::ecs::FrameTimings>().copied() else {
            return;
        };
        self.frame_count += 1;
        self.totals.add(&t);

        if self.frame_count >= self.frames_per_summary {
            let n = self.frame_count as u64;
            let avg = |total: u64| total / n;
            (self.on_summary)(FpsSummary {
                frames: self.frame_count,
                avg_frame_ns: avg(self.totals.frame_ns),
                avg_event_poll_ns: avg(self.totals.event_poll_ns),
                avg_systems_ns: avg(self.totals.systems_ns),
                avg_layout_ns: avg(self.totals.layout_ns),
                avg_render_ns: avg(self.totals.render_ns),
                avg_flush_ns: avg(self.totals.flush_ns),
                avg_seed_prev_ns: avg(self.totals.seed_prev_ns),
                stats: world.resource::<crate::ecs::FrameStats>(),
                perf_events: crate::perf::drain_events(),
            });
            self.frame_count = 0;
            self.totals = StageTotals::default();
        }
    }
}

#[cfg(feature = "std")]
fn default_summary(report: FpsSummary<'_>) {
    let fps = if report.avg_frame_ns == 0 {
        0.0
    } else {
        1_000_000_000.0 / report.avg_frame_ns as f64
    };
    eprintln!(
        "[fps] {} frames | frame {}us ({:.1} fps) = event {} + systems {} + layout {} + render {} + flush {} + seed {}",
        report.frames,
        report.avg_frame_ns / 1000,
        fps,
        report.avg_event_poll_ns / 1000,
        report.avg_systems_ns / 1000,
        report.avg_layout_ns / 1000,
        report.avg_render_ns / 1000,
        report.avg_flush_ns / 1000,
        report.avg_seed_prev_ns / 1000,
    );
    if let Some(s) = report.stats {
        eprintln!(
            "[fps] window={} min {}us max {}us p99 {}us jitter {}us",
            s.len(),
            s.min() / 1000,
            s.max() / 1000,
            s.p99() / 1000,
            s.jitter() / 1000,
        );
    }
    if !report.perf_events.is_empty() {
        let stats = crate::perf::aggregate(&report.perf_events);
        for s in &stats {
            eprintln!(
                "[fps] {:24} count {:>5}  avg {:>5}us  max {:>5}us",
                s.name,
                s.count,
                (s.total_ns / s.count as u64) / 1000,
                s.max_ns / 1000,
            );
        }
    }
}

#[cfg(not(feature = "std"))]
fn default_summary(_report: FpsSummary<'_>) {
    // No-op on bare metal — users override with_sink to route to an
    // overlay / UART. ESP demos pass a sink calling esp_println.
}
