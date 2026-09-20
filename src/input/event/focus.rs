use crate::ecs::{Entity, World};
use crate::surface::InputEvent;
use crate::ui::Parent;

use super::gesture::GestureEvent;

/// Marker component: this entity can receive keyboard/char input.
pub struct Focusable;

/// Marker for a focusable entity that accepts native text input.
///
/// Mobile hosts use this to show or hide the software keyboard without
/// coupling custom text editors to a concrete widget implementation.
pub struct TextEditable;

/// World resource tracking which entity currently has keyboard focus.
#[derive(Default)]
pub struct FocusState {
    pub focused: Option<Entity>,
}

/// Handler component for receiving Key/CharInput events on focused entities.
pub struct KeyHandler {
    pub on_key: fn(&mut World, Entity, &InputEvent) -> bool,
}

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn text_input_requested(world: &World) -> bool {
    world
        .resource::<FocusState>()
        .and_then(|state| state.focused)
        .is_some_and(|entity| world.get::<TextEditable>(entity).is_some())
}

/// On Tap gesture, walk from target upward looking for a `Focusable`
/// entity. If found, set focus to it; otherwise blur.
pub fn focus_on_tap(world: &mut World, event: &GestureEvent) {
    let target = match event {
        GestureEvent::Tap { target, .. } => *target,
        _ => return,
    };

    let focused_entity = find_focusable(world, target);

    if let Some(fs) = world.resource_mut::<FocusState>() {
        fs.focused = focused_entity;
    }
}

fn find_focusable(world: &World, entity: Entity) -> Option<Entity> {
    if super::entity_or_ancestor_disabled(world, entity) {
        return None;
    }
    let mut cur = entity;
    loop {
        if world.get::<Focusable>(cur).is_some() {
            return Some(cur);
        }
        let p = world.get::<Parent>(cur)?;
        cur = p.0;
    }
}

/// Route Key/CharInput events to the focused entity's KeyHandler.
pub fn key_dispatch(world: &mut World, event: &InputEvent) {
    let target = match world.resource::<FocusState>() {
        Some(fs) => match fs.focused {
            Some(e) => e,
            None => return,
        },
        None => return,
    };

    match event {
        InputEvent::Key { .. } | InputEvent::CharInput { .. } => {}
        _ => return,
    }

    if super::entity_or_ancestor_disabled(world, target) {
        return;
    }

    let handler_fn = world.get::<KeyHandler>(target).map(|h| h.on_key);
    if let Some(f) = handler_fn {
        f(world, target, event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::UserState;

    #[test]
    fn focus_skips_disabled_subtree() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(child, Focusable);
        world.insert(parent, UserState::Disabled);
        assert_eq!(find_focusable(&world, child), None);
    }

    #[test]
    fn focus_ignores_disabled_self() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, Focusable);
        world.insert(e, UserState::Disabled);
        assert_eq!(find_focusable(&world, e), None);
    }

    #[test]
    fn focus_walks_through_non_disabled() {
        let mut world = World::new();
        let parent = world.spawn_empty();
        let child = world.spawn_empty();
        world.insert(child, Parent(parent));
        world.insert(parent, Focusable);
        assert_eq!(find_focusable(&world, child), Some(parent));
    }

    #[test]
    fn native_text_input_requires_a_focused_editable() {
        let mut world = World::new();
        world.insert_resource(FocusState::default());
        let entity = world.spawn_empty();
        world.insert(entity, Focusable);
        world.resource_mut::<FocusState>().unwrap().focused = Some(entity);
        assert!(!text_input_requested(&world));

        world.insert(entity, TextEditable);
        assert!(text_input_requested(&world));

        world.resource_mut::<FocusState>().unwrap().focused = None;
        assert!(!text_input_requested(&world));
    }
}
