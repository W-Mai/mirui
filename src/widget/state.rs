use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::surface::DisplayInfo;
use crate::widget::WidgetRoot;
use crate::widget::dirty::Dirty;

/// User-set state. `Disabled` propagates to descendants; `Errored` is self-only.
pub enum UserState {
    Disabled,
    Errored,
}

/// Driven by `hover_system` / `press_system`; user shouldn't write directly.
pub enum InteractionState {
    Hovered,
    Pressed,
}

#[crate::system(order = SIM_INPUT)]
pub fn hover_system(world: &mut World) {
    let cursor = world
        .resource::<crate::event::PointerCursor>()
        .copied()
        .unwrap_or_default();
    let new_hover = if cursor.down {
        None
    } else {
        compute_pointer_target(world, cursor.x, cursor.y)
    };
    swap_marker(
        world,
        new_hover,
        |s| matches!(s, InteractionState::Hovered),
        || InteractionState::Hovered,
    );
}

#[crate::system(order = SIM_INPUT)]
pub fn press_system(world: &mut World) {
    let cursor = world
        .resource::<crate::event::PointerCursor>()
        .copied()
        .unwrap_or_default();
    let new_pressed = if cursor.down {
        compute_pointer_target(world, cursor.x, cursor.y)
    } else {
        None
    };
    swap_marker(
        world,
        new_pressed,
        |s| matches!(s, InteractionState::Pressed),
        || InteractionState::Pressed,
    );
}

fn compute_pointer_target(
    world: &World,
    x: crate::types::Fixed,
    y: crate::types::Fixed,
) -> Option<Entity> {
    let root = world.resource::<WidgetRoot>().copied()?.0;
    let info = world.resource::<DisplayInfo>()?;
    hit_test(world, root, x, y, info.width, info.height)
}

fn swap_marker(
    world: &mut World,
    new_target: Option<Entity>,
    is_state: impl Fn(&InteractionState) -> bool,
    make_state: impl Fn() -> InteractionState,
) {
    let prev: Option<Entity> = world
        .query::<InteractionState>()
        .iter()
        .find_map(|(e, s)| if is_state(s) { Some(e) } else { None });
    if prev == new_target {
        return;
    }
    if let Some(p) = prev {
        world.remove::<InteractionState>(p);
        world.insert(p, Dirty);
    }
    if let Some(n) = new_target {
        world.insert(n, make_state());
        world.insert(n, Dirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_marker_inserts_when_target_arrives() {
        let mut world = World::new();
        let e = world.spawn();
        swap_marker(
            &mut world,
            Some(e),
            |s| matches!(s, InteractionState::Hovered),
            || InteractionState::Hovered,
        );
        assert!(matches!(
            world.get::<InteractionState>(e),
            Some(InteractionState::Hovered)
        ));
    }

    #[test]
    fn swap_marker_removes_when_target_leaves() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, InteractionState::Hovered);
        swap_marker(
            &mut world,
            None,
            |s| matches!(s, InteractionState::Hovered),
            || InteractionState::Hovered,
        );
        assert!(world.get::<InteractionState>(e).is_none());
    }

    #[test]
    fn swap_marker_moves_when_target_changes() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.insert(a, InteractionState::Hovered);
        swap_marker(
            &mut world,
            Some(b),
            |s| matches!(s, InteractionState::Hovered),
            || InteractionState::Hovered,
        );
        assert!(world.get::<InteractionState>(a).is_none());
        assert!(matches!(
            world.get::<InteractionState>(b),
            Some(InteractionState::Hovered)
        ));
    }

    #[test]
    fn swap_marker_noop_when_target_unchanged() {
        let mut world = World::new();
        let e = world.spawn();
        world.insert(e, InteractionState::Hovered);
        assert!(world.get::<crate::widget::dirty::Dirty>(e).is_none());
        swap_marker(
            &mut world,
            Some(e),
            |s| matches!(s, InteractionState::Hovered),
            || InteractionState::Hovered,
        );
        assert!(world.get::<crate::widget::dirty::Dirty>(e).is_none());
    }
}
