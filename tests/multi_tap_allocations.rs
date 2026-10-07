#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::input::event::gesture::GestureEvent;
use mirui::input::event::multi_tap::{MultiTapTracker, current_count};
use mirui::types::Fixed;
use tracking_allocator::tracked_allocations;

#[test]
fn installed_tap_options_dispatch_without_allocating_or_reevaluating_arguments() {
    use std::cell::Cell;
    use std::rc::Rc;

    let mut app = App::headless(8, 8);
    let root = app.world.spawn_empty();
    let calls = Rc::new(Cell::new(0_u32));
    let evaluations = Rc::new(Cell::new(0_u32));
    let called = Rc::clone(&calls);
    let plain_calls = Rc::new(Cell::new(0_u32));
    let plain_called = plain_calls.clone();
    let evaluated = Rc::clone(&evaluations);
    let target = {
        let mut cx = mirui::ui::UiScope::new(&mut app.world, root);
        mirui::ui! {
            View () on Tap {
                plain_called.set(plain_called.get() + 1);
                false
            } on Tap({
                evaluated.set(evaluated.get() + 1);
                2
            }) { called.set(called.get() + 1); }
        }
    };
    assert_eq!(evaluations.get(), 1);
    let tap = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    };
    for now in [100, 200, 300, 700, 800] {
        assert_eq!(
            tracked_allocations(|| {
                mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap, now).unwrap();
                mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
            }),
            0
        );
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(plain_calls.get(), 5);
    assert_eq!(evaluations.get(), 1);
}

#[test]
fn first_tap_reuses_tracker_after_owner_initialization() {
    let mut app = App::headless(8, 8);
    let target = app.world.spawn_empty();

    app.render().unwrap();
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
        tracked_allocations(|| {
            mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap, 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        }),
        0
    );
    assert_eq!(current_count(&app.world, target), 1);
}
