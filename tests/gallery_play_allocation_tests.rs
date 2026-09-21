#![cfg(feature = "gallery")]

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use tracking_allocator::tracked_allocations;

const MAX_WARMED_RENDER_ALLOCATIONS: usize = 40;

#[test]
fn warmed_marble_frame_stays_inside_allocation_budget() {
    let (width, height) = mirui::gallery::demos::marble_play::VIEWPORT;
    let mut app = App::headless(width, height);
    app.with_default_widgets().with_default_systems();
    let root = app.spawn_root().id();
    mirui::gallery::demos::marble_play::setup_app(&mut app, root);
    app.set_root(root);

    for _ in 0..3 {
        app.systems.run_all(&mut app.world);
        app.render_dirty().unwrap();
    }
    let system_allocations = tracked_allocations(|| app.systems.run_all(&mut app.world));
    let render_allocations = tracked_allocations(|| app.render_dirty().unwrap());

    let mut blank = App::headless(width, height);
    blank.with_default_widgets().with_default_systems();
    let blank_root = blank.spawn_root().id();
    blank.set_root(blank_root);
    for _ in 0..3 {
        blank.systems.run_all(&mut blank.world);
        blank.render_dirty().unwrap();
    }
    let blank_systems = tracked_allocations(|| blank.systems.run_all(&mut blank.world));
    let blank_render = tracked_allocations(|| blank.render_dirty().unwrap());

    assert!(blank_systems <= 1);
    assert_eq!(blank_render, 0);
    assert!(system_allocations <= 2);
    assert!(
        render_allocations <= MAX_WARMED_RENDER_ALLOCATIONS,
        "warmed render allocated {render_allocations} times"
    );
}
