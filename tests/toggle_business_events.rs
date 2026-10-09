#[cfg(test)]
mod tests {
    use mirui::app::App;
    use mirui::ecs::Entity;
    use mirui::input::event::gesture::GestureEvent;
    use mirui::surface::framebuf::FramebufSurface;
    use mirui::types::{Fixed, PhysicalRect, Rect};
    use mirui::ui;
    use mirui::ui::ComputedRect;
    use mirui::ui::widgets::{Checkbox, ProgressBar, Switch};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};

    static SWITCH_LAST_NOW: AtomicBool = AtomicBool::new(false);
    static SWITCH_FIRES: AtomicU32 = AtomicU32::new(0);
    static CHECKBOX_LAST_NOW: AtomicBool = AtomicBool::new(false);
    static CHECKBOX_FIRES: AtomicU32 = AtomicU32::new(0);
    static PROGRESS_LAST_NEW: AtomicI64 = AtomicI64::new(0);
    static PROGRESS_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    type HeadlessApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

    fn reset() {
        SWITCH_LAST_NOW.store(false, Ordering::SeqCst);
        SWITCH_FIRES.store(0, Ordering::SeqCst);
        CHECKBOX_LAST_NOW.store(false, Ordering::SeqCst);
        CHECKBOX_FIRES.store(0, Ordering::SeqCst);
        PROGRESS_LAST_NEW.store(0, Ordering::SeqCst);
        PROGRESS_FIRES.store(0, Ordering::SeqCst);
    }

    fn fresh_app() -> (HeadlessApp, Entity) {
        let mut app = App::headless(128, 128);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        (app, root)
    }

    fn tap(target: Entity, x: i32) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::ZERO,
            target,
        }
    }

    #[test]
    fn switch_on_toggled_emits_with_new_state() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut app, root) = fresh_app();
        let switch = app.compose(root, |cx| {
            ui! {
                Switch (width: 60, height: 32) on Toggled {
                    SWITCH_LAST_NOW.store(event.now, Ordering::SeqCst);
                    SWITCH_FIRES.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        let mut world = app.world;
        world.insert(
            switch,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(60),
                h: Fixed::from_int(32),
            }),
        );
        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(switch, 30), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(SWITCH_FIRES.load(Ordering::SeqCst), 1);
        assert!(SWITCH_LAST_NOW.load(Ordering::SeqCst));

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(switch, 30), 600)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(SWITCH_FIRES.load(Ordering::SeqCst), 2);
        assert!(!SWITCH_LAST_NOW.load(Ordering::SeqCst));
    }

    #[test]
    fn checkbox_on_toggled_emits_with_new_state() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut app, root) = fresh_app();
        let checkbox = app.compose(root, |cx| {
            ui! {
                Checkbox (width: 32, height: 32) on Toggled {
                    CHECKBOX_LAST_NOW.store(event.now, Ordering::SeqCst);
                    CHECKBOX_FIRES.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        let mut world = app.world;
        world.insert(
            checkbox,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(32),
                h: Fixed::from_int(32),
            }),
        );
        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(checkbox, 16), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(CHECKBOX_FIRES.load(Ordering::SeqCst), 1);
        assert!(CHECKBOX_LAST_NOW.load(Ordering::SeqCst));
    }

    #[test]
    fn progress_bar_on_value_changed_emits_with_new_old() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut app, root) = fresh_app();
        let pb = app.compose(root, |cx| {
            ui! {
                ProgressBar (width: 100, height: 16) on ValueChanged {
                    let _ = event.old;
                    PROGRESS_LAST_NEW.store((event.new * 100.0) as i64, Ordering::SeqCst);
                    PROGRESS_FIRES.fetch_add(1, Ordering::SeqCst);
                }
            }
        });
        let mut world = app.world;
        world.insert(
            pb,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(16),
            }),
        );
        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(pb, 50), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(PROGRESS_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(PROGRESS_LAST_NEW.load(Ordering::SeqCst), 50);
    }
}
