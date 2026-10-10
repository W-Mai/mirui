use crate::ecs::Entity;
#[cfg(any(target_os = "android", target_os = "ios", test))]
use crate::ecs::World;

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

#[cfg(any(target_os = "android", target_os = "ios", test))]
pub(crate) fn text_input_requested(world: &World) -> bool {
    world
        .resource::<FocusState>()
        .and_then(|state| state.focused)
        .is_some_and(|entity| world.get::<TextEditable>(entity).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

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
