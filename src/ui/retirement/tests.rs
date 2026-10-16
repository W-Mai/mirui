use super::*;
use crate::ecs::{CommandCapacity, commands::CommandQueue};
use crate::prelude::*;
use crate::types::Transform;
use crate::ui::{Outputs, WidgetTexture};
use alloc::vec;

fn attach(world: &mut World, parent: Entity) -> Entity {
    let child = world.spawn_empty();
    world.insert(child, Parent(parent));
    world.insert(child, Children(vec![]));
    world.get_mut::<Children>(parent).unwrap().0.push(child);
    child
}

#[test]
fn retirement_follows_tree_without_widget_markers_or_business_references() {
    #[crate::component]
    struct Reference(Entity);
    let mut world = World::new();
    let root = world.spawn_empty();
    world.insert(root, Children(vec![]));
    let before = attach(&mut world, root);
    let target = attach(&mut world, root);
    let leaf = attach(&mut world, target);
    let after = attach(&mut world, root);
    world.insert(target, Reference(after));
    assert_eq!(world.get::<Reference>(target).unwrap().0, after);
    let mut queue = CommandQueue::bounded(CommandCapacity::new().despawn(1)).unwrap();
    let mut batch = queue.begin();
    batch.commands().despawn(target);
    assert!(world.is_alive(leaf));
    batch.finish(&mut world).unwrap();
    assert!(!world.is_alive(target));
    assert!(!world.is_alive(leaf));
    assert!(world.is_alive(after));
    assert_eq!(world.get::<Children>(root).unwrap().0, [before, after]);

    let replacement = world.spawn_empty();
    assert_eq!(replacement.id, target.id);
    despawn_subtree(&mut world, target);
    assert!(world.is_alive(replacement));
}

#[test]
fn root_protection_precedes_mutation_and_does_not_stop_the_command_batch() {
    let mut app = App::headless(16, 16);
    let root = app.spawn_root().id();
    let ancestor = app.world.spawn_empty();
    app.world.insert(ancestor, Children(vec![root]));
    app.world.insert(root, Parent(ancestor));
    let child = attach(&mut app.world, root);
    let unrelated = app.world.spawn_empty();
    let mut queue = CommandQueue::bounded(CommandCapacity::new().despawn(3)).unwrap();
    let mut batch = queue.begin();
    batch.commands().despawn(ancestor);
    batch.commands().despawn(root);
    batch.commands().despawn(unrelated);
    batch.finish(&mut app.world).unwrap();
    assert!(app.world.is_alive(ancestor));
    assert!(app.world.is_alive(root));
    assert!(app.world.is_alive(child));
    assert_eq!(app.world.get::<Children>(ancestor).unwrap().0, [root]);
    assert_eq!(app.world.get::<Children>(root).unwrap().0, [child]);
    assert_eq!(app.world.resource::<WidgetRoot>().unwrap().0, root);
    assert!(!app.world.is_alive(unrelated));
}

#[test]
fn retirement_of_a_deep_bound_tree_needs_no_heap_or_recursive_stack() {
    let mut world = World::new();
    let root = world.spawn_empty();
    world.insert(root, Children(vec![]));
    let source = crate::core::reactive::Signal::new(7u8);
    let mut parent = root;
    for _ in 0..4096 {
        parent = attach(&mut world, parent);
    }
    let source_copy = source.clone();
    crate::core::reactive::with_world_scope(&mut world, || {
        crate::core::reactive::effect_with_widget(parent, move || {
            source_copy.get();
        });
    });
    assert_eq!(
        crate::test_support::tracked_allocations(|| despawn_subtree(&mut world, root)),
        0,
    );
    assert_eq!(world.live_entity_count(), 0);
    source.set(8);
    crate::core::reactive::flush_signal_dirty(&mut world);
    assert_eq!(world.live_entity_count(), 0);
}

#[test]
fn retirement_does_not_remove_an_id_rebound_to_another_generation() {
    let mut world = World::new();
    let old = world.spawn_empty();
    let current = world.spawn_empty();
    world.insert(old, NamedId("label"));
    let mut map = IdMap::new();
    map.insert("label", current);
    world.insert_resource(map);
    despawn_subtree(&mut world, old);
    assert_eq!(world.find_by_id("label"), Some(current));
}

#[test]
fn retirement_preserves_painted_coverage_without_full_viewport_damage() {
    use crate::ui::dirty::{ExactDirtyRegions, PaintedCoverage};
    let mut app = App::headless(128, 128);
    let root = app.spawn_root().id();
    let target = attach(&mut app.world, root);
    let leaf = attach(&mut app.world, target);
    app.world.insert(
        target,
        PaintedCoverage(crate::ui::dirty::PaintCoverage::solid(Rect::new(
            10, 20, 4, 5,
        ))),
    );
    app.world.insert(
        leaf,
        PaintedCoverage(crate::ui::dirty::PaintCoverage::solid(Rect::new(
            30, 40, 5, 6,
        ))),
    );
    app.world.invalidate(root);
    assert_eq!(
        crate::test_support::tracked_allocations(|| {
            despawn_subtree(&mut app.world, target);
        }),
        0
    );
    let mut rects = alloc::vec::Vec::new();
    app.world
        .resource_mut::<ExactDirtyRegions>()
        .unwrap()
        .prepare_into(&mut rects);
    assert_eq!(rects, [Rect::new(30, 40, 5, 6), Rect::new(10, 20, 4, 5)]);
}

#[test]
fn retired_overflow_is_erased_by_dirty_render() {
    use crate::surface::FramebufferAccess;
    use crate::ui::widgets::transform::WidgetTransform;
    use crate::ui::widgets::{DropShadow, MirrorOf};

    let mut app = App::headless(64, 64);
    app.with_default_widgets()
        .with_offscreen_pool_budget(16 * 1024);
    let root = app.spawn_root().id();
    let target = app.compose(root, |cx| {
        ui! {
            View (width: 8, height: 8, bg_color: Color::rgb(200, 0, 0)) {
                View (
                    width: 12,
                    height: 12,
                    left: 28,
                    top: 24,
                    position: Position::Absolute,
                    bg_color: Color::rgb(0, 200, 0)
                ) [
                    WidgetTransform(Transform::scale(Fixed::from_int(2), Fixed::from_int(2))),
                ]
            }
        }
    });
    let source = app.world.get::<Children>(target).unwrap().0[0];
    let mirror = app.compose(root, |cx| {
        ui! {
            View (width: 12, height: 12) [
                MirrorOf::new(source),
            ]
        }
    });
    app.compose(root, |cx| {
        ui! {
            View (width: 12, height: 12) [
                DropShadow::new(source),
            ]
        }
    });
    app.render().unwrap();
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .is_some()
    );
    let before = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    despawn_subtree(&mut app.world, target);
    app.render_dirty().unwrap();
    assert!(app.world.is_alive(mirror));
    assert!(
        app.world
            .resource::<Outputs<WidgetTexture>>()
            .unwrap()
            .get(source)
            .is_none()
    );
    let dirty = app
        .backend
        .framebuffer()
        .read_pixels()
        .unwrap()
        .bytes()
        .to_vec();
    assert_ne!(before, dirty);
    app.render().unwrap();
    assert_eq!(
        dirty,
        app.backend.framebuffer().read_pixels().unwrap().bytes()
    );
}
