#[cfg(test)]
mod tests {
    use mirui::app::App;
    use mirui::ecs::{Entity, World};
    use mirui::input::event::gesture::GestureEvent;
    use mirui::input::event::multi_tap::MultiTapTracker;
    use mirui::types::{Color, Fixed};
    use mirui::ui;
    use mirui::ui::widgets::Button;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static COUNTER: AtomicI64 = AtomicI64::new(0);
    static SUM_X: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    #[mirui::component]
    struct ResetCount;

    #[mirui::event]
    fn reset_tap_count(
        _tap: event!(mirui::input::event::gesture::Tap),
        commands: commands!(),
    ) -> bool {
        commands.remove_resource::<MultiTapTracker>();
        false
    }

    #[mirui::view(component = ResetCount, input = reset_tap_count::callback())]
    fn reset_count_view() {}

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

    fn tap_event(target: Entity) -> GestureEvent {
        GestureEvent::Tap {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        }
    }

    fn dispatch(
        world: &mut World,
        event: &GestureEvent,
        now_ms: u32,
    ) -> mirui::input::event::BubbleControl {
        let decision =
            mirui::__macro_support::dispatch_entity_gesture(world, event, now_ms).unwrap();
        mirui::__macro_support::flush_entity_events(world).unwrap();
        decision
    }

    #[test]
    fn trailing_tap_block_installs_a_callable_listener() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap { fire(); }
            }
        });
        let world = &mut app.world;

        assert_eq!(
            dispatch(world, &tap_event(target), 100),
            mirui::input::event::BubbleControl::Prevent
        );
        assert_eq!(fired(), 1);
    }

    #[test]
    fn inferred_payload_closure_installs_a_callable_listener() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap(|input| {
                    assert_eq!(input.x, Fixed::ZERO);
                    fire();
                })
            }
        });
        let world = &mut app.world;

        assert_eq!(
            dispatch(world, &tap_event(target), 100),
            mirui::input::event::BubbleControl::Prevent
        );
        assert_eq!(fired(), 1);
    }

    #[test]
    fn form_b_body_runs_on_dispatch() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap { fire(); }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);

        assert_eq!(fired(), 1, "on Tap body must fire once per Tap");
    }

    #[test]
    fn form_b_chained_on_distinct_events() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap { fire(); } on LongPress {
                    fire();
                    fire();
                }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);
        assert_eq!(fired(), 1);

        let lp = GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        dispatch(world, &lp, 1000);
        assert_eq!(
            fired(),
            3,
            "Tap fires once + LongPress fires twice = 3 total"
        );
    }

    #[test]
    fn drag_cancel_is_available_to_dsl_handlers() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on DragCancel { fire(); }
            }
        });
        let world = &mut app.world;

        dispatch(
            world,
            &GestureEvent::DragCancel {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
            100,
        );
        assert_eq!(fired(), 1);
    }

    #[test]
    fn form_b_payload_fields_are_in_scope() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap { SUM_X.fetch_add(event.x.to_int() as i64, Ordering::SeqCst); }
            }
        });
        let world = &mut app.world;

        let event = GestureEvent::Tap {
            x: Fixed::from_int(7),
            y: Fixed::ZERO,
            target,
        };
        dispatch(world, &event, 100);
        assert_eq!(SUM_X.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn component_widget_runs_its_typed_tap_listener() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                Button (normal_color: Color::rgb(100, 100, 100)) on Tap { fire(); }
            }
        });
        let world = &mut app.world;

        assert!(world.has::<Button>(target));
        dispatch(world, &tap_event(target), 100);
        assert_eq!(
            fired(),
            1,
            "Component widget on Tap fires the user body once"
        );
    }

    #[test]
    fn form_c_on_tap_count_two_only_triggers_on_double() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap(2) { fire(); }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);
        assert_eq!(fired(), 0, "single Tap with only on Tap(2) must not fire");
        dispatch(world, &tap_event(target), 200);
        assert_eq!(fired(), 1, "second Tap within window fires on Tap(2)");
    }

    #[test]
    fn invalid_bindings_warn_once_without_installing_input_or_blocking_valid_bindings() {
        use mirui::core::log::{self, Level, Sink};
        use mirui::ui::HitTarget;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::sync::Arc;
        use std::sync::atomic::AtomicUsize;

        struct Warnings {
            count: Arc<AtomicUsize>,
            thread: std::thread::ThreadId,
        }

        impl Sink for Warnings {
            fn enabled(&self, level: Level, target: &'static str) -> bool {
                level == Level::Warn && target == "mirui::event::install"
            }

            fn emit(&self, event: &log::Event<'_>) {
                if self.thread == std::thread::current().id() {
                    assert_eq!(event.meta.level, Level::Warn);
                    assert!(event.args.to_string().contains("TapCountError"));
                    self.count.fetch_add(1, Ordering::SeqCst);
                }
            }
        }

        let warnings = Arc::new(AtomicUsize::new(0));
        let mut app = App::headless(128, 128);
        app.with_log(Warnings {
            count: Arc::clone(&warnings),
            thread: std::thread::current().id(),
        });
        for count in [0, 256, u32::MAX] {
            let mut app = App::headless(128, 128);
            let root = app.spawn_root().id();
            let target = app.compose(root, |cx| {
                ui! {
                    View () on Tap(count) { }
                }
            });
            let world = &mut app.world;
            assert!(world.is_alive(target));
            assert!(!world.has::<HitTarget>(target));
            for now in [100, 200, 300] {
                dispatch(world, &tap_event(target), now);
            }
        }
        assert_eq!(warnings.load(Ordering::SeqCst), 3);

        let root = app.spawn_root().id();
        let calls = Rc::new(Cell::new(0));
        let invalid_called = Rc::clone(&calls);
        let plain_called = Rc::clone(&calls);
        let counted_called = Rc::clone(&calls);
        let long_pressed = Rc::clone(&calls);
        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap(0) {
                    invalid_called.set(1000);
                    true
                } on Tap {
                    plain_called.set(plain_called.get() + 1);
                    false
                } on Tap(2) {
                    counted_called.set(counted_called.get() + 10);
                    false
                } on LongPress { long_pressed.set(long_pressed.get() + 100); }
            }
        });
        let world = &mut app.world;
        assert!(world.has::<HitTarget>(target));
        for now in [100, 200, 300] {
            dispatch(world, &tap_event(target), now);
        }
        assert_eq!(calls.get(), 13);
        dispatch(
            world,
            &GestureEvent::LongPress {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target,
            },
            400,
        );
        assert_eq!(calls.get(), 113);
        assert_eq!(warnings.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn tap_selector_evaluates_its_configuration_once_during_installation() {
        use std::cell::Cell;
        use std::rc::Rc;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let configured = Rc::new(Cell::new(2_u32));
        let evaluations = Rc::new(Cell::new(0_u32));
        let calls = Rc::new(Cell::new(0_u32));
        let count = Rc::clone(&configured);
        let evaluated = Rc::clone(&evaluations);
        let called = Rc::clone(&calls);
        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap({
                    evaluated.set(evaluated.get() + 1);
                    count.get()
                }) { called.set(called.get() + 1); }
            }
        });
        let world = &mut app.world;
        assert_eq!(evaluations.get(), 1);
        configured.set(3);
        for now in [100, 200, 300] {
            dispatch(world, &tap_event(target), now);
        }
        assert_eq!(evaluations.get(), 1);
        assert_eq!(
            calls.get(),
            1,
            "installed selector must still match count 2"
        );
    }

    #[test]
    fn form_b_on_tap_count_resets_after_window() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap(2) { fire(); }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);
        dispatch(world, &tap_event(target), 500);
        assert_eq!(fired(), 0, "second Tap past 300ms window resets count");
    }

    #[test]
    fn form_b_on_tap_mixed_single_and_double_routes_by_count() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap {
                    fire();
                    false
                } on Tap(2) {
                    fire();
                    fire();
                }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);
        assert_eq!(fired(), 1, "first Tap fires single");
        dispatch(world, &tap_event(target), 200);
        assert_eq!(
            fired(),
            4,
            "second Tap fires the ordinary arm before the counted arm"
        );
    }

    #[test]
    fn mixed_tap_handlers_preserve_all_declarations_and_order() {
        use std::cell::RefCell;
        use std::rc::Rc;
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let trace = Rc::new(RefCell::new(([0_u8; 8], 0_usize)));
        let record = {
            let trace = trace.clone();
            move |value| {
                let mut trace = trace.borrow_mut();
                let slot = trace.1;
                trace.0[slot] = value;
                trace.1 += 1;
            }
        };
        let first_counted = record.clone();
        let first_plain = record.clone();
        let second_counted = record.clone();
        let second_plain = record.clone();
        let single = record;
        app.compose(root, |cx| {
            ui! {
                View (id: "ordered_taps") on Tap(2) {
                    first_counted(1);
                    false
                } on Tap {
                    first_plain(2);
                    false
                } on Tap(2) {
                    second_counted(3);
                    false
                } on Tap {
                    second_plain(4);
                    false
                } on Tap(1) {
                    single(5);
                    false
                }
            };
        });
        let world = &mut app.world;
        let target = world.find_by_id("ordered_taps").unwrap();
        for (time, expected) in [
            (100, &[2, 4, 5][..]),
            (200, &[1, 2, 3, 4][..]),
            (250, &[2, 4][..]),
        ] {
            trace.borrow_mut().1 = 0;
            dispatch(world, &tap_event(target), time);
            let trace = trace.borrow();
            assert_eq!(&trace.0[..trace.1], expected);
        }
    }

    #[test]
    fn consuming_plain_tap_stops_later_counted_handlers() {
        use std::cell::Cell;
        use std::rc::Rc;
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        let plain = Rc::new(Cell::new(0));
        let exact = Rc::new(Cell::new(0));
        let (plain_body, exact_body) = (plain.clone(), exact.clone());
        app.compose(root, |cx| {
            ui! {
                View (id: "consuming_taps") on Tap { plain_body.set(plain_body.get() + 1); } on Tap(2) { exact_body.set(exact_body.get() + 1); }
            };
        });
        let world = &mut app.world;
        let target = world.find_by_id("consuming_taps").unwrap();
        for time in [100, 200] {
            dispatch(world, &tap_event(target), time);
        }
        assert_eq!(plain.get(), 2);
        assert_eq!(exact.get(), 0);
    }

    #[test]
    fn tap_count_is_frozen_from_the_origin_before_internal_and_parent_callbacks() {
        use mirui::input::event::multi_tap::MultiTapEntry;
        use std::cell::Cell;
        use std::rc::Rc;

        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();
        app.with_widget(reset_count_view::view());
        let seen = Rc::new(Cell::new(0));
        let parent_seen = seen.clone();
        app.compose(root, |cx| {
            ui! {
                View (id: "tap_parent") {
                    View (id: "tap_child") [
                        ResetCount,
                    ] on Tap(2) {
                        assert_eq!(event.x, Fixed::ZERO);
                        false
                    }
                } on Tap(2) { parent_seen.set(parent_seen.get() + 1); }
            };
        });
        let world = &mut app.world;
        let target = world.find_by_id("tap_child").unwrap();
        world.resource_mut::<MultiTapTracker>().unwrap().last = Some(MultiTapEntry {
            entity: target,
            time_ms: 100,
            count: 1,
        });
        dispatch(world, &tap_event(target), 200);
        assert!(world.resource::<MultiTapTracker>().is_none());
        assert_eq!(seen.get(), 1);
    }

    #[test]
    fn form_b_plus_c_mixed_on_same_widget() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let mut app = App::headless(128, 128);
        let root = app.spawn_root().id();

        let target = app.compose(root, |cx| {
            ui! {
                View () on Tap { fire(); } on LongPress {
                    fire();
                    fire();
                }
            }
        });
        let world = &mut app.world;

        dispatch(world, &tap_event(target), 100);
        assert_eq!(fired(), 1, "Form C Tap arm fires");
        let lp = GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target,
        };
        dispatch(world, &lp, 1000);
        assert_eq!(fired(), 3, "Form B LongPress arm fires twice");
    }
}
