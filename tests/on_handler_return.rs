#[cfg(test)]
mod tests {
    use mirui::app::App;
    use mirui::ecs::Entity;
    use mirui::input::event::contract::Event;
    use mirui::input::event::gesture::{GestureEvent, Tap};
    use mirui::types::Fixed;
    use mirui::ui;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    static CHILD_FIRES: AtomicU32 = AtomicU32::new(0);
    static PARENT_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        CHILD_FIRES.store(0, Ordering::SeqCst);
        PARENT_FIRES.store(0, Ordering::SeqCst);
    }

    #[mirui::event]
    fn parent_handler() -> bool {
        PARENT_FIRES.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn tap(target: Entity) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
    }

    #[test]
    fn unit_body_consumes_by_default() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let child = app.compose(root, |cx| {
            cx.on_event(root, Tap::EVENT, parent_handler::callback());
            ui! {
                View () on Tap { CHILD_FIRES.fetch_add(1, Ordering::SeqCst); }
            }
        });

        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap(child), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(
            PARENT_FIRES.load(Ordering::SeqCst),
            0,
            "child body returns () → consumed → parent must not fire"
        );
    }

    #[test]
    fn explicit_false_lets_bubble_continue() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let child = app.compose(root, |cx| {
            cx.on_event(root, Tap::EVENT, parent_handler::callback());
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    false
                }
            }
        });

        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap(child), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(
            PARENT_FIRES.load(Ordering::SeqCst),
            1,
            "child body tail `false` propagates; parent handler runs",
        );
    }

    #[test]
    fn explicit_true_consumes() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let child = app.compose(root, |cx| {
            cx.on_event(root, Tap::EVENT, parent_handler::callback());
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    true
                }
            }
        });

        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap(child), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(PARENT_FIRES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn bubble_control_prevent_consumes() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let child = app.compose(root, |cx| {
            cx.on_event(root, Tap::EVENT, parent_handler::callback());
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    ::mirui::input::event::BubbleControl::Prevent
                }
            }
        });

        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap(child), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(
            PARENT_FIRES.load(Ordering::SeqCst),
            0,
            "BubbleControl::Prevent stops bubble"
        );
    }

    #[test]
    fn bubble_control_allow_lets_bubble_continue() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let child = app.compose(root, |cx| {
            cx.on_event(root, Tap::EVENT, parent_handler::callback());
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    ::mirui::input::event::BubbleControl::Allow
                }
            }
        });

        mirui::__macro_support::dispatch_entity_gesture(&mut app.world, &tap(child), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut app.world).unwrap();
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(
            PARENT_FIRES.load(Ordering::SeqCst),
            1,
            "BubbleControl::Allow propagates"
        );
    }
}
