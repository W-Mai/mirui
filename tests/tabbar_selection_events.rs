#[cfg(test)]
mod tests {
    use mirui::components::TabBar;
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
    use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

    static LAST_NEW: AtomicU8 = AtomicU8::new(0);
    static LAST_OLD: AtomicU8 = AtomicU8::new(255);
    static FIRES: AtomicU32 = AtomicU32::new(0);
    static USER_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        LAST_NEW.store(0, Ordering::SeqCst);
        LAST_OLD.store(255, Ordering::SeqCst);
        FIRES.store(0, Ordering::SeqCst);
        USER_FIRES.store(0, Ordering::SeqCst);
    }

    fn user_gesture(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
        USER_FIRES.fetch_add(1, Ordering::SeqCst);
        false
    }

    fn fresh_world() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(ViewRegistry::with_builtins());
        let root = WidgetBuilder::new(&mut world).id();
        (world, root)
    }

    fn build_tabbar(world: &mut World, root: Entity) -> Entity {
        let bar = ui! {
            :(
                parent: root
                world: world
            :)

            TabBar (width: 300, height: 40)
                on SelectionChanged {
                    LAST_NEW.store(*new, Ordering::SeqCst);
                    LAST_OLD.store(*old, Ordering::SeqCst);
                    FIRES.fetch_add(1, Ordering::SeqCst);
                }
                {}
        };
        if let Some(tb) = world.get_mut::<TabBar>(bar) {
            tb.count = 3;
        }
        world.insert(
            bar,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(300),
                h: Fixed::from_int(40),
            }),
        );
        bar
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
        let (mut world, root) = fresh_world();
        let bar = build_tabbar(&mut world, root);

        bubble_dispatch_at(&mut world, &tap(bar, 150), 100);
        assert_eq!(FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 1);
        assert_eq!(LAST_OLD.load(Ordering::SeqCst), 0);

        bubble_dispatch_at(&mut world, &tap(bar, 270), 600);
        assert_eq!(FIRES.load(Ordering::SeqCst), 2);
        assert_eq!(LAST_NEW.load(Ordering::SeqCst), 2);
        assert_eq!(LAST_OLD.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn tabbar_same_tab_tap_does_not_emit() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        let bar = build_tabbar(&mut world, root);

        bubble_dispatch_at(&mut world, &tap(bar, 50), 100);
        assert_eq!(FIRES.load(Ordering::SeqCst), 0);
        assert_eq!(world.get::<TabBar>(bar).unwrap().selected, 0);

        bubble_dispatch_at(&mut world, &tap(bar, 80), 600);
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
        let (mut world, root) = fresh_world();
        let bar = build_tabbar(&mut world, root);
        world.insert(bar, GestureHandler::from_fn(user_gesture));

        bubble_dispatch_at(&mut world, &tap(bar, 150), 100);

        assert_eq!(
            FIRES.load(Ordering::SeqCst),
            1,
            "TabBar internal still fires under user GestureHandler",
        );
        assert_eq!(
            USER_FIRES.load(Ordering::SeqCst),
            0,
            "internal returned true so user handler did not run",
        );
        assert_eq!(world.get::<TabBar>(bar).unwrap().selected, 1);
    }
}
