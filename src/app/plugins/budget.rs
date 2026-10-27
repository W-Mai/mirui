use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::surface::Surface;

/// Watches `FrameStats` and fires a sink whenever avg or p99
/// recorded phase wall time exceeds a configured budget. `budget_*_ns = 0`
/// disables that threshold.
///
/// **Inserts**
/// - hooks: `on_timing` (every `operations_per_check` completed operations, reads
///   `FrameStats` and may invoke the sink)
pub struct BudgetReportPlugin {
    operations_per_check: u32,
    operation_count: u32,
    budget_avg_ns: u64,
    budget_p99_ns: u64,
    on_violation: fn(report: BudgetViolation),
}

/// Snapshot handed to the [`BudgetReportPlugin`] sink when a
/// threshold is breached. Sinks may compare `avg_ns` / `p99_ns`
/// against `budget_avg_ns` / `budget_p99_ns` to format the message.
pub struct BudgetViolation {
    pub operation: crate::ecs::FrameOperation,
    pub phases: crate::ecs::FramePhases,
    pub avg_ns: u64,
    pub p99_ns: u64,
    pub jitter_ns: u64,
    pub budget_avg_ns: u64,
    pub budget_p99_ns: u64,
}

impl BudgetReportPlugin {
    /// Check the current operation's phase budget every `operations_per_check` completed operations.
    pub fn new(operations_per_check: u32) -> Self {
        Self {
            operations_per_check: operations_per_check.max(1),
            operation_count: 0,
            budget_avg_ns: 0,
            budget_p99_ns: 0,
            on_violation: default_violation,
        }
    }

    pub fn with_avg_budget(mut self, ns: u64) -> Self {
        self.budget_avg_ns = ns;
        self
    }

    pub fn with_p99_budget(mut self, ns: u64) -> Self {
        self.budget_p99_ns = ns;
        self
    }

    pub fn with_sink(mut self, sink: fn(BudgetViolation)) -> Self {
        self.on_violation = sink;
        self
    }
}

impl Default for BudgetReportPlugin {
    fn default() -> Self {
        Self::new(60)
    }
}

impl<B, F> Plugin<B, F> for BudgetReportPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        app.enable_timing_reports();
    }

    fn on_timing(&mut self, world: &mut World, sample: &crate::ecs::TimingSample<'_>) {
        if sample.outcome != crate::ecs::TimingOutcome::Completed
            || sample.phases.phase_wall_nanos().is_none()
        {
            return;
        }
        self.operation_count += 1;
        if self.operation_count < self.operations_per_check {
            return;
        }
        self.operation_count = 0;
        let Some(stats) = world.resource::<crate::ecs::FrameStats>() else {
            return;
        };
        let Some(stats) = stats.summary(sample.operation, sample.phases.phases()) else {
            return;
        };
        let avg = stats.avg_nanos;
        let p99 = stats.p99_nanos;
        let avg_over = self.budget_avg_ns != 0 && avg > self.budget_avg_ns;
        let p99_over = self.budget_p99_ns != 0 && p99 > self.budget_p99_ns;
        if avg_over || p99_over {
            (self.on_violation)(BudgetViolation {
                operation: sample.operation,
                phases: sample.phases.phases(),
                avg_ns: avg,
                p99_ns: p99,
                jitter_ns: stats.jitter_nanos(),
                budget_avg_ns: self.budget_avg_ns,
                budget_p99_ns: self.budget_p99_ns,
            });
        }
    }
}

fn default_violation(v: BudgetViolation) {
    crate::warn!(
        target: "mirui::budget",
        "{:?} {:?} phase budget avg {}us (budget {}us) p99 {}us (budget {}us) jitter {}us",
        v.operation,
        v.phases,
        v.avg_ns / 1000,
        v.budget_avg_ns / 1000,
        v.p99_ns / 1000,
        v.budget_p99_ns / 1000,
        v.jitter_ns / 1000,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::FrameStats;
    use crate::ecs::World;
    use core::sync::atomic::{AtomicU32, Ordering};

    fn run(plugin: &mut BudgetReportPlugin, world: &mut World, frames: u32) {
        for _ in 0..frames {
            <BudgetReportPlugin as Plugin<
                crate::surface::framebuf::FramebufSurface<fn(&[u8], crate::types::PhysicalRect)>,
                crate::app::SwRendererFactory,
            >>::on_timing(
                plugin,
                world,
                &crate::ecs::TimingSample {
                    operation: crate::ecs::FrameOperation::Tick,
                    wall_nanos: Some(1),
                    thread_cpu_nanos: None,
                    instrumented: false,
                    outcome: crate::ecs::TimingOutcome::Completed,
                    phases: &timings(1),
                    finished_at_ns: Some(1),
                },
            );
        }
    }

    fn timings(value: u64) -> crate::ecs::FrameTimings {
        let mut t = crate::ecs::FrameTimings::new(crate::ecs::FrameOperation::Tick, true);
        t.record(crate::ecs::FramePhase::Render, Some(0), Some(value));
        t
    }

    fn fixture(samples: &[u64]) -> World {
        let mut world = World::new();
        let mut stats = FrameStats::default();
        for &v in samples {
            stats.push(&timings(v));
        }
        world.insert_resource(stats);
        world
    }

    static FIRES_AVG: AtomicU32 = AtomicU32::new(0);
    static FIRES_UNDER: AtomicU32 = AtomicU32::new(0);
    static FIRES_P99: AtomicU32 = AtomicU32::new(0);
    static FIRES_ZERO: AtomicU32 = AtomicU32::new(0);

    fn sink_avg(_v: BudgetViolation) {
        FIRES_AVG.fetch_add(1, Ordering::Relaxed);
    }
    fn sink_under(_v: BudgetViolation) {
        FIRES_UNDER.fetch_add(1, Ordering::Relaxed);
    }
    fn sink_p99(_v: BudgetViolation) {
        FIRES_P99.fetch_add(1, Ordering::Relaxed);
    }
    fn sink_zero(_v: BudgetViolation) {
        FIRES_ZERO.fetch_add(1, Ordering::Relaxed);
    }

    #[test]
    fn fires_when_avg_over_budget() {
        FIRES_AVG.store(0, Ordering::Relaxed);
        let mut world = fixture(&[20_000_000; 60]);
        let mut p = BudgetReportPlugin::new(60)
            .with_avg_budget(16_000_000)
            .with_sink(sink_avg);
        run(&mut p, &mut world, 60);
        assert_eq!(FIRES_AVG.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn does_not_fire_under_budget() {
        FIRES_UNDER.store(0, Ordering::Relaxed);
        let mut world = fixture(&[10_000_000; 60]);
        let mut p = BudgetReportPlugin::new(60)
            .with_avg_budget(16_000_000)
            .with_p99_budget(20_000_000)
            .with_sink(sink_under);
        run(&mut p, &mut world, 60);
        assert_eq!(FIRES_UNDER.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn fires_on_p99_alone() {
        FIRES_P99.store(0, Ordering::Relaxed);
        let mut samples = [10_000_000_u64; 256];
        for s in samples.iter_mut().take(10) {
            *s = 50_000_000;
        }
        let mut world = fixture(&samples);
        let mut p = BudgetReportPlugin::new(60)
            .with_p99_budget(20_000_000)
            .with_sink(sink_p99);
        run(&mut p, &mut world, 60);
        assert_eq!(FIRES_P99.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn zero_budget_disables_check() {
        FIRES_ZERO.store(0, Ordering::Relaxed);
        let mut world = fixture(&[100_000_000; 60]);
        let mut p = BudgetReportPlugin::new(60).with_sink(sink_zero);
        run(&mut p, &mut world, 60);
        assert_eq!(FIRES_ZERO.load(Ordering::Relaxed), 0);
    }
}
