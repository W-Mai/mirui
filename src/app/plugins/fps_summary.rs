use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::{FrameOperation, FramePhase, FramePhases, FrameStats, TimingSample, World};
use crate::surface::Surface;

const KINDS: [FrameOperation; 4] = [
    FrameOperation::Tick,
    FrameOperation::Update,
    FrameOperation::Draw,
    FrameOperation::Maintenance,
];
const PHASES: [FramePhase; 6] = [
    FramePhase::Input,
    FramePhase::Systems,
    FramePhase::Layout,
    FramePhase::Render,
    FramePhase::Flush,
    FramePhase::SeedPrev,
];

/// Reports closed operation wall time and cadence. Phase sums and operation rates are not display FPS or CPU time.
///
/// **Inserts**
/// - hooks: `on_timing`
pub struct FpsSummaryPlugin {
    samples_per_summary: u32,
    samples: u32,
    totals: [OperationSummary; 4],
    window: SummaryWindow,
    on_summary: fn(FpsSummary<'_>),
    cpu: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Average {
    total: u128,
    count: u64,
}
impl Average {
    fn add(&mut self, value: Option<u64>) {
        if let Some(value) = value {
            self.total += u128::from(value);
            self.count += 1;
        }
    }
    fn value(&self) -> Option<u64> {
        (self.count != 0).then(|| (self.total / u128::from(self.count)) as u64)
    }
}

/// One operation kind, retaining separate valid sample counts for each measurement.
#[derive(Clone, Copy, Debug, Default)]
pub struct OperationSummary {
    pub operation: FrameOperation,
    pub count: u64,
    pub window_operations: u64,
    pub instrumented_operations: u64,
    pub latest_phases: FramePhases,
    wall: Average,
    cpu: Average,
    phases: [Average; 6],
    outcomes: [u64; 5],
}
impl OperationSummary {
    pub fn outcome_count(&self, outcome: crate::ecs::TimingOutcome) -> u64 {
        self.outcomes[outcome as usize]
    }
    pub fn wall_nanos(&self) -> Option<u64> {
        self.wall.value()
    }
    pub fn wall_samples(&self) -> u64 {
        self.wall.count
    }
    pub fn thread_cpu_nanos(&self) -> Option<u64> {
        (self.cpu.count == self.count)
            .then(|| self.cpu.value())
            .flatten()
    }
    pub fn cpu_samples(&self) -> u64 {
        self.cpu.count
    }
    /// Mean over valid CPU samples only; compare against their coverage count before combining it with another metric.
    pub fn partial_thread_cpu_nanos(&self) -> Option<u64> {
        self.cpu.value()
    }
    pub fn phase_nanos(&self, phase: FramePhase) -> Option<u64> {
        self.phases[phase as usize].value()
    }
    pub fn phase_samples(&self, phase: FramePhase) -> u64 {
        self.phases[phase as usize].count
    }
}

/// Operation summaries within one report window. Rates count operations in a half-open clock interval; idle operations remain distinct from accepted or displayed frames.
pub struct FpsSummary<'a> {
    pub samples: u32,
    pub operations: &'a [OperationSummary; 4],
    pub wall_ns: Option<u64>,
    pub stats: Option<&'a FrameStats>,
}
impl FpsSummary<'_> {
    pub fn operation_rate_hz(&self, operation: FrameOperation) -> Option<f64> {
        self.wall_ns.filter(|&ns| ns != 0).map(|ns| {
            self.operations[operation as usize].window_operations as f64 * 1_000_000_000.0
                / ns as f64
        })
    }
}

impl FpsSummaryPlugin {
    pub fn new(samples_per_summary: u32) -> Self {
        Self {
            samples_per_summary: samples_per_summary.max(1),
            samples: 0,
            totals: core::array::from_fn(|i| OperationSummary {
                operation: KINDS[i],
                ..OperationSummary::default()
            }),
            window: SummaryWindow::default(),
            on_summary: default_summary,
            cpu: false,
        }
    }
    pub fn with_sink(mut self, sink: fn(FpsSummary<'_>)) -> Self {
        self.on_summary = sink;
        self
    }
    /// Enable coarse thread CPU timing without requiring detailed tracing.
    pub fn with_thread_cpu_time(mut self, enabled: bool) -> Self {
        self.cpu = enabled;
        self
    }
}
impl Default for FpsSummaryPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for FpsSummaryPlugin {
    fn build(&mut self, app: &mut App<B, F>) {
        app.enable_timing_reports();
        if self.cpu {
            app.enable_cpu_timing();
        }
    }
    fn on_timing(&mut self, world: &mut World, sample: &TimingSample<'_>) {
        self.window.observe(sample.finished_at_ns, sample.operation);
        let total = &mut self.totals[sample.operation as usize];
        total.count += 1;
        total.instrumented_operations += u64::from(sample.instrumented);
        total.outcomes[sample.outcome as usize] += 1;
        total.latest_phases = sample.phases.phases();
        total.wall.add(sample.wall_nanos);
        total.cpu.add(sample.thread_cpu_nanos);
        for phase in PHASES {
            total.phases[phase as usize].add(sample.phases.phase_nanos(phase));
        }
        self.samples += 1;
        if self.samples < self.samples_per_summary {
            return;
        }
        let (counts, wall_ns) = self.window.close();
        for (total, count) in self.totals.iter_mut().zip(counts) {
            total.window_operations = count;
        }
        (self.on_summary)(FpsSummary {
            samples: self.samples,
            operations: &self.totals,
            wall_ns,
            stats: world.resource::<FrameStats>(),
        });
        self.samples = 0;
        self.totals = core::array::from_fn(|i| OperationSummary {
            operation: KINDS[i],
            ..OperationSummary::default()
        });
    }
}

fn default_summary(report: FpsSummary<'_>) {
    for op in report.operations.iter().filter(|op| op.count != 0) {
        crate::info!(target: "mirui::perf", "{:?} count={} rate={:?}/s wall={:?}ns (before reporting/pacing) cpu={:?}ns instrumented={}; display rate unavailable", op.operation, op.count, report.operation_rate_hz(op.operation), op.wall_nanos(), op.thread_cpu_nanos(), op.instrumented_operations);
        for phase in PHASES {
            if let Some(value) = op.phase_nanos(phase) {
                crate::info!(target: "mirui::perf", "  {:?} wall={}ns samples={}", phase, value, op.phase_samples(phase));
            }
        }
        if let Some(stats) = report
            .stats
            .and_then(|stats| stats.summary(op.operation, op.latest_phases))
        {
            crate::info!(target: "mirui::perf", "  phase budget {:?} samples={} avg={}ns p99={}ns jitter={}ns", op.latest_phases, stats.count, stats.avg_nanos, stats.p99_nanos, stats.jitter_nanos());
        }
    }
}

/// Keeps boundary events for the next window, including equal timestamps.
#[derive(Default)]
struct SummaryWindow {
    start: Option<u64>,
    latest: Option<u64>,
    events: [u64; 4],
    boundary_events: [u64; 4],
    invalid: bool,
}

impl SummaryWindow {
    fn observe(&mut self, now: Option<u64>, kind: FrameOperation) {
        let Some(now) = now else {
            self.invalid = true;
            self.latest = None;
            return;
        };
        if self.latest.is_some_and(|last| now < last) {
            self.invalid = true;
        }
        self.start.get_or_insert(now);
        if self.latest != Some(now) {
            self.boundary_events = [0; 4];
        }
        self.boundary_events[kind as usize] += 1;
        self.latest = Some(now);
        self.events[kind as usize] += 1;
    }

    fn close(&mut self) -> ([u64; 4], Option<u64>) {
        if self.invalid {
            *self = Self {
                start: self.latest,
                latest: self.latest,
                events: if self.latest.is_some() {
                    self.boundary_events
                } else {
                    [0; 4]
                },
                boundary_events: if self.latest.is_some() {
                    self.boundary_events
                } else {
                    [0; 4]
                },
                ..Self::default()
            };
            return ([0; 4], None);
        }
        let elapsed = self
            .start
            .zip(self.latest)
            .and_then(|(start, end)| end.checked_sub(start))
            .filter(|&ns| ns != 0);
        let Some(elapsed) = elapsed else {
            return ([0; 4], None);
        };
        let count = core::array::from_fn(|i| self.events[i] - self.boundary_events[i]);
        self.start = self.latest;
        self.events = self.boundary_events;
        (count, Some(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameOperation, SummaryWindow};

    #[test]
    fn rate_windows_count_boundaries_once() {
        let mut window = SummaryWindow::default();
        window.observe(Some(0), FrameOperation::Tick);
        assert_eq!(window.close(), ([0; 4], None));
        window.observe(Some(10), FrameOperation::Tick);
        window.observe(Some(20), FrameOperation::Tick);
        assert_eq!(window.close(), ([2, 0, 0, 0], Some(20)));
        window.observe(Some(30), FrameOperation::Tick);
        window.observe(Some(40), FrameOperation::Tick);
        assert_eq!(window.close(), ([2, 0, 0, 0], Some(20)));
    }

    #[test]
    fn equal_timestamps_remain_in_the_next_half_open_window() {
        let mut window = SummaryWindow::default();
        for t in [10, 10, 20, 20, 20] {
            window.observe(Some(t), FrameOperation::Tick);
        }
        assert_eq!(window.close(), ([2, 0, 0, 0], Some(10)));
        assert_eq!(window.close(), ([0; 4], None));
        window.observe(Some(30), FrameOperation::Tick);
        assert_eq!(window.close(), ([3, 0, 0, 0], Some(10)));
    }

    #[test]
    fn missing_or_rewound_clock_invalidates_the_entire_window() {
        for gap in [None, Some(5)] {
            let mut window = SummaryWindow::default();
            window.observe(Some(10), FrameOperation::Tick);
            window.observe(gap, FrameOperation::Tick);
            window.observe(Some(20), FrameOperation::Tick);
            assert_eq!(window.close(), ([0; 4], None));
            window.observe(Some(30), FrameOperation::Tick);
            assert_eq!(window.close(), ([1, 0, 0, 0], Some(10)));
        }
    }
}
