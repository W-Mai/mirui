use crate::app::plugin::Plugin;
use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::surface::Surface;

/// Watches `FrameStats` and fires a sink whenever avg or p99
/// frame time exceeds a configured budget. `budget_*_ns = 0`
/// disables that threshold.
///
/// **Inserts**
/// - hooks: `post_render` (every `frames_per_check` frames, reads
///   `FrameStats` and may invoke the sink)
pub struct BudgetReportPlugin {
    frames_per_check: u32,
    frame_count: u32,
    budget_avg_ns: u64,
    budget_p99_ns: u64,
    on_violation: fn(report: BudgetViolation),
}

/// Snapshot handed to the [`BudgetReportPlugin`] sink when a
/// threshold is breached. Sinks may compare `avg_ns` / `p99_ns`
/// against `budget_avg_ns` / `budget_p99_ns` to format the message.
pub struct BudgetViolation {
    pub avg_ns: u64,
    pub p99_ns: u64,
    pub jitter_ns: u64,
    pub budget_avg_ns: u64,
    pub budget_p99_ns: u64,
}

impl BudgetReportPlugin {
    /// Create a plugin that checks every `frames_per_check` frames.
    pub fn new(frames_per_check: u32) -> Self {
        Self {
            frames_per_check,
            frame_count: 0,
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
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn post_render(&mut self, world: &mut World, _render_nanos: u64) {
        self.frame_count += 1;
        if self.frame_count < self.frames_per_check {
            return;
        }
        self.frame_count = 0;
        let Some(stats) = world.resource::<crate::ecs::FrameStats>() else {
            return;
        };
        if stats.is_empty() {
            return;
        }
        let avg = stats.avg();
        let p99 = stats.p99();
        let avg_over = self.budget_avg_ns != 0 && avg > self.budget_avg_ns;
        let p99_over = self.budget_p99_ns != 0 && p99 > self.budget_p99_ns;
        if avg_over || p99_over {
            (self.on_violation)(BudgetViolation {
                avg_ns: avg,
                p99_ns: p99,
                jitter_ns: stats.jitter(),
                budget_avg_ns: self.budget_avg_ns,
                budget_p99_ns: self.budget_p99_ns,
            });
        }
    }
}

#[cfg(feature = "std")]
fn default_violation(v: BudgetViolation) {
    crate::warn!(
        target: "mirui::budget",
        "avg {}us (budget {}us) p99 {}us (budget {}us) jitter {}us",
        v.avg_ns / 1000,
        v.budget_avg_ns / 1000,
        v.p99_ns / 1000,
        v.budget_p99_ns / 1000,
        v.jitter_ns / 1000,
    );
}

#[cfg(not(feature = "std"))]
fn default_violation(_v: BudgetViolation) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::FrameStats;
    use crate::ecs::World;
    use core::sync::atomic::{AtomicU32, Ordering};

    fn run(plugin: &mut BudgetReportPlugin, world: &mut World, frames: u32) {
        for _ in 0..frames {
            <BudgetReportPlugin as Plugin<
                crate::surface::framebuf::FramebufSurface<fn(&[u8], &crate::types::Rect)>,
                crate::app::SwRendererFactory,
            >>::post_render(plugin, world, 0);
        }
    }

    fn fixture(samples: &[u64]) -> World {
        let mut world = World::new();
        let mut stats = FrameStats::default();
        for &v in samples {
            stats.push(v);
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
