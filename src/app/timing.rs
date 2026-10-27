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
}

impl<B: Surface, F: RendererFactory<B>> App<B, F> {
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
