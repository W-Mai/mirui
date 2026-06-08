#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::event::GestureHandler;
    use mirui::event::bubble_dispatch_at;
    use mirui::event::gesture::GestureEvent;
    use mirui::event::multi_tap::MultiTapTracker;
    use mirui::types::Fixed;
    use mirui::ui;
    use mirui::widget::IdMap;
    use mirui::widget::builder::WidgetBuilder;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    static CHILD_FIRES: AtomicU32 = AtomicU32::new(0);
    static PARENT_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        CHILD_FIRES.store(0, Ordering::SeqCst);
        PARENT_FIRES.store(0, Ordering::SeqCst);
    }

    fn parent_handler(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
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
        world.insert(
            root,
            GestureHandler {
                on_gesture: parent_handler,
            },
        );

        let child = ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap {
                CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
            }
        };

        bubble_dispatch_at(&mut world, &tap(child), 100);
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
        world.insert(
            root,
            GestureHandler {
                on_gesture: parent_handler,
            },
        );

        let child = ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap {
                CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                false
            }
        };

        bubble_dispatch_at(&mut world, &tap(child), 100);
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
        world.insert(
            root,
            GestureHandler {
                on_gesture: parent_handler,
            },
        );

        let child = ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap {
                CHILD_FIRES.fetch_add(1, Ordering::SeqCst);
                true
            }
        };

        bubble_dispatch_at(&mut world, &tap(child), 100);
        assert_eq!(CHILD_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(PARENT_FIRES.load(Ordering::SeqCst), 0);
    }
}
