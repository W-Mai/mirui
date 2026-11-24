use super::*;
use crate::input::event::gesture::GestureEvent;
use crate::input::event::multi_tap::current_count;
use crate::types::Fixed;

#[test]
fn with_factory_starts_with_an_empty_multi_tap_tracker() {
    let flush: HeadlessFlush = |_buf, _area| {};
    let surface = crate::surface::framebuf::FramebufSurface::new(8, 8, flush);
    let app = App::with_factory(surface, SwRendererFactory::new());

    assert!(
        app.world
            .resource::<MultiTapTracker>()
            .unwrap()
            .last
            .is_none()
    );
}

#[test]
fn app_dispatch_counts_first_and_second_taps() {
    let mut app = App::headless(8, 8);
    let target = app.world.spawn_empty();
    let tap = GestureEvent::Tap {
        x: Fixed::ZERO,
        y: Fixed::ZERO,
        target,
    };

    crate::ui::delivery::dispatch_gesture(&mut app.world, &tap, 100).unwrap();
    assert_eq!(current_count(&app.world, target), 1);
    crate::ui::delivery::dispatch_gesture(&mut app.world, &tap, 200).unwrap();
    assert_eq!(current_count(&app.world, target), 2);
}
