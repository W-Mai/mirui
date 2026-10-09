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
    use mirui::ui::widgets::Slider;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static EVENT_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    static LAST_NEW: AtomicI64 = AtomicI64::new(0);
    static USER_GESTURE_FIRES: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    type HeadlessApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

    fn reset() {
        EVENT_LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
        LAST_NEW.store(0, Ordering::SeqCst);
        USER_GESTURE_FIRES.store(0, Ordering::SeqCst);
    }

    fn log_event(name: &'static str) {
        EVENT_LOG
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(name);
    }

    #[mirui::event]
    fn user_gesture_handler() -> bool {
        USER_GESTURE_FIRES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn slider_app() -> (HeadlessApp, Entity) {
        let mut app = App::headless(128, 128);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let slider = app.compose(root, |cx| {
            ui! {
                Slider (width: 100, height: 20, min: 0, max: 100, value: 0) on ValueChanged {
                    let _ = event.old;
                    log_event("ValueChanged");
                    LAST_NEW.store(event.new.to_int() as i64, std::sync::atomic::Ordering::SeqCst);
                } on DragStarted { log_event("DragStarted"); } on DragEnded { log_event("DragEnded"); }
            }
        });
        app.world.insert(
            slider,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );
        (app, slider)
    }

    fn drain_log() -> Vec<&'static str> {
        EVENT_LOG.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn tap(target: Entity, x: i32) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::from_int(x),
            y: Fixed::ZERO,
            target,
        }
    }

    #[test]
    fn ui_macro_registers_value_changed_notifications() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (app, slider) = slider_app();
        let mut world = app.world;
        mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(slider, 75), 100).unwrap();
        mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        assert_eq!(drain_log(), &["ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 75);
    }

    #[test]
    fn slider_tap_emits_value_changed_through_ui_macro() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (app, slider) = slider_app();
        let mut world = app.world;
        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(slider, 75), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(drain_log(), &["ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 75);
    }

    #[test]
    fn slider_drag_lifecycle_emits_in_order() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (app, slider) = slider_app();
        let mut world = app.world;
        {
            mirui::__macro_support::dispatch_entity_gesture(
                &mut world,
                &GestureEvent::DragStart {
                    x: Fixed::from_int(40),
                    y: Fixed::ZERO,
                    target: slider,
                },
                100,
            )
            .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        {
            mirui::__macro_support::dispatch_entity_gesture(
                &mut world,
                &GestureEvent::DragMove {
                    x: Fixed::from_int(60),
                    y: Fixed::ZERO,
                    dx: Fixed::ZERO,
                    dy: Fixed::ZERO,
                    target: slider,
                },
                120,
            )
            .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        {
            mirui::__macro_support::dispatch_entity_gesture(
                &mut world,
                &GestureEvent::DragEnd {
                    x: Fixed::from_int(60),
                    y: Fixed::ZERO,
                    vx: Fixed::ZERO,
                    vy: Fixed::ZERO,
                    target: slider,
                },
                140,
            )
            .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(
            drain_log(),
            &["DragStarted", "ValueChanged", "DragEnded"],
            "drag lifecycle order: start → value change → end",
        );
    }

    #[test]
    fn user_gesture_handler_does_not_replace_slider_internal() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut app, slider) = slider_app();
        app.compose(slider, |tree| {
            tree.on_event(slider, Tap::EVENT, user_gesture_handler::callback());
        });
        let mut world = app.world;

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(slider, 50), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };

        assert_eq!(
            drain_log(),
            &["ValueChanged"],
            "slider internal channel still fires when author gesture binding is attached",
        );
        assert_eq!(
            USER_GESTURE_FIRES.load(Ordering::SeqCst),
            0,
            "internal channel consumed the event so user handler did not run",
        );

        let value = world.get::<Slider>(slider).map(|s| s.value).unwrap();
        assert_eq!(
            value,
            Fixed::from_int(50),
            "slider value moved as a result of the internal handler",
        );
    }

    #[test]
    fn qualified_on_slider_value_changed_compiles() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        app.with_default_widgets();
        let root = app.spawn_root().id();
        let slider = app.compose(root, |cx| {
            ui! {
                Slider (width: 100, height: 20, min: 0, max: 100, value: 0) on Slider::ValueChanged {
                    let _ = event.old;
                    LAST_NEW.store(event.new.to_int() as i64, std::sync::atomic::Ordering::SeqCst);
                    log_event("Q-ValueChanged");
                }
            }
        });
        let mut world = app.world;
        world.insert(
            slider,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );
        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(slider, 50), 100)
                .unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(drain_log(), &["Q-ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 50);
    }
}
