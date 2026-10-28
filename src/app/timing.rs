use super::{App, RendererFactory};
use crate::ecs::{
    FrameOperation, FramePhase, FrameStats, FrameTimings, TimingOutcome, TimingSample,
};
use crate::surface::Surface;

#[derive(Default)]
pub(super) struct OperationTiming {
    active: bool,
    measuring: bool,
    reporting: bool,
    requested: bool,
    published: bool,
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
    /// Request timing samples and statistics without installing a reporter. The request applies to the next outer operation. Releasing it does not disable registered reporters or an active trace capture.
    ///
    /// When no demand remains, the last sample becomes unavailable and the statistics window becomes empty. Capture changes are reconciled at the next App operation; an explicit release outside an operation clears results immediately.
    pub fn request_timings(&mut self, requested: bool) -> &mut Self {
        self.timing.requested = requested;
        if !requested
            && !self.timing.active
            && !self.timing.report
            && !crate::core::perf::is_enabled()
        {
            self.clear_timings();
        }
        self
    }

    fn clear_timings(&mut self) {
        if !self.timing.published {
            return;
        }
        if let Some(current) = self.world.resource_mut::<FrameTimings>() {
            *current = FrameTimings::default();
        }
        if let Some(stats) = self.world.resource_mut::<FrameStats>() {
            stats.clear();
        }
        self.timing.published = false;
    }

    pub(super) fn record_acceptance(&mut self) {
        crate::core::perf::accepted();
        if !self.timing.reporting {
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
        if !self.timing.measuring {
            return None;
        }
        self.world
            .resource::<crate::ecs::MonoClock>()
            .map(|clock| clock.now_ns())
            .or_else(crate::core::time::try_clock_now_ns)
    }

    pub(super) fn record_phase(&mut self, phase: FramePhase, start: Option<u64>, end: Option<u64>) {
        if !self.timing.measuring {
            return;
        }
        self.timing.phases.record(phase, start, end);
    }

    pub(super) fn timing_outcome(&mut self, outcome: TimingOutcome) {
        if self.timing.measuring && self.timing.outcome != TimingOutcome::Failed {
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
        let instrumented = crate::core::perf::is_enabled();
        let report = self.timing.report || self.timing.requested;
        self.timing.reporting = report;
        self.timing.measuring = report || instrumented;
        if !self.timing.measuring {
            self.clear_timings();
            let result = action(self);
            self.timing.active = false;
            return result;
        }
        self.timing.phases = FrameTimings::new(
            kind,
            self.world.resource::<crate::ecs::MonoClock>().is_some()
                || crate::core::time::is_clock_installed(),
        );
        self.timing.outcome = TimingOutcome::Idle;
        let trace = instrumented
            .then(|| crate::core::perf::operation(self.world.id().diagnostic_id(), kind));
        let measure_cpu =
            self.timing.cpu || (instrumented && crate::core::perf::thread_cpu_time_enabled());
        self.timing.start = report.then(|| self.timing_clock_ns()).flatten();
        if report {
            self.timing.accepted = 0;
            self.timing.accepted_last_ns = self.timing.start;
            self.timing.accepted_last_count = 0;
        }
        let cpu_start = measure_cpu.then(crate::core::perf::cpu::now_ns).flatten();
        let result = action(self);
        let thread_cpu_nanos = cpu_start
            .zip(measure_cpu.then(crate::core::perf::cpu::now_ns).flatten())
            .and_then(|(a, b)| b.checked_sub(a));
        let end = report.then(|| self.timing_clock_ns()).flatten();
        let wall_nanos = self
            .timing
            .start
            .zip(end)
            .and_then(|(a, b)| b.checked_sub(a));
        if report && wall_nanos.is_none() {
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
        self.timing.measuring = false;
        self.timing.reporting = false;
        let trace_id = trace.and_then(|trace| trace.close_operation(outcome, thread_cpu_nanos));
        self.publish_timings(&phases, outcome);
        if report {
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
        self.timing.published = true;
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

    std::thread_local! {
        static CLOCK_READS: core::cell::Cell<u64> = const { core::cell::Cell::new(0) };
    }

    fn counted_clock() -> u64 {
        CLOCK_READS.with(|reads| {
            let next = reads.get() + 1;
            reads.set(next);
            next
        })
    }

    fn record_input<B: Surface, F: RendererFactory<B>>(app: &mut App<B, F>) {
        let start = app.timing_clock_ns();
        app.record_phase(FramePhase::Input, start, app.timing_clock_ns());
        app.timing_outcome(TimingOutcome::Completed);
    }

    #[test]
    fn absent_demand_skips_measurement_resources_and_clock_reads() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let mut app = App::headless(8, 8);
        app.add_resource(crate::ecs::MonoClock::new(counted_clock));
        CLOCK_READS.with(|reads| reads.set(0));
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                for _ in 0..100 {
                    app.with_timing(FrameOperation::Tick, record_input);
                }
            }),
            0
        );
        CLOCK_READS.with(|reads| assert_eq!(reads.get(), 0));
        assert!(app.world.resource::<FrameTimings>().is_none());
        assert!(app.world.resource::<FrameStats>().is_none());
    }

    #[test]
    fn explicit_release_invalidates_results_and_reuses_statistics_storage() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let mut app = App::headless(8, 8);
        app.add_resource(crate::ecs::MonoClock::new(counted_clock));
        app.request_timings(true);
        app.with_timing(FrameOperation::Tick, |app| {
            record_input(app);
            app.with_timing(FrameOperation::Draw, record_input);
        });
        assert_eq!(
            app.world.resource::<FrameTimings>().unwrap().operation(),
            FrameOperation::Tick
        );
        let stats = app.world.resource::<FrameStats>().unwrap() as *const FrameStats;
        assert_eq!(app.world.resource::<FrameStats>().unwrap().len(), 1);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                app.request_timings(false);
                assert!(
                    app.world
                        .resource::<FrameTimings>()
                        .unwrap()
                        .phase_wall_nanos()
                        .is_none()
                );
                assert!(app.world.resource::<FrameStats>().unwrap().is_empty());
                app.request_timings(true);
                app.with_timing(FrameOperation::Tick, record_input);
            }),
            0
        );
        assert_eq!(
            stats,
            app.world.resource::<FrameStats>().unwrap() as *const FrameStats
        );
        assert_eq!(app.world.resource::<FrameStats>().unwrap().len(), 1);
    }

    #[test]
    fn requests_during_operations_apply_at_the_next_outer_boundary() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let mut app = App::headless(8, 8);
        app.add_resource(crate::ecs::MonoClock::new(counted_clock));
        app.with_timing(FrameOperation::Tick, |app| {
            app.request_timings(true);
            app.with_timing(FrameOperation::Draw, record_input);
            assert!(app.timing_clock_ns().is_none());
        });
        assert!(app.world.resource::<FrameTimings>().is_none());
        app.with_timing(FrameOperation::Tick, |app| {
            app.request_timings(false);
            record_input(app);
        });
        assert!(
            app.world
                .resource::<FrameTimings>()
                .unwrap()
                .phase_wall_nanos()
                .is_some()
        );
        app.with_timing(FrameOperation::Draw, record_input);
        assert!(
            app.world
                .resource::<FrameTimings>()
                .unwrap()
                .phase_wall_nanos()
                .is_none()
        );
        assert!(app.world.resource::<FrameStats>().unwrap().is_empty());
    }

    #[test]
    fn explicit_release_preserves_reporter_demand_and_apps_are_independent() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let mut first = App::headless(8, 8);
        let mut second = App::headless(8, 8);
        first.add_resource(crate::ecs::MonoClock::new(counted_clock));
        first.add_plugin(Samples(Rc::new(RefCell::new(Vec::new()))));
        first.enable_timing_reports();
        first.request_timings(false);
        first.with_timing(FrameOperation::Tick, record_input);
        second.with_timing(FrameOperation::Tick, record_input);
        assert!(
            first
                .world
                .resource::<FrameTimings>()
                .unwrap()
                .phase_wall_nanos()
                .is_some()
        );
        assert!(second.world.resource::<FrameTimings>().is_none());
    }

    #[test]
    fn post_render_remains_active_without_fabricating_a_duration() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        struct RenderSamples(Rc<RefCell<Vec<Option<u64>>>>);
        impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for RenderSamples {
            fn build(&mut self, _: &mut App<B, F>) {}
            fn post_render(&mut self, _: &mut World, nanos: Option<u64>) {
                self.0.borrow_mut().push(nanos);
            }
        }
        let mut app = App::headless(8, 8);
        app.with_default_widgets();
        app.spawn_root().id();
        app.add_resource(crate::ecs::MonoClock::new(counted_clock));
        let values = Rc::new(RefCell::new(Vec::new()));
        app.add_plugin(RenderSamples(values.clone()));
        app.render().unwrap();
        app.request_timings(true).render().unwrap();
        app.request_timings(false).render().unwrap();
        let values = values.borrow();
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], None);
        assert!(values[1].is_some());
        assert_eq!(values[2], None);
    }

    struct Samples(Rc<RefCell<Vec<(u64, TimingOutcome)>>>);

    impl<B: Surface, F: RendererFactory<B>> Plugin<B, F> for Samples {
        fn build(&mut self, app: &mut App<B, F>) {
            app.request_timings(true);
        }
        fn on_timing(&mut self, _: &mut World, sample: &TimingSample<'_>) {
            self.0
                .borrow_mut()
                .push((sample.accepted_submissions, sample.outcome));
        }
    }

    #[test]
    fn nested_draws_count_actual_acceptances_even_before_later_failure() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
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
