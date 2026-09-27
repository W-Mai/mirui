use mirui::app::App;
use mirui::ui::dirty::Dirty;
use mirui::ui::render_system::LastDirtyRegions;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;
use tracking_allocator::tracked_allocations;

#[test]
fn first_dirty_render_reuses_prepared_plan_and_last_regions() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();

    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert!(last.0.rects.capacity() >= 4);
    let last_ptr = last as *const LastDirtyRegions;
    let rects_ptr = last.0.rects.as_ptr();

    app.render().unwrap();
    app.world.insert(root, Dirty);
    assert_eq!(tracked_allocations(|| app.render_dirty().unwrap()), 0);

    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert_eq!(last as *const LastDirtyRegions, last_ptr);
    assert_eq!(last.0.rects.as_ptr(), rects_ptr);
    assert_eq!(last.0.rects.len(), 1);
}

#[test]
fn direct_first_dirty_frame_and_root_replacement_keep_prepared_storage() {
    let mut app = App::headless(32, 32);
    app.with_default_widgets();
    let root = app.spawn_root().id();
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    let last_ptr = last as *const LastDirtyRegions;
    let rects_ptr = last.0.rects.as_ptr();

    app.world.insert(root, Dirty);
    app.render_dirty().unwrap();
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert_eq!(last as *const LastDirtyRegions, last_ptr);
    assert_eq!(last.0.rects.as_ptr(), rects_ptr);

    let replacement = app.spawn_root().id();
    app.render().unwrap();
    app.world.insert(replacement, Dirty);
    assert_eq!(tracked_allocations(|| app.render_dirty().unwrap()), 0);
    let last = app.world.resource::<LastDirtyRegions>().unwrap();
    assert_eq!(last as *const LastDirtyRegions, last_ptr);
    assert_eq!(last.0.rects.as_ptr(), rects_ptr);
}
