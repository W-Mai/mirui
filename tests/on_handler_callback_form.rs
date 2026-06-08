#[cfg(test)]
mod tests {
    use mirui::ecs::{Entity, World};
    use mirui::event::bubble_dispatch_at;
    use mirui::event::gesture::GestureEvent;
    use mirui::event::multi_tap::MultiTapTracker;
    use mirui::types::Fixed;
    use mirui::ui;
    use mirui::widget::IdMap;
    use mirui::widget::builder::WidgetBuilder;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI64, Ordering};

    static FIRES: AtomicI64 = AtomicI64::new(0);
    static SERIAL: Mutex<()> = Mutex::new(());

    fn reset() {
        FIRES.store(0, Ordering::SeqCst);
    }

    fn my_callback(_w: &mut World, _e: Entity, _ev: &GestureEvent) -> bool {
        FIRES.fetch_add(1, Ordering::SeqCst);
        true
    }

    fn fresh_world() -> (World, Entity) {
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
    fn callback_form_no_count() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap(my_callback)
        };
        let target = world.query::<mirui::event::GestureHandler>().collect();
        let entity = *target.first().expect("dispatch fn attached");
        bubble_dispatch_at(&mut world, &tap(entity), 100);
        assert_eq!(FIRES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn callback_form_with_count() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () on Tap(2, my_callback)
        };
        let target = world.query::<mirui::event::GestureHandler>().collect();
        let entity = *target.first().expect("dispatch fn attached");
        bubble_dispatch_at(&mut world, &tap(entity), 100);
        assert_eq!(FIRES.load(Ordering::SeqCst), 0, "single Tap silent for count=2");
        bubble_dispatch_at(&mut world, &tap(entity), 200);
        assert_eq!(FIRES.load(Ordering::SeqCst), 1, "double Tap fires callback");
    }

    #[test]
    fn callback_and_body_forms_coexist() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        reset();
        let (mut world, root) = fresh_world();
        ui! {
            :(
                parent: root
                world: &mut world
            :)

            View () {}
                on Tap(my_callback)
                on LongPress { FIRES.fetch_add(10, Ordering::SeqCst); }
        };
        let target = world.query::<mirui::event::GestureHandler>().collect();
        let entity = *target.first().expect("dispatch fn attached");
        bubble_dispatch_at(&mut world, &tap(entity), 100);
        assert_eq!(FIRES.load(Ordering::SeqCst), 1, "Tap callback fires");

        let lp = GestureEvent::LongPress {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            target: entity,
        };
        bubble_dispatch_at(&mut world, &lp, 200);
        assert_eq!(FIRES.load(Ordering::SeqCst), 11, "LongPress body fires += 10");
    }
}
