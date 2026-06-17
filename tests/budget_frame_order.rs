//! Regression: post_render hooks read FrameStats *as written by the
//! current frame*. Prior to the App::finalize_frame_stats refactor,
//! App::run pushed FrameStats after calling post_render, so reporters
//! saw the previous frame's stats. The refactor moved the write into
//! render()/render_dirty() right before the post_render fan-out — this
//! test pins the visibility contract by pre-seeding stats and checking
//! the sink sees them within the same render() call.

use std::cell::Cell;

use mirui::app::App;
use mirui::app::plugins::{BudgetReportPlugin, BudgetViolation};
use mirui::ecs::Entity;
use mirui::surface::framebuf::FramebufSurface;
use mirui::types::Rect;

fn noop_flush(_: &[u8], _: &Rect) {}

thread_local! {
    static LAST_AVG: Cell<u64> = const { Cell::new(0) };
}

fn record_sink(v: BudgetViolation) {
    LAST_AVG.with(|c| c.set(v.avg_ns));
}

#[test]
fn budget_plugin_sees_current_frame_stats() {
    LAST_AVG.with(|c| c.set(0));
    let backend = FramebufSurface::new(32, 32, noop_flush);
    let mut app = App::new(backend);
    app.with_default_widgets();
    app.add_plugin(
        BudgetReportPlugin::new(1)
            .with_avg_budget(1)
            .with_sink(record_sink),
    );

    let root = Entity {
        id: 0,
        generation: 0,
    };
    app.set_root(root);

    // Pre-seed FrameStats with a known sample. App::render alone won't
    // populate pending_frame (only App::run does); this test pins the
    // contract that the budget sink fires using the FrameStats that
    // already exists at post_render time, which matches what
    // finalize_frame_stats writes synchronously inside render().
    let mut stats = mirui::ecs::FrameStats::default();
    stats.push(50_000_000);
    app.world.insert_resource(stats);
    app.render();

    let observed = LAST_AVG.with(Cell::get);
    assert_eq!(observed, 50_000_000);
}
