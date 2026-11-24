use super::*;

#[test]
fn input_lease_checks_every_retained_ancestor_without_allocating() {
    let mut world = World::new();
    let outer = world.spawn_empty();
    let inner = world.spawn_empty();
    let child = world.spawn_empty();
    let unrelated = world.spawn_empty();
    world.insert(inner, Parent(outer));
    world.insert(child, Parent(inner));
    for entity in [outer, inner, unrelated] {
        let gate = RetainedEffects::new(&mut world, |_, _| true);
        world.insert(entity, gate);
    }
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            let lease = InputLease::capture(&world, child).unwrap();
            retire(&mut world, unrelated);
            assert!(lease.valid(&world));
            retire(&mut world, outer);
            assert!(!lease.valid(&world));
            let lease = InputLease::capture(&world, child).unwrap();
            assert!(lease.valid(&world));
            retire(&mut world, inner);
            retire(&mut world, inner);
            assert!(!lease.valid(&world));
            assert!(InputLease::capture(&world, child).unwrap().valid(&world));
            world.get_mut::<RetainedEffects>(outer).unwrap().active = false;
            assert!(!enabled(&world, child));
        }),
        0
    );
}
