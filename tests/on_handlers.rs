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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { fire(); }
            }
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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { fire(); }
            }
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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { fire(); }
            }
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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { fire(); } on LongPress {
                    fire();
                    fire();
                }
            }
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
    fn drag_cancel_is_available_to_dsl_handlers() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on DragCancel { fire(); }
            }
        };

        let target = world
            .query::<GestureHandler>()
            .collect()
            .into_iter()
            .next()
            .expect("DragCancel handler");
        bubble_dispatch_at(
            &mut world,
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
    fn form_b_destructured_x_y_in_scope() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { SUM_X.fetch_add(x.to_int() as i64, Ordering::SeqCst); }
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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                Button (normal_color: Color::rgb(100, 100, 100)) on Tap { fire(); }
            }
        };

        let buttons: Vec<Entity> = world.query::<Button>().collect();
        let target = buttons[0];
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
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
        let (mut world, root) = fresh_world();

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap(2) { fire(); }
            }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 0, "single Tap with only on Tap(2) must not fire");
        bubble_dispatch_at(&mut world, &tap_event(target), 200);
        assert_eq!(fired(), 1, "second Tap within window fires on Tap(2)");
    }

    #[test]
    fn tap_selector_rejects_invalid_counts_before_installing_a_handler() {
        for count in [0, 256, u32::MAX] {
            let (mut world, root) = fresh_world();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
                ui! {
                    View () on Tap(count) { }
                };
            }));
            assert!(result.is_err(), "invalid count {count} was accepted");
            assert!(world.query::<GestureHandler>().iter().next().is_none());
        }
    }

    #[test]
    fn tap_selector_evaluates_its_configuration_once_during_installation() {
        use std::cell::Cell;
        use std::rc::Rc;

        let (mut world, root) = fresh_world();
        let configured = Rc::new(Cell::new(2_u32));
        let evaluations = Rc::new(Cell::new(0_u32));
        let calls = Rc::new(Cell::new(0_u32));
        let count = Rc::clone(&configured);
        let evaluated = Rc::clone(&evaluations);
        let called = Rc::clone(&calls);
        let target = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap({
                    evaluated.set(evaluated.get() + 1);
                    count.get()
                }) { called.set(called.get() + 1); }
            }
        };
        assert_eq!(evaluations.get(), 1);
        configured.set(3);
        for now in [100, 200, 300] {
            bubble_dispatch_at(&mut world, &tap_event(target), now);
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
        let (mut world, root) = fresh_world();

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap(2) { fire(); }
            }
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

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap {
                    fire();
                    false
                } on Tap(2) {
                    fire();
                    fire();
                }
            }
        };

        let mut handlers: Vec<Entity> = world.query::<GestureHandler>().collect();
        let target = handlers.pop().unwrap();
        bubble_dispatch_at(&mut world, &tap_event(target), 100);
        assert_eq!(fired(), 1, "first Tap fires single");
        bubble_dispatch_at(&mut world, &tap_event(target), 200);
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
        let (mut world, root) = fresh_world();
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
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View (id: "ordered_taps") on Tap(2) {
                    record(1);
                    false
                } on Tap {
                    record(2);
                    false
                } on Tap(2) {
                    record(3);
                    false
                } on Tap {
                    record(4);
                    false
                } on Tap(1) {
                    record(5);
                    false
                }
            };
        }
        let target = world.find_by_id("ordered_taps").unwrap();
        for (time, expected) in [
            (100, &[2, 4, 5][..]),
            (200, &[1, 2, 3, 4][..]),
            (250, &[2, 4][..]),
        ] {
            trace.borrow_mut().1 = 0;
            bubble_dispatch_at(&mut world, &tap_event(target), time);
            let trace = trace.borrow();
            assert_eq!(&trace.0[..trace.1], expected);
        }
    }

    #[test]
    fn consuming_plain_tap_stops_later_counted_handlers() {
        use std::cell::Cell;
        use std::rc::Rc;
        let (mut world, root) = fresh_world();
        let plain = Rc::new(Cell::new(0));
        let exact = Rc::new(Cell::new(0));
        let (plain_body, exact_body) = (plain.clone(), exact.clone());
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View (id: "consuming_taps") on Tap { plain_body.set(plain_body.get() + 1); } on Tap(2) { exact_body.set(exact_body.get() + 1); }
            };
        }
        let target = world.find_by_id("consuming_taps").unwrap();
        for time in [100, 200] {
            bubble_dispatch_at(&mut world, &tap_event(target), time);
        }
        assert_eq!(plain.get(), 2);
        assert_eq!(exact.get(), 0);
    }

    #[test]
    fn tap_count_is_frozen_from_the_origin_before_internal_and_parent_callbacks() {
        use mirui::input::event::multi_tap::MultiTapEntry;
        use mirui::ui::view::View;
        use std::cell::Cell;
        use std::rc::Rc;

        #[mirui::component]
        struct ResetCount;
        fn reset(world: &mut World, _: Entity, _: &GestureEvent) -> bool {
            world.remove_resource::<MultiTapTracker>();
            false
        }
        let (mut world, root) = fresh_world();
        let mut registry = mirui::ui::view::ViewRegistry::default();
        registry.insert(
            View::systems_only("ResetCount", &[])
                .with_filter::<ResetCount>()
                .with_internal_gesture(reset),
        );
        world.insert_resource(registry);
        let seen = Rc::new(Cell::new(0));
        let parent_seen = seen.clone();
        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View (id: "tap_parent") on Tap(2) { parent_seen.set(parent_seen.get() + 1); }
                {
                    View (id: "tap_child") [
                        ResetCount,
                    ] on Tap(2) {
                        assert_eq!(x, Fixed::ZERO);
                        false
                    }
                }
            };
        }
        let target = world.find_by_id("tap_child").unwrap();
        world.resource_mut::<MultiTapTracker>().unwrap().last = Some(MultiTapEntry {
            entity: target,
            time_ms: 100,
            count: 1,
        });
        bubble_dispatch_at(&mut world, &tap_event(target), 200);
        assert!(world.resource::<MultiTapTracker>().is_none());
        assert_eq!(seen.get(), 1);
    }

    #[test]
    fn form_b_plus_c_mixed_on_same_widget() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();

        {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { fire(); } on LongPress {
                    fire();
                    fire();
                }
            }
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
