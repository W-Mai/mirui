#[cfg(test)]
mod tests {
    use mirui::app::App;
    use mirui::ecs::Entity;
    use mirui::input::event::contract::Event;
    use mirui::input::event::gesture::{GestureEvent, Tap};
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::types::{Fixed, PhysicalRect, Rect};
    use mirui::ui;
    use mirui::ui::ComputedRect;
    use mirui::ui::widgets::TabBar;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

    static LAST_NEW: AtomicU8 = AtomicU8::new(0);
    static LAST_OLD: AtomicU8 = AtomicU8::new(255);
    static FIRES: AtomicU32 = AtomicU32::new(0);
    static USER_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    type HeadlessApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

    fn reset() {
        LAST_NEW.store(0, Ordering::SeqCst);
        LAST_OLD.store(255, Ordering::SeqCst);
        FIRES.store(0, Ordering::SeqCst);
        USER_FIRES.store(0, Ordering::SeqCst);
    }

    #[mirui::event]
    fn user_gesture() -> bool {
        USER_FIRES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn tabbar_app() -> (HeadlessApp, Entity) {
        let mut app = App::headless(128, 128);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let bar = app.compose(root, |cx| {
            ui! {
                TabBar (width: 300, height: 40, count: 3) on SelectionChanged {
                    LAST_NEW.store(event.new, Ordering::SeqCst);
                    LAST_OLD.store(event.old, Ordering::SeqCst);
                    FIRES.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        app.world.insert(
            bar,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(300),
                h: Fixed::from_int(40),
            }),
        );
        (app, bar)
    }

    fn tap(target: Entity, x: i32) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::ZERO,
            target,
        }
    }

    #[test]
    fn tabbar_tap_emits_selection_changed_with_new_old() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (app, bar) = tabbar_app();
        let mut world = app.world;

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(bar, 150), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(LAST_OLD.load(Ordering::SeqCst), 0);

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(bar, 270), 600)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(FIRES.load(Ordering::SeqCst), 2);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 2);
        assert_eq!(LAST_OLD.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn tabbar_same_tab_tap_does_not_emit() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (app, bar) = tabbar_app();
        let mut world = app.world;

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(bar, 50), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(FIRES.load(Ordering::SeqCst), 0);
        assert_eq!(world.get::<TabBar>(bar).unwrap().selected, 0);

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(bar, 80), 600)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(
            FIRES.load(Ordering::SeqCst),
            0,
            "same-tab re-tap must not emit SelectionChanged"
        );
    }

    #[test]
    fn user_gesture_handler_does_not_replace_tabbar_internal() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut app, bar) = tabbar_app();
        app.compose(bar, |tree| {
            tree.on_event(bar, Tap::EVENT, user_gesture::callback());
        });
        let mut world = app.world;

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(bar, 150), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };

        assert_eq!(
            FIRES.load(Ordering::SeqCst),
            1,
            "TabBar internal still fires under author gesture binding",
        );
        assert_eq!(
            USER_FIRES.load(Ordering::SeqCst),
            0,
            "internal returned true so user handler did not run",
        );
        assert_eq!(world.get::<TabBar>(bar).unwrap().selected, 1);
    }
}
