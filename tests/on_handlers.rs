#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::input::event::GestureHandler;
    use mirui::input::event::bubble_dispatch_at;
    use mirui::input::event::gesture::GestureEvent;
    use mirui::input::event::multi_tap::MultiTapTracker;
    use mirui::types::{Color, Fixed};
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::builder::WidgetBuilder;
    use mirui::ui::widgets::Button;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static COUNTER: AtomicI64 = AtomicI64::new(0);
    static SUM_X: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        COUNTER.store(0, Ordering::SeqCst);
        SUM_X.store(0, Ordering::SeqCst);
    }

    fn fire() {
        COUNTER.fetch_add(1, Ordering::SeqCst);
    }

    fn fired() -> i64 {
        COUNTER.load(Ordering::SeqCst)
    }

    fn fresh_world() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        let root = WidgetBuilder::new(&mut world).id();
        (world, root)
    }

    fn tap_event(target: Entity) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
    }

    #[test]
    fn form_b_modifier_chain_attaches_to_widget() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {} on Tap { fire(); }
        };

        let layouts: Vec<_> = world.query::<GestureHandler>().collect();
        assert!(
            !layouts.is_empty(),
            "Form B: trailing on Tap must attach a GestureHandler to the widget"
        );
    }

    #[test]
    fn form_c_inline_on_attaches_to_widget() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap { fire(); } {}
        };

        let layouts: Vec<_> = world.query::<GestureHandler>().collect();
        assert!(
            !layouts.is_empty(),
            "Form C: inline on Tap between attrs and body attaches a GestureHandler"
        );
    }

    #[test]
    fn form_b_body_runs_on_dispatch() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {} on Tap { fire(); }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().expect("one GestureHandler attached");
        bubble_dispatch_at(&mut world, &tap_event(target), 100);

        assert_eq!(fired(), 1, "on Tap body must fire once per Tap");
    }

    #[test]
    fn form_b_chained_on_distinct_events() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {}
                on Tap { fire(); }
                on LongPress { fire(); fire(); }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 1);

        let lp = GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        bubble_dispatch_at(&mut world, &lp, 1000);
        assert_eq!(
            fired(),
            3,
            "Tap fires once + LongPress fires twice = 3 total"
        );
    }

    #[test]
    fn form_b_destructured_x_y_in_scope() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {}
                on Tap {
                    SUM_X.fetch_add(x.to_int() as i64, Ordering::SeqCst);
                }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        let event = GestureEvent::Tap {
            x: Fixed::from_int(7),
            y: Fixed::ZERO,
            target,
        };
        bubble_dispatch_at(&mut world, &event, 100);
        assert_eq!(SUM_X.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn form_b_on_component_widget_overrides_internal() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            Button (normal_color: Color::rgb(100, 100, 100)) {}
                on Tap { fire(); }
        };

        let buttons: Vec<Entity> = world.query::<Button>().collect();
        let target = buttons[0];
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(
            fired(),
            1,
            "Component widget on Tap fires user body once \
             (v0.27.1: user dispatch overrides internal handler)"
        );
    }

    #[test]
    fn form_c_on_tap_count_two_only_triggers_on_double() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap(2) { fire(); } {}
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 0, "single Tap with only on Tap(2) must not fire");
        bubble_dispatch_at(&mut world, &tap_event(target), 200);
        assert_eq!(fired(), 1, "second Tap within window fires on Tap(2)");
    }

    #[test]
    fn form_b_on_tap_count_resets_after_window() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {} on Tap(2) { fire(); }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        bubble_dispatch_at(&mut world, &tap_event(target), 500);
        assert_eq!(fired(), 0, "second Tap past 300ms window resets count");
    }

    #[test]
    fn form_b_on_tap_mixed_single_and_double_routes_by_count() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {}
                on Tap { fire(); }
                on Tap(2) { fire(); fire(); }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 1, "first Tap fires single");
        bubble_dispatch_at(&mut world, &tap_event(target), 200);
        assert_eq!(
            fired(),
            3,
            "second Tap within window fires double (2 fires)"
        );
    }

    #[test]
    fn form_b_plus_c_mixed_on_same_widget() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View ()
                on Tap { fire(); }
                {}
                on LongPress { fire(); fire(); }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 1, "Form C Tap arm fires");
        let lp = GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        bubble_dispatch_at(&mut world, &lp, 1000);
        assert_eq!(fired(), 3, "Form B LongPress arm fires twice");
    }
}
