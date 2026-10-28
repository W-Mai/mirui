//! Timing reporters observe the current completed operation after business post-render hooks.
use mirui::app::plugin::Plugin;
use mirui::app::plugins::{BudgetReportPlugin, BudgetViolation};
use mirui::app::{App, SwRendererFactory};
use mirui::ecs::{FrameOperation, FramePhase, MonoClock, TimingSample};
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::PhysicalRect;
use std::cell::Cell;

type Backend = FramebufSurface<fn(&[u8], PhysicalRect)>;
thread_local! {
    static CLOCK: Cell<u64> = const { Cell::new(0) };
    static BUDGET: Cell<Option<(FrameOperation, u64)>> = const { Cell::new(None) };
    static POST_RENDER: Cell<bool> = const { Cell::new(false) };
    static OBSERVED: Cell<bool> = const { Cell::new(false) };
}
fn clock() -> u64 {
    CLOCK.with(|value| {
        let now = value.get() + 100;
        value.set(now);
        now
    })
}
fn sink(report: BudgetViolation) {
    BUDGET.with(|value| value.set(Some((report.operation, report.avg_ns))));
}
struct Observe;
impl Plugin<Backend, SwRendererFactory> for Observe {
    fn build(&mut self, _: &mut App<Backend>) {}
    fn post_render(&mut self, _: &mut mirui::ecs::World, _: Option<u64>) {
        POST_RENDER.with(|value| value.set(true));
    }
    fn on_timing(&mut self, world: &mut mirui::ecs::World, sample: &TimingSample<'_>) {
        assert!(POST_RENDER.with(Cell::get));
        assert_eq!(sample.operation, FrameOperation::Draw);
        assert!(!sample.phases.has_phase(FramePhase::Input));
        assert_eq!(
            world
                .resource::<mirui::ecs::FrameTimings>()
                .unwrap()
                .operation(),
            FrameOperation::Draw
        );
        OBSERVED.with(|value| value.set(true));
    }
}
#[test]
fn budget_uses_current_closed_draw_after_business_hook() {
    CLOCK.with(|value| value.set(0));
    BUDGET.with(|value| value.set(None));
    POST_RENDER.with(|value| value.set(false));
    OBSERVED.with(|value| value.set(false));
    let mut app = App::headless(32, 32);
    app.add_resource(MonoClock::new(clock));
    app.with_default_widgets();
    app.spawn_root().id();
    app.add_plugin(
        BudgetReportPlugin::new(1)
            .with_avg_budget(1)
            .with_sink(sink),
    );
    app.add_plugin(Observe);
    app.render().unwrap();
    assert!(OBSERVED.with(Cell::get));
    let (kind, cost) = BUDGET.with(Cell::get).unwrap();
    assert_eq!(kind, FrameOperation::Draw);
    assert!(cost > 0);
}
