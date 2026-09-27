#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::input::event::bubble_dispatch_at;
use mirui::input::event::gesture::GestureEvent;
use mirui::input::event::multi_tap::{MultiTapTracker, current_count};
use mirui::types::Fixed;
use tracking_allocator::tracked_allocations;

#[test]
fn first_tap_reuses_tracker_after_owner_initialization() {
    let mut app = App::headless(8, 8);
    let target = app.world.spawn_empty();

    // Initialize the dispatch owner's graph without recording a tap.
    bubble_dispatch_at(
        &mut app.world,
        &GestureEvent::DragStart {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        },
        50,
    );
    assert!(
        app.world
            .resource::<MultiTapTracker>()
            .unwrap()
            .last
            .is_none()
    );

    let tap = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    };
    assert_eq!(
        tracked_allocations(|| bubble_dispatch_at(&mut app.world, &tap, 100)),
        0
    );
    assert_eq!(current_count(&app.world, target), 1);
}
