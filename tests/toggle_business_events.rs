#[cfg(test)]
mod tests {
    use mirui::components::{Checkbox, ProgressBar, Switch};
    use mirui::ecs::{Entity, World};
    use mirui::event::bubble_dispatch_at;
    use mirui::event::gesture::GestureEvent;
    use mirui::event::multi_tap::MultiTapTracker;
    use mirui::types::{Fixed, Rect};
    use mirui::ui;
    use mirui::widget::builder::WidgetBuilder;
    use mirui::widget::view::ViewRegistry;
    use mirui::widget::{ComputedRect, IdMap};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering};

    static SWITCH_LAST_NOW: AtomicBool = AtomicBool::new(false);
    static SWITCH_FIRES: AtomicU32 = AtomicU32::new(0);
    static CHECKBOX_LAST_NOW: AtomicBool = AtomicBool::new(false);
    static CHECKBOX_FIRES: AtomicU32 = AtomicU32::new(0);
    static PROGRESS_LAST_NEW: AtomicI64 = AtomicI64::new(0);
    static PROGRESS_FIRES: AtomicU32 = AtomicU32::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        SWITCH_LAST_NOW.store(false, Ordering::SeqCst);
        SWITCH_FIRES.store(0, Ordering::SeqCst);
        CHECKBOX_LAST_NOW.store(false, Ordering::SeqCst);
        CHECKBOX_FIRES.store(0, Ordering::SeqCst);
        PROGRESS_LAST_NEW.store(0, Ordering::SeqCst);
        PROGRESS_FIRES.store(0, Ordering::SeqCst);
    }

    fn fresh_world() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(IdMap::new());
        world.insert_resource(MultiTapTracker::new());
        world.insert_resource(ViewRegistry::with_builtins());
        let root = WidgetBuilder::new(&mut world).id();
        (world, root)
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
        let (mut world, root) = fresh_world();
        let switch = ui! {
            :(
                parent: root
                world: &mut world
            :)

            Switch (width: 60, height: 32)
                on Toggled {
                    SWITCH_LAST_NOW.store(*now, Ordering::SeqCst);
                    SWITCH_FIRES.fetch_add(1, Ordering::SeqCst);
                }
                {}
        };
        world.insert(
            switch,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(60),
                h: Fixed::from_int(32),
            }),
        );
        bubble_dispatch_at(&mut world, &tap(switch, 30), 100);
        assert_eq!(SWITCH_FIRES.load(Ordering::SeqCst), 1);
        assert!(SWITCH_LAST_NOW.load(Ordering::SeqCst));

        bubble_dispatch_at(&mut world, &tap(switch, 30), 600);
        assert_eq!(SWITCH_FIRES.load(Ordering::SeqCst), 2);
        assert!(!SWITCH_LAST_NOW.load(Ordering::SeqCst));
    }

    #[test]
    fn checkbox_on_toggled_emits_with_new_state() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        let checkbox = ui! {
            :(
                parent: root
                world: &mut world
            :)

            Checkbox (width: 32, height: 32)
                on Toggled {
                    CHECKBOX_LAST_NOW.store(*now, Ordering::SeqCst);
                    CHECKBOX_FIRES.fetch_add(1, Ordering::SeqCst);
                }
                {}
        };
        world.insert(
            checkbox,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(32),
                h: Fixed::from_int(32),
            }),
        );
        bubble_dispatch_at(&mut world, &tap(checkbox, 16), 100);
        assert_eq!(CHECKBOX_FIRES.load(Ordering::SeqCst), 1);
        assert!(CHECKBOX_LAST_NOW.load(Ordering::SeqCst));
    }

    #[test]
    fn progress_bar_on_value_changed_emits_with_new_old() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        let pb = ui! {
            :(
                parent: root
                world: &mut world
            :)

            ProgressBar (width: 100, height: 16)
                on ValueChanged {
                    let _ = old;
                    PROGRESS_LAST_NEW.store((*new * 100.0) as i64, Ordering::SeqCst);
                    PROGRESS_FIRES.fetch_add(1, Ordering::SeqCst);
                }
                {}
        };
        world.insert(
            pb,
            ComputedRect(Rect {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                w: Fixed::from_int(100),
                h: Fixed::from_int(16),
            }),
        );
        bubble_dispatch_at(&mut world, &tap(pb, 50), 100);
        assert_eq!(PROGRESS_FIRES.load(Ordering::SeqCst), 1);
        assert_eq!(PROGRESS_LAST_NEW.load(Ordering::SeqCst), 50);
    }
}
