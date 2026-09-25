use std::any::TypeId;

use mirui::ecs::World;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use tracking_allocator::tracked_allocations;

#[test]
fn reserved_component_change_queue_reuses_storage() {
    let mut world = World::new();
    let entity = world.spawn_empty();
    world.insert(entity, 1u16);
    world.watch_component_type(TypeId::of::<u16>());
    world.reserve_component_changes(1);

    let allocations = tracked_allocations(|| {
        world.insert(entity, 2u16);
        world.insert(entity, 3u16);
        world.drain_component_changes(|world, change| {
            assert_eq!(change.entity, entity);
            assert_eq!(world.get::<u16>(entity), Some(&3));
        });
    });
    assert_eq!(allocations, 0);
}
