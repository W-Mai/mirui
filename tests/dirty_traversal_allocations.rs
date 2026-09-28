#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::ecs::World;
use mirui::types::Rect;
use mirui::ui::{
    Children, ComputedRect, Hidden, Style, Widget, branch,
    dirty::{Dirty, PrevRect},
};
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

#[test]
fn first_hidden_reveal_after_same_size_reparenting_reuses_reserved_stack() {
    let mut app = App::headless(64, 64);
    let root = app.spawn_root().id();
    let shallow = app.world.spawn_empty();
    let hidden = app.world.spawn_empty();
    let children: Vec<_> = (0..8).map(|_| app.world.spawn_empty()).collect();
    app.world.insert(root, Children(vec![shallow, hidden]));
    app.world.insert(shallow, Children(children.clone()));
    app.world.insert(hidden, Hidden);
    app.render().unwrap();

    app.world.insert(shallow, Children(Vec::new()));
    app.world.insert(hidden, Children(children.clone()));
    app.world.remove::<Hidden>(hidden);
    let allocations = tracked_allocations(|| app.world.mark_subtree_dirty(hidden));

    assert_eq!(allocations, 0);
    assert!(
        children
            .iter()
            .all(|&entity| app.world.has::<Dirty>(entity))
    );
}

#[test]
fn cached_branches_reserve_geometry_components_without_creating_placeholders() {
    let mut world = World::new();
    let parent = world.spawn_empty();
    let first = world.spawn_empty();
    let first_child = world.spawn_empty();
    let second = world.spawn_empty();
    let second_child = world.spawn_empty();
    world.insert(parent, Children(vec![first, second]));
    world.insert(first, Children(vec![first_child]));
    world.insert(second, Children(vec![second_child]));
    for entity in [first, first_child, second, second_child] {
        world.insert(entity, Widget);
        world.insert(entity, Style::default());
    }
    let branches = [vec![first], vec![second]];
    branch::prepare(&mut world, parent, &branches);
    assert!(world.get::<ComputedRect>(second_child).is_none());
    assert!(world.get::<PrevRect>(second_child).is_none());

    let geometry = Rect::new(1, 2, 3, 4);
    let allocations = tracked_allocations(|| {
        for entity in [first, first_child, second, second_child] {
            world.insert(entity, ComputedRect(geometry));
            world.insert(entity, PrevRect(geometry));
        }
    });
    assert_eq!(allocations, 0);
    assert_eq!(world.get::<ComputedRect>(second_child).unwrap().0, geometry);
    assert_eq!(world.get::<PrevRect>(second_child).unwrap().0, geometry);
}
