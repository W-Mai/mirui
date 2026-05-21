use crate::ecs::{Entity, World};
use crate::event::hit_test::hit_test;
use crate::surface::DisplayInfo;
use crate::types::Fixed;
use crate::widget::WidgetRoot;
use crate::widget::dirty::Dirty;

/// Skip hover/press hit_test when PointerCursor hasn't moved since last
/// frame. Without this, idle frames pay a full hit_test walk twice per
/// frame for no result change.
#[derive(Clone, Copy, Default, PartialEq)]
struct PointerSnapshot {
    x: Fixed,
    y: Fixed,
    down: bool,
    seq: u32,
}

#[derive(Default)]
struct HoverSnapshot(PointerSnapshot);
#[derive(Default)]
struct PressSnapshot(PointerSnapshot);

fn cursor_snapshot(world: &World) -> PointerSnapshot {
    let cursor = world
        .resource::<crate::event::PointerCursor>()
        .copied()
        .unwrap_or_default();
    PointerSnapshot {
        x: cursor.x,
        y: cursor.y,
        down: cursor.down,
        seq: cursor.event_seq,
    }
}

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

#[crate::system(order = INTERACTION_STATE)]
pub fn hover_system(world: &mut World) {
    let snap = cursor_snapshot(world);
    let last = world
        .resource::<HoverSnapshot>()
        .map(|s| s.0)
        .unwrap_or_default();
    if snap == last {
        return;
    }
    world.insert_resource(HoverSnapshot(snap));
    let new_hover = if snap.down {
        None
    } else {
        compute_pointer_target(world, snap.x, snap.y)
    };
    swap_marker(
        world,
        new_hover,
        |s| matches!(s, InteractionState::Hovered),
        || InteractionState::Hovered,
    );
}

#[crate::system(order = INTERACTION_STATE)]
pub fn press_system(world: &mut World) {
    let snap = cursor_snapshot(world);
    let last = world
        .resource::<PressSnapshot>()
        .map(|s| s.0)
        .unwrap_or_default();
    if snap == last {
        return;
    }
    world.insert_resource(PressSnapshot(snap));
    let new_pressed = if snap.down {
        compute_pointer_target(world, snap.x, snap.y)
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

#[cfg(all(test, feature = "std"))]
mod hover_press_e2e {
    extern crate std;
    use super::*;
    use crate::event::PointerCursor;
    use crate::layout::LayoutStyle;
    use crate::types::{Dimension, Fixed};
    use crate::widget::Style;
    use crate::widget::Widget;

    fn make_world_with_button() -> (World, Entity) {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = world.spawn();
        world.insert(root, Widget);
        world.insert(
            root,
            Style {
                layout: LayoutStyle {
                    width: Dimension::Px(Fixed::from_int(64)),
                    height: Dimension::Px(Fixed::from_int(64)),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert_resource(WidgetRoot(root));
        crate::widget::render_system::update_layout(
            &mut world,
            root,
            &crate::types::Viewport::new(64, 64, Fixed::ONE),
        );
        (world, root)
    }

    #[test]
    fn hover_system_marks_pointer_target_when_not_down() {
        let (mut world, root) = make_world_with_button();
        world.insert_resource(PointerCursor {
            x: Fixed::from_int(32),
            y: Fixed::from_int(32),
            down: false,
            event_seq: 1,
        });
        hover_system(&mut world);
        assert!(matches!(
            world.get::<InteractionState>(root),
            Some(InteractionState::Hovered)
        ));
    }

    #[test]
    fn hover_system_clears_when_down() {
        let (mut world, root) = make_world_with_button();
        world.insert(root, InteractionState::Hovered);
        world.insert_resource(PointerCursor {
            x: Fixed::from_int(32),
            y: Fixed::from_int(32),
            down: true,
            event_seq: 1,
        });
        hover_system(&mut world);
        assert!(world.get::<InteractionState>(root).is_none());
    }

    #[test]
    fn press_system_marks_when_down() {
        let (mut world, root) = make_world_with_button();
        world.insert_resource(PointerCursor {
            x: Fixed::from_int(32),
            y: Fixed::from_int(32),
            down: true,
            event_seq: 1,
        });
        press_system(&mut world);
        assert!(matches!(
            world.get::<InteractionState>(root),
            Some(InteractionState::Pressed)
        ));
    }
}
