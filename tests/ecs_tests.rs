#[cfg(test)]
mod tests {
    use mirui::ecs::*;

    #[test]
    fn spawn_and_alive() {
        let mut world = World::new();
        let e = world.spawn();
        assert!(world.is_alive(e));
    }

    #[test]
    fn despawn_invalidates() {
        let mut world = World::new();
        let e = world.spawn();
        assert!(world.despawn(e));
        assert!(!world.is_alive(e));
    }

    #[test]
    fn generation_reuse() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.despawn(e1);
        let e2 = world.spawn();
        assert_eq!(e1.id, e2.id);
        assert_ne!(e1.generation, e2.generation);
        assert!(!world.is_alive(e1));
        assert!(world.is_alive(e2));
    }

    #[test]
    fn insert_and_get_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, 42u32);
        assert_eq!(world.get::<u32>(e), Some(&42));
    }

    #[test]
    fn remove_component() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, 99i64);
        assert_eq!(world.remove::<i64>(e), Some(99));
        assert_eq!(world.get::<i64>(e), None);
    }

    #[test]
    fn despawn_removes_components() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, 10u8);
        world.despawn(e);
        // stale entity can't access component
        assert_eq!(world.get::<u8>(e), None);
    }

    #[test]
    fn multiple_components() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, 1u32);
        world.insert(e, "hello");
        assert_eq!(world.get::<u32>(e), Some(&1));
        assert_eq!(world.get::<&str>(e), Some(&"hello"));
    }

    #[test]
    fn stale_entity_no_access() {
        let mut world = World::new();
        let e1 = world.spawn();
        world.insert(e1, 100u32);
        world.despawn(e1);
        let e2 = world.spawn(); // reuses id
        // e1 is stale, should not see e2's data
        world.insert(e2, 200u32);
        assert_eq!(world.get::<u32>(e1), None);
        assert_eq!(world.get::<u32>(e2), Some(&200));
    }

    #[test]
    fn system_runs() {
        struct DoubleSystem;
        impl System for DoubleSystem {
            fn run(&self, world: &mut World) {
                let e = world.spawn();
                world.insert(e, 42u32);
            }
        }

        let mut world = World::new();
        let sys = DoubleSystem;
        sys.run(&mut world);
        // world should have one entity with component
        let e = Entity {
            id: 0,
            generation: 0,
        };
        assert_eq!(world.get::<u32>(e), Some(&42));
    }
}
