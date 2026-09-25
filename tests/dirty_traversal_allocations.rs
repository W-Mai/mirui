#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::ecs::World;
use mirui::ui::{Children, dirty::Dirty};
use tracking_allocator::tracked_allocations;

#[test]
fn warmed_subtree_dirty_traversal_reuses_its_stack() {
    let mut world = World::new();
    let root = world.spawn_empty();
    let left = world.spawn_empty();
    let right = world.spawn_empty();
    let leaf = world.spawn_empty();
    world.insert(root, Children(vec![left, right]));
    world.insert(left, Children(vec![leaf]));

    world.mark_subtree_dirty(root);
    world.clear_subtree_dirty(root);

    let allocations = tracked_allocations(|| {
        for _ in 0..20_000 {
            world.mark_subtree_dirty(root);
            world.clear_subtree_dirty(root);
        }
    });

    assert_eq!(allocations, 0);
    assert!(world.get::<Dirty>(root).is_none());
    assert!(world.get::<Dirty>(leaf).is_none());
}
