use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::surface::Surface;

/// Reports recorded phase wall times and render callback cadence. Neither measurement describes display completion or CPU execution time.
///
/// **Inserts**
/// - hooks: `post_render`
pub struct FpsSummaryPlugin {
    frames_per_summary: u32,
    frame_count: u32,
    totals: StageTotals,
    window: SummaryWindow,
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
        self.frame_ns += t.phase_wall_nanos;
        self.input_ns += t.input_nanos;
        self.systems_ns += t.systems_nanos;
        self.layout_ns += t.layout_nanos;
        self.render_ns += t.render_nanos;
        self.flush_ns += t.flush_nanos;
        self.seed_prev_ns += t.seed_prev_nanos;
    }
}

/// Recorded phase wall-time averages over `samples`, with callback cadence measured independently in a half-open clock window. Phase sums omit work outside the recorded phases and must not be inverted to estimate FPS. `stats` retains phase sums, not display intervals.
///
/// Trace events have a single consumer; this plugin does not drain them.
pub struct FpsSummary<'a> {
    pub samples: u32,
    pub avg_phase_wall_ns: u64,
    pub avg_input_ns: u64,
    pub avg_systems_ns: u64,
    pub avg_layout_ns: u64,
    pub avg_render_ns: u64,
    pub avg_flush_ns: u64,
    pub avg_seed_prev_ns: u64,
    pub wall_ns: Option<u64>,
    /// Callbacks in the rate window, excluding events at its closing boundary.
    pub window_callbacks: u64,
    pub stats: Option<&'a crate::ecs::FrameStats>,
}

impl FpsSummary<'_> {
    /// Render callback frequency, including idle callbacks; not a display frame rate.
    pub fn callback_rate_hz(&self) -> Option<f64> {
        self.wall_ns
            .filter(|&ns| ns != 0)
            .map(|ns| self.window_callbacks as f64 * 1_000_000_000.0 / ns as f64)
    }
}

/// Keeps boundary events for the next window, including equal timestamps.
#[derive(Default)]
struct SummaryWindow {
    start: Option<u64>,
    latest: Option<u64>,
    events: u64,
    boundary_events: u64,
    invalid: bool,
}

impl SummaryWindow {
    fn observe(&mut self, now: Option<u64>) {
        let Some(now) = now else {
            self.invalid = true;
            self.latest = None;
            return;
        };
        if self.latest.is_some_and(|last| now < last) {
            self.invalid = true;
        }
        self.start.get_or_insert(now);
        self.boundary_events = if self.latest == Some(now) {
            self.boundary_events.saturating_add(1)
        } else {
            1
        };
        self.latest = Some(now);
        self.events = self.events.saturating_add(1);
    }

    fn close(&mut self) -> (u64, Option<u64>) {
        if self.invalid {
            *self = Self {
                start: self.latest,
                latest: self.latest,
                events: if self.latest.is_some() {
                    self.boundary_events
                } else {
                    0
                },
                boundary_events: if self.latest.is_some() {
                    self.boundary_events
                } else {
                    0
                },
                ..Self::default()
            };
            return (0, None);
        }
        let elapsed = self
            .start
            .zip(self.latest)
            .and_then(|(start, end)| end.checked_sub(start))
            .filter(|&ns| ns != 0);
        let Some(elapsed) = elapsed else {
            return (0, None);
        };
        let count = self.events - self.boundary_events;
        self.start = self.latest;
        self.events = self.boundary_events;
        (count, Some(elapsed))
    }
}

impl FpsSummaryPlugin {
    pub fn new(frames_per_summary: u32) -> Self {
        Self {
            frames_per_summary,
            frame_count: 0,
            totals: StageTotals::default(),
            window: SummaryWindow::default(),
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
        self.window.observe(now_ns);
        self.frame_count += 1;
        self.totals.add(&t);

        if self.frame_count >= self.frames_per_summary {
            let n = self.frame_count as u64;
            let avg = |total: u64| total / n;
            let (window_callbacks, wall_ns) = self.window.close();
            (self.on_summary)(FpsSummary {
                samples: self.frame_count,
                avg_phase_wall_ns: avg(self.totals.frame_ns),
                avg_input_ns: avg(self.totals.input_ns),
                avg_systems_ns: avg(self.totals.systems_ns),
                avg_layout_ns: avg(self.totals.layout_ns),
                avg_render_ns: avg(self.totals.render_ns),
                avg_flush_ns: avg(self.totals.flush_ns),
                avg_seed_prev_ns: avg(self.totals.seed_prev_ns),
                wall_ns,
                window_callbacks,
                stats: world.resource::<crate::ecs::FrameStats>(),
            });
            self.frame_count = 0;
            self.totals = StageTotals::default();
        }
    }
}

fn default_summary(report: FpsSummary<'_>) {
    if let Some(rate) = report.callback_rate_hz() {
        crate::info!(target: "mirui::fps", "render callbacks {:.1}/s; display rate unavailable", rate);
    } else {
        crate::info!(target: "mirui::fps", "render callback rate unavailable; display rate unavailable");
    }
    crate::info!(
        target: "mirui::fps",
        "{} samples | phase wall {}us = input {} + systems {} + layout {} + render {} + flush {} + seed {}",
        report.samples,
        report.avg_phase_wall_ns / 1000,
        report.avg_input_ns / 1000,
        report.avg_systems_ns / 1000,
        report.avg_layout_ns / 1000,
        report.avg_render_ns / 1000,
        report.avg_flush_ns / 1000,
        report.avg_seed_prev_ns / 1000,
    );
    if let Some(s) = report.stats {
        crate::info!(
            target: "mirui::fps",
            "window={} min {}us max {}us p99 {}us jitter {}us",
            s.len(),
            s.min() / 1000,
            s.max() / 1000,
            s.p99() / 1000,
            s.jitter() / 1000,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::SummaryWindow;

    #[test]
    fn rate_windows_count_boundaries_once() {
        let mut window = SummaryWindow::default();
        window.observe(Some(0));
        assert_eq!(window.close(), (0, None));
        window.observe(Some(10));
        window.observe(Some(20));
        assert_eq!(window.close(), (2, Some(20)));
        window.observe(Some(30));
        window.observe(Some(40));
        assert_eq!(window.close(), (2, Some(20)));
    }

    #[test]
    fn equal_timestamps_remain_in_the_next_half_open_window() {
        let mut window = SummaryWindow::default();
        for t in [10, 10, 20, 20, 20] {
            window.observe(Some(t));
        }
        assert_eq!(window.close(), (2, Some(10)));
        assert_eq!(window.close(), (0, None));
        window.observe(Some(30));
        assert_eq!(window.close(), (3, Some(10)));
    }

    #[test]
    fn missing_or_rewound_clock_invalidates_the_entire_window() {
        for gap in [None, Some(5)] {
            let mut window = SummaryWindow::default();
            window.observe(Some(10));
            window.observe(gap);
            window.observe(Some(20));
            assert_eq!(window.close(), (0, None));
            window.observe(Some(30));
            assert_eq!(window.close(), (1, Some(10)));
        }
    }
}
