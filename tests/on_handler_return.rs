#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::input::event::gesture::GestureEvent;
    use mirui::input::event::multi_tap::MultiTapTracker;
    use mirui::types::Fixed;
    use mirui::ui;
    use mirui::ui::IdMap;
    use mirui::ui::builder::WidgetBuilder;
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

    fn fresh() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        let root = WidgetBuilder::new(&mut world).id();
        (world, root)
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
        let (mut world, root) = fresh();
        mirui::__macro_support::install_entity_event(
            &mut world,
            root,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            parent_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );

        let child = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap { CHILD_FIRES.fetch_add(1, Ordering::SeqCst); }
            }
        };

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(child), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
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
        let (mut world, root) = fresh();
        mirui::__macro_support::install_entity_event(
            &mut world,
            root,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            parent_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );

        let child = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    false
                }
            }
        };

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(child), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
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
        let (mut world, root) = fresh();
        mirui::__macro_support::install_entity_event(
            &mut world,
            root,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            parent_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );

        let child = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    true
                }
            }
        };

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(child), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(PARENT_FIRES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn bubble_control_prevent_consumes() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh();
        mirui::__macro_support::install_entity_event(
            &mut world,
            root,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            parent_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );

        let child = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    ::mirui::input::event::BubbleControl::Prevent
                }
            }
        };

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(child), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
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
        let (mut world, root) = fresh();
        mirui::__macro_support::install_entity_event(
            &mut world,
            root,
            <mirui::input::event::gesture::Tap as mirui::input::event::contract::Event>::EVENT,
            parent_handler::callback::<
                mirui::input::event::gesture::Tap,
                mirui::__macro_support::NodeContext,
            >(),
        );

        let child = {
            let mut cx = ::mirui::ui::UiScope::new(&mut world, root);
            ui! {
                View () on Tap {
                    CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                    ::mirui::input::event::BubbleControl::Allow
                }
            }
        };

        {
            mirui::__macro_support::dispatch_entity_gesture(&mut world, &tap(child), 100).unwrap();
            mirui::__macro_support::flush_entity_events(&mut world).unwrap();
        };
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(
            PARENT_FIRES.load(Ordering::SeqCst),
            1,
            "BubbleControl::Allow propagates"
        );
    }
}
