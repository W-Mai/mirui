#[cfg(test)]
mod tests {
    use mirui::components::Slider;
    use mirui::components::slider::SliderHandler;
    use mirui::ecs::{Entity, World};
    use mirui::event::GestureHandler;
    use mirui::event::bubble_dispatch_at;
    use mirui::event::gesture::GestureEvent;
    use mirui::event::multi_tap::MultiTapTracker;
    use mirui::types::{Fixed, Rect};
    use mirui::ui;
    use mirui::widget::builder::WidgetBuilder;
    use mirui::widget::view::ViewRegistry;
    use mirui::widget::{ComputedRect, IdMap};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static EVENT_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    static LAST_NEW: AtomicI64 = AtomicI64::new(0);
    static USER_GESTURE_FIRES: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

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

    fn user_gesture_handler(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
        USER_GESTURE_FIRES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn fresh_world_with_slider() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(ViewRegistry::with_builtins());
        let root = WidgetBuilder::new(&mut world).id();
        let slider = ui! {
            :(
                parent: root
                world: &mut world
            :)

            Slider (width: 100, height: 20)
                on ValueChanged {
                    let _ = old;
                    log_event("ValueChanged");
                    LAST_NEW.store(new.to_int() as i64, std::sync::atomic::Ordering::SeqCst);
                }
                on DragStarted { log_event("DragStarted"); }
                on DragEnded { log_event("DragEnded"); }
                {}
        };
        world.insert(
            slider,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );
        if let Some(s) = world.get_mut::<Slider>(slider) {
            s.min = Fixed::ZERO;
            s.max = Fixed::from_int(100);
            s.value = Fixed::ZERO;
        }
        (world, slider)
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
    fn ui_macro_attaches_slider_handler_for_value_changed() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (world, slider) = fresh_world_with_slider();
        assert!(
            world.get::<SliderHandler>(slider).is_some(),
            "ui! `on ValueChanged` must attach a SliderHandler",
        );
    }

    #[test]
    fn slider_tap_emits_value_changed_through_ui_macro() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, slider) = fresh_world_with_slider();
        bubble_dispatch_at(&mut world, &tap(slider, 75), 100);
        assert_eq!(drain_log(), &["ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 75);
    }

    #[test]
    fn slider_drag_lifecycle_emits_in_order() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, slider) = fresh_world_with_slider();
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::DragStart {
                x: Fixed::from_int(40),
                y: Fixed::ZERO,
                target: slider,
            },
            100,
        );
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::DragMove {
                x: Fixed::from_int(60),
                y: Fixed::ZERO,
                dx: Fixed::ZERO,
                dy: Fixed::ZERO,
                target: slider,
            },
            120,
        );
        bubble_dispatch_at(
            &mut world,
            &GestureEvent::DragEnd {
                x: Fixed::from_int(60),
                y: Fixed::ZERO,
                vx: Fixed::ZERO,
                vy: Fixed::ZERO,
                target: slider,
            },
            140,
        );
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
        let (mut world, slider) = fresh_world_with_slider();
        world.insert(slider, GestureHandler::from_fn(user_gesture_handler));

        bubble_dispatch_at(&mut world, &tap(slider, 50), 100);

        assert_eq!(
            drain_log(),
            &["ValueChanged"],
            "slider internal channel still fires when user GestureHandler is attached",
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
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(ViewRegistry::with_builtins());
        let root = WidgetBuilder::new(&mut world).id();
        let slider = ui! {
            :(
                parent: root
                world: &mut world
            :)

            Slider (width: 100, height: 20)
                on Slider::ValueChanged {
                    let _ = old;
                    LAST_NEW.store(new.to_int() as i64, std::sync::atomic::Ordering::SeqCst);
                    log_event("Q-ValueChanged");
                }
                {}
        };
        world.insert(
            slider,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(20),
            }),
        );
        if let Some(s) = world.get_mut::<Slider>(slider) {
            s.min = Fixed::ZERO;
            s.max = Fixed::from_int(100);
            s.value = Fixed::ZERO;
        }
        bubble_dispatch_at(&mut world, &tap(slider, 50), 100);
        assert_eq!(drain_log(), &["Q-ValueChanged"]);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 50);
    }
}
