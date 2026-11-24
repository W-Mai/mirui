use super::*;

#[test]
fn inherited_state_tracks_first_insertion_and_last_removal() {
    let mut world = World::new();
    let root = world.spawn_empty();
    let parent = world.spawn_empty();
    let child = world.spawn_empty();
    world.insert(parent, Parent(root));
    world.insert(child, Parent(parent));
    world.insert(child, InteractionState::Pressed);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Pressed);
    world.insert(root, UserState::Disabled);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Disabled);
    world.remove::<UserState>(root);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Pressed);
    world.insert(parent, UserState::Errored);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Pressed);
    assert_eq!(resolve_widget_state(&world, parent), WidgetState::Error);
    world.despawn(parent);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Pressed);
}

#[test]
fn enabled_when_no_state_components() {
    let mut world = World::new();
    let e = world.spawn_empty();
    assert_eq!(resolve_widget_state(&world, e), WidgetState::Enabled);
}

#[test]
fn disabled_user_state_self() {
    let mut world = World::new();
    let e = world.spawn_empty();
    world.insert(e, UserState::Disabled);
    assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
}

#[test]
fn disabled_propagates_via_parent() {
    let mut world = World::new();
    let parent = world.spawn_empty();
    let child = world.spawn_empty();
    world.insert(child, Parent(parent));
    world.insert(parent, UserState::Disabled);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Disabled);
}

#[test]
fn errored_self_only() {
    let mut world = World::new();
    let parent = world.spawn_empty();
    let child = world.spawn_empty();
    world.insert(child, Parent(parent));
    world.insert(parent, UserState::Errored);
    assert_eq!(resolve_widget_state(&world, child), WidgetState::Enabled);
    assert_eq!(resolve_widget_state(&world, parent), WidgetState::Error);
}

#[test]
fn pressed_beats_hovered() {
    let mut world = World::new();
    let e = world.spawn_empty();
    world.insert(e, InteractionState::Pressed);
    assert_eq!(resolve_widget_state(&world, e), WidgetState::Pressed);
}

#[test]
fn hovered_when_only_hover() {
    let mut world = World::new();
    let e = world.spawn_empty();
    world.insert(e, InteractionState::Hovered);
    assert_eq!(resolve_widget_state(&world, e), WidgetState::Hovered);
}

#[test]
fn disabled_beats_pressed() {
    let mut world = World::new();
    let e = world.spawn_empty();
    world.insert(e, UserState::Disabled);
    world.insert(e, InteractionState::Pressed);
    assert_eq!(resolve_widget_state(&world, e), WidgetState::Disabled);
}
