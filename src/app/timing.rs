use super::{App, RendererFactory};
use crate::ecs::{
    FrameOperation, FramePhase, FrameStats, FrameTimings, TimingOutcome, TimingSample,
};
use crate::surface::Surface;

#[derive(Default)]
pub(super) struct OperationTiming {
    active: bool,
    start: Option<u64>,
    pub(super) phases: FrameTimings,
    pub(super) outcome: TimingOutcome,
    pub(super) report: bool,
    cpu: bool,
    accepted: u64,
    accepted_last_ns: Option<u64>,
    accepted_last_count: u64,
}

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
    pub(super) fn record_acceptance(&mut self) {
        crate::core::perf::accepted();
        if !self.timing.report {
            return;
        }
        let now = self.timing_clock_ns();
        self.timing.accepted += 1;
        let Some((last, now)) = self
            .timing
            .accepted_last_ns
            .zip(now)
            .filter(|(last, now)| now >= last)
        else {
            self.timing.accepted_last_ns = None;
            return;
        };
        if now == last {
            self.timing.accepted_last_count += 1;
        } else {
            self.timing.accepted_last_count = 1;
        }
        self.timing.accepted_last_ns = Some(now);
    }

    pub(crate) fn enable_timing_reports(&mut self) {
        self.timing.report = true;
    }
    pub(crate) fn enable_cpu_timing(&mut self) {
        self.timing.cpu = true;
    }

    pub(super) fn timing_clock_ns(&self) -> Option<u64> {
        self.world
            .resource::<crate::ecs::MonoClock>()
            .map(|clock| clock.now_ns())
            .or_else(crate::core::time::try_clock_now_ns)
    }

    pub(super) fn record_phase(&mut self, phase: FramePhase, start: Option<u64>, end: Option<u64>) {
        self.timing.phases.record(phase, start, end);
    }

    pub(super) fn timing_outcome(&mut self, outcome: TimingOutcome) {
        if self.timing.outcome != TimingOutcome::Failed {
            self.timing.outcome = outcome;
        }
    }

    pub(super) fn with_timing<R>(
        &mut self,
        kind: FrameOperation,
        action: impl FnOnce(&mut Self) -> R,
    ) -> R {
        if self.timing.active {
            return action(self);
        }
        self.timing.active = true;
        self.timing.phases = FrameTimings::new(
            kind,
            self.world.resource::<crate::ecs::MonoClock>().is_some()
                || crate::core::time::is_clock_installed(),
        );
        self.timing.outcome = TimingOutcome::Idle;
        let instrumented = crate::core::perf::is_enabled();
        let trace = instrumented
            .then(|| crate::core::perf::operation(self.world.id().diagnostic_id(), kind));
        let measure_cpu =
            self.timing.cpu || (instrumented && crate::core::perf::thread_cpu_time_enabled());
        self.timing.start = self.timing.report.then(|| self.timing_clock_ns()).flatten();
        if self.timing.report {
            self.timing.accepted = 0;
            self.timing.accepted_last_ns = self.timing.start;
            self.timing.accepted_last_count = 0;
        }
        let cpu_start = measure_cpu.then(crate::core::perf::cpu::now_ns).flatten();
        let result = action(self);
        let thread_cpu_nanos = cpu_start
            .zip(measure_cpu.then(crate::core::perf::cpu::now_ns).flatten())
            .and_then(|(a, b)| b.checked_sub(a));
        let end = self.timing.report.then(|| self.timing_clock_ns()).flatten();
        let wall_nanos = self
            .timing
            .start
            .zip(end)
            .and_then(|(a, b)| b.checked_sub(a));
        if self.timing.report && wall_nanos.is_none() {
            self.timing.phases.invalidate_clock();
        }
        if self.quitting {
            self.timing.outcome = TimingOutcome::Quit;
        } else if self.suspended {
            self.timing.outcome = TimingOutcome::Suspended;
        }
        let phases = self.timing.phases;
        let outcome = self.timing.outcome;
        self.timing.active = false;
        let trace_id = trace.and_then(|trace| trace.close_operation(outcome, thread_cpu_nanos));
        self.publish_timings(&phases, outcome);
        if self.timing.report {
            let _context = crate::core::perf::report_context(trace_id);
            let sample = TimingSample {
                operation: kind,
                wall_nanos,
                thread_cpu_nanos,
                instrumented,
                outcome,
                phases: &phases,
                finished_at_ns: end,
                accepted_submissions: self.timing.accepted,
                accepted_at_finish: self
                    .timing
                    .accepted_last_ns
                    .zip(end)
                    .filter(|(last, end)| last <= end)
                    .map(|(last, end)| {
                        if last == end {
                            self.timing.accepted_last_count
                        } else {
                            0
                        }
                    }),
            };
            for plugin in &mut self.plugins {
                plugin.on_timing(&mut self.world, &sample);
            }
        }
        result
    }

    pub(super) fn publish_timings(&mut self, phases: &FrameTimings, outcome: TimingOutcome) {
        if let Some(current) = self.world.resource_mut::<FrameTimings>() {
            *current = *phases;
        } else {
            self.world.insert_resource(*phases);
        }
        if outcome == TimingOutcome::Completed && phases.phase_wall_nanos().is_some() {
            if let Some(stats) = self.world.resource_mut::<FrameStats>() {
                stats.push(phases);
            } else {
                let mut stats = FrameStats::default();
                stats.push(phases);
                self.world.insert_resource(stats);
            }
        }
    }

    pub(super) fn record_render_result<T>(
        &mut self,
        result: &Result<T, crate::render::RenderError>,
    ) {
        use crate::render::RenderError;
        if let Err(error) = result {
            let outcome = if matches!(
                error,
                RenderError::ResourcePending | RenderError::SurfaceUnavailable
            ) {
                TimingOutcome::Idle
            } else {
                TimingOutcome::Failed
            };
            self.timing_outcome(outcome);
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::app::plugin::Plugin;
    use crate::ecs::World;
    use std::{cell::RefCell, rc::Rc};

    struct Samples(Rc<RefCell<Vec<(u64, TimingOutcome)>>>);

    impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for Samples {
        fn build(&mut self, app: &mut App<B, F>) {
            app.enable_timing_reports();
        }
        fn on_timing(&mut self, _: &mut World, sample: &TimingSample<'_>) {
            self.0
                .borrow_mut()
                .push((sample.accepted_submissions, sample.outcome));
        }
    }

    #[test]
    fn nested_draws_count_actual_acceptances_even_before_later_failure() {
        let mut app = App::headless(8, 8);
        app.with_default_widgets();
        app.spawn_root().id();
        let samples = Rc::new(RefCell::new(Vec::new()));
        app.add_plugin(Samples(samples.clone()));
        app.with_timing(FrameOperation::Tick, |app| {
            app.render().unwrap();
            app.render().unwrap();
            app.timing_outcome(TimingOutcome::Failed);
        });
        app.render_dirty().unwrap();
        assert_eq!(
            &*samples.borrow(),
            &[(2, TimingOutcome::Failed), (0, TimingOutcome::Idle)]
        );
    }
}
