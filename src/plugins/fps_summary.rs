use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::plugin::Plugin;
use crate::surface::Surface;

/// Calls a sink with averaged `FrameTimings` + a borrow of
/// `FrameStats` every N frames.
///
/// **Inserts**
/// - hooks: `post_render`
pub struct FpsSummaryPlugin {
    frames_per_summary: u32,
    frame_count: u32,
    totals: StageTotals,
    window_start_ns: Option<u64>,
    on_summary: fn(report: FpsSummary<'_>),
}

#[derive(Default)]
struct StageTotals {
    frame_ns: u64,
    input_ns: u64,
    systems_ns: u64,
    layout_ns: u64,
    render_ns: u64,
    flush_ns: u64,
    seed_prev_ns: u64,
}

impl StageTotals {
    fn add(&mut self, t: &crate::ecs::FrameTimings) {
        self.frame_ns += t.frame_nanos;
        self.input_ns += t.input_nanos;
        self.systems_ns += t.systems_nanos;
        self.layout_ns += t.layout_nanos;
        self.render_ns += t.render_nanos;
        self.flush_ns += t.flush_nanos;
        self.seed_prev_ns += t.seed_prev_nanos;
    }
}

/// Snapshot handed to the [`FpsSummaryPlugin`] sink each window.
/// `avg_*_ns` are per-frame averages over `frames`; `stats` is the
/// 256-frame sliding window for jitter / p99.
///
/// `frames * 1e9 / wall_ns` is the visible frame rate (covers idle
/// frames and any frame-rate cap sleep); `1e9 / avg_frame_ns` is the
/// "could-go" rate ignoring idle skips and pacing. `wall_ns` is
/// `None` if no `MonoClock` resource is installed.
///
/// Sinks that want per-span detail call `crate::core::perf::drain_events()`
/// explicitly — the plugin doesn't pre-drain, because that single
/// global stream is also where `PerfReportPlugin` reads from.
pub struct FpsSummary<'a> {
    pub frames: u32,
    pub avg_frame_ns: u64,
    pub avg_input_ns: u64,
    pub avg_systems_ns: u64,
    pub avg_layout_ns: u64,
    pub avg_render_ns: u64,
    pub avg_flush_ns: u64,
    pub avg_seed_prev_ns: u64,
    pub wall_ns: Option<u64>,
    pub stats: Option<&'a crate::ecs::FrameStats>,
}

impl FpsSummaryPlugin {
    pub fn new(frames_per_summary: u32) -> Self {
        Self {
            frames_per_summary,
            frame_count: 0,
            totals: StageTotals::default(),
            window_start_ns: None,
            on_summary: default_summary,
        }
    }

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
        let now_ns = world
            .resource::<crate::ecs::MonoClock>()
            .map(|c| c.now_ns());
        if self.window_start_ns.is_none() {
            self.window_start_ns = now_ns;
        }
        self.frame_count += 1;
        self.totals.add(&t);

        if self.frame_count >= self.frames_per_summary {
            let n = self.frame_count as u64;
            let avg = |total: u64| total / n;
            let wall_ns = match (self.window_start_ns, now_ns) {
                (Some(start), Some(end)) if end > start => Some(end - start),
                _ => None,
            };
            (self.on_summary)(FpsSummary {
                frames: self.frame_count,
                avg_frame_ns: avg(self.totals.frame_ns),
                avg_input_ns: avg(self.totals.input_ns),
                avg_systems_ns: avg(self.totals.systems_ns),
                avg_layout_ns: avg(self.totals.layout_ns),
                avg_render_ns: avg(self.totals.render_ns),
                avg_flush_ns: avg(self.totals.flush_ns),
                avg_seed_prev_ns: avg(self.totals.seed_prev_ns),
                wall_ns,
                stats: world.resource::<crate::ecs::FrameStats>(),
            });
            self.frame_count = 0;
            self.totals = StageTotals::default();
            self.window_start_ns = None;
        }
    }
}

#[cfg(feature = "std")]
fn default_summary(report: FpsSummary<'_>) {
    let work_fps = if report.avg_frame_ns == 0 {
        0.0
    } else {
        1_000_000_000.0 / report.avg_frame_ns as f64
    };
    let wall_fps = match report.wall_ns {
        Some(ns) if ns > 0 => f64::from(report.frames) * 1_000_000_000.0 / ns as f64,
        _ => 0.0,
    };
    eprintln!(
        "[fps] {} frames | wall {:.1} fps | work {}us ({:.1} fps) = input {} + systems {} + layout {} + render {} + flush {} + seed {}",
        report.frames,
        wall_fps,
        report.avg_frame_ns / 1000,
        work_fps,
        report.avg_input_ns / 1000,
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
}

#[cfg(not(feature = "std"))]
fn default_summary(_report: FpsSummary<'_>) {}
