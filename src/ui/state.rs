use crate::ecs::{Entity, World};
use crate::input::event::hit_test::hit_test;
use crate::surface::DisplayInfo;
use crate::types::Fixed;
use crate::ui::dirty::Dirty;
use crate::ui::{IgnoreHitTest, Parent, WidgetRoot};

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
struct PressSnapshot {
    pointer: PointerSnapshot,
    target: Option<Entity>,
}

fn cursor_snapshot(world: &World) -> PointerSnapshot {
    let cursor = world
        .resource::<crate::input::event::PointerCursor>()
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    swap_markers(
        world,
        new_hover,
        |s| matches!(s, InteractionState::Hovered),
        InteractionState::Hovered,
    );
}

#[crate::system(order = INTERACTION_STATE)]
pub fn press_system(world: &mut World) {
    let snap = cursor_snapshot(world);
    let last = world
        .resource::<PressSnapshot>()
        .map(|s| s.pointer)
        .unwrap_or_default();
    if snap == last {
        return;
    }

    // Mid-drag: skip the ~1.4 ms hit_test while the pointer stays
    // inside the deepest Pressed entity's rect.
    if snap.down && last.down {
        let prev_pressed: Option<Entity> = world
            .resource::<PressSnapshot>()
            .and_then(|snapshot| snapshot.target);
        if let Some(p) = prev_pressed
            && let Some(rect) = world.get::<crate::ui::ComputedRect>(p).map(|r| r.0)
            && snap.x >= rect.x
            && snap.x < rect.x + rect.w
            && snap.y >= rect.y
            && snap.y < rect.y + rect.h
        {
            world.insert_resource(PressSnapshot {
                pointer: snap,
                target: prev_pressed,
            });
            return;
        }
    }

    let new_pressed = if snap.down {
        compute_pointer_target(world, snap.x, snap.y)
    } else {
        None
    };
    world.insert_resource(PressSnapshot {
        pointer: snap,
        target: new_pressed,
    });
    swap_markers(
        world,
        new_pressed,
        |s| matches!(s, InteractionState::Pressed),
        InteractionState::Pressed,
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

fn on_hit_path(world: &World, target: Option<Entity>, candidate: Entity) -> bool {
    let mut current = target;
    while let Some(entity) = current {
        if entity == candidate {
            return true;
        }
        current = world.get::<Parent>(entity).map(|parent| parent.0);
    }
    false
}

fn swap_markers(
    world: &mut World,
    new_target: Option<Entity>,
    is_state: impl Fn(&InteractionState) -> bool,
    state: InteractionState,
) {
    loop {
        let stale = world
            .query::<InteractionState>()
            .iter()
            .find_map(|(entity, current)| {
                (is_state(current)
                    && (!on_hit_path(world, new_target, entity)
                        || world.get::<IgnoreHitTest>(entity).is_some()))
                .then_some(entity)
            });
        let Some(entity) = stale else {
            break;
        };
        world.remove::<InteractionState>(entity);
        world.insert(entity, Dirty);
    }

    let mut current = new_target;
    while let Some(entity) = current {
        let parent = world.get::<Parent>(entity).map(|parent| parent.0);
        if world.get::<IgnoreHitTest>(entity).is_none()
            && !world.get::<InteractionState>(entity).is_some_and(&is_state)
        {
            world.insert(entity, state);
            world.insert(entity, Dirty);
        }
        current = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_marker_inserts_when_target_arrives() {
        let mut world = World::new();
        let e = world.spawn_empty();
        swap_markers(
            &mut world,
            Some(e),
            |s| matches!(s, InteractionState::Hovered),
            InteractionState::Hovered,
        );
        assert!(matches!(
            world.get::<InteractionState>(e),
            Some(InteractionState::Hovered)
        ));
    }

    #[test]
    fn swap_marker_removes_when_target_leaves() {
        let mut world = World::new();
        let e = world.spawn_empty();
        world.insert(e, InteractionState::Hovered);
        swap_markers(
            &mut world,
            None,
            |s| matches!(s, InteractionState::Hovered),
            InteractionState::Hovered,
        );
        assert!(world.get::<InteractionState>(e).is_none());
    }

    #[test]
    fn swap_marker_moves_when_target_changes() {
        let mut world = World::new();
        let a = world.spawn_empty();
        let b = world.spawn_empty();
        world.insert(a, InteractionState::Hovered);
        swap_markers(
            &mut world,
            Some(b),
            |s| matches!(s, InteractionState::Hovered),
            InteractionState::Hovered,
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
        let e = world.spawn_empty();
        world.insert(e, InteractionState::Hovered);
        assert!(world.get::<crate::ui::dirty::Dirty>(e).is_none());
        swap_markers(
            &mut world,
            Some(e),
            |s| matches!(s, InteractionState::Hovered),
            InteractionState::Hovered,
        );
        assert!(world.get::<crate::ui::dirty::Dirty>(e).is_none());
    }
}

#[cfg(all(test, feature = "std"))]
mod hover_press_e2e {
    extern crate std;
    use super::*;
    use crate::input::event::GestureHandler;
    use crate::input::event::PointerCursor;
    use crate::input::event::gesture::GestureEvent;
    use crate::types::{Dimension, Fixed};
    use crate::ui::layout::{LayoutStyle, Position};
    use crate::ui::widgets::Text;
    use crate::ui::{Children, Parent, Style, Widget};

    fn make_world_with_button() -> (World, Entity) {
        let mut app = crate::app::App::headless(64, 64);
        app.with_default_widgets();
        let mut world = app.world;
        let root = world.spawn_empty();
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
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &crate::types::Viewport::new(64, 64, Fixed::ONE),
        );
        (world, root)
    }

    fn make_world_with_text_child() -> (World, Entity, Entity) {
        let (mut world, root) = make_world_with_button();
        let child = world.spawn_empty();
        world.insert(child, Widget);
        world.insert(child, Parent(root));
        world.insert(child, Text::from("Tap me"));
        world.insert(
            child,
            Style {
                layout: LayoutStyle {
                    position: Position::Absolute,
                    left: Dimension::px(8),
                    top: Dimension::px(8),
                    width: Dimension::px(40),
                    height: Dimension::px(24),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        world.insert(root, Children(alloc::vec![child]));
        crate::ui::render_system::update_layout(
            &mut world,
            root,
            &crate::types::Viewport::new(64, 64, Fixed::ONE),
        );
        (world, root, child)
    }

    #[derive(Default)]
    struct TapCounts {
        parent: u8,
        child: u8,
    }

    fn parent_tap(world: &mut World, _: Entity, _: &GestureEvent) -> bool {
        world.resource_mut::<TapCounts>().unwrap().parent += 1;
        true
    }

    fn child_tap(world: &mut World, _: Entity, _: &GestureEvent) -> bool {
        world.resource_mut::<TapCounts>().unwrap().child += 1;
        true
    }

    #[test]
    fn tap_on_text_reaches_its_own_handler_or_bubbles_to_parent() {
        let (mut world, root, child) = make_world_with_text_child();
        world.insert_resource(TapCounts::default());
        world.insert(root, GestureHandler::from_fn(parent_tap));
        let probe = Fixed::from_int(16);
        let target = crate::input::event::hit_test::hit_test(&world, root, probe, probe, 64, 64)
            .expect("text target");
        assert_eq!(target, child);
        let tap = GestureEvent::Tap {
            x: probe,
            y: probe,
            target,
        };
        crate::input::event::bubble_dispatch_at(&mut world, &tap, 0);
        assert_eq!(world.resource::<TapCounts>().unwrap().parent, 1);

        world.insert(child, GestureHandler::from_fn(child_tap));
        crate::input::event::bubble_dispatch_at(&mut world, &tap, 0);
        let counts = world.resource::<TapCounts>().unwrap();
        assert_eq!(counts.child, 1);
        assert_eq!(counts.parent, 1);
    }

    #[test]
    fn child_text_keeps_its_parent_hovered() {
        let (mut world, root, child) = make_world_with_text_child();
        let probe = Fixed::from_int(16);
        assert_eq!(
            crate::input::event::hit_test::hit_test(&world, root, probe, probe, 64, 64),
            Some(child)
        );
        world.insert_resource(PointerCursor {
            x: probe,
            y: probe,
            down: false,
            event_seq: 1,
        });
        hover_system(&mut world);
        assert_eq!(
            world.get::<InteractionState>(child),
            Some(&InteractionState::Hovered)
        );
        assert_eq!(
            world.get::<InteractionState>(root),
            Some(&InteractionState::Hovered)
        );

        world.insert_resource(PointerCursor {
            x: Fixed::from_int(56),
            y: Fixed::from_int(56),
            down: false,
            event_seq: 1,
        });
        hover_system(&mut world);
        assert!(world.get::<InteractionState>(child).is_none());
        assert_eq!(
            world.get::<InteractionState>(root),
            Some(&InteractionState::Hovered)
        );
    }

    #[test]
    fn ignored_parent_does_not_receive_child_interaction_state() {
        let (mut world, root, child) = make_world_with_text_child();
        world.insert(root, IgnoreHitTest);
        let probe = Fixed::from_int(16);
        world.insert_resource(PointerCursor {
            x: probe,
            y: probe,
            down: false,
            event_seq: 1,
        });
        hover_system(&mut world);
        assert_eq!(
            world.get::<InteractionState>(child),
            Some(&InteractionState::Hovered)
        );
        assert!(world.get::<InteractionState>(root).is_none());

        world.insert_resource(PointerCursor {
            x: probe,
            y: probe,
            down: true,
            event_seq: 2,
        });
        press_system(&mut world);
        assert_eq!(
            world.get::<InteractionState>(child),
            Some(&InteractionState::Pressed)
        );
        assert!(world.get::<InteractionState>(root).is_none());
    }

    #[test]
    fn child_text_keeps_its_parent_pressed() {
        let (mut world, root, child) = make_world_with_text_child();
        let probe = Fixed::from_int(16);
        world.insert_resource(PointerCursor {
            x: probe,
            y: probe,
            down: true,
            event_seq: 1,
        });
        press_system(&mut world);
        assert_eq!(
            world.get::<InteractionState>(child),
            Some(&InteractionState::Pressed)
        );
        assert_eq!(
            world.get::<InteractionState>(root),
            Some(&InteractionState::Pressed)
        );

        world.insert_resource(PointerCursor {
            x: probe,
            y: probe,
            down: false,
            event_seq: 2,
        });
        press_system(&mut world);
        assert!(world.get::<InteractionState>(child).is_none());
        assert!(world.get::<InteractionState>(root).is_none());
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
