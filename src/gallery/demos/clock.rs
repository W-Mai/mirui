use crate::ecs::MonoClock;

#[mirui_macros::compose]
pub(super) fn ensure_clock() {
    let installed = res!(optional MonoClock).with(|clock| clock.is_some());
    if !installed {
        res!().insert(MonoClock::from_time_source());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::ui::Widget;

    #[test]
    fn default_clock_initialization_preserves_existing_state_and_tree() {
        let mut app = App::headless(16, 16);
        let root = app.spawn_root().id();
        let nodes = app.world.query::<Widget>().iter().count();

        app.compose(root, ensure_clock());
        let clock = app.world.resource::<MonoClock>().unwrap();
        assert!(core::ptr::fn_addr_eq(
            clock.clock,
            crate::core::time::clock_now_ns as fn() -> u64,
        ));

        fn custom_clock() -> u64 {
            123_000_000
        }
        app.add_resource(MonoClock {
            clock: custom_clock,
            last_ns: 17,
        });
        for _ in 0..2 {
            app.compose(root, ensure_clock());
            let clock = app.world.resource::<MonoClock>().unwrap();
            assert!(core::ptr::fn_addr_eq(
                clock.clock,
                custom_clock as fn() -> u64
            ));
            assert_eq!(clock.last_ns, 17);
        }
        assert_eq!(app.world.query::<Widget>().iter().count(), nodes);
    }
}
