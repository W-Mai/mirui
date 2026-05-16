use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::components::progress_bar::ProgressBar;
use crate::components::tabbar::TabBar;
use crate::components::text_input::TextInput;
use crate::ecs::{Entity, World};
use crate::types::Fixed;
use crate::widget::dirty::Dirty;

use super::GestureHandler;
use super::focus::{Focusable, KeyHandler};
use super::gesture::GestureEvent;
use super::input::{InputEvent, KEY_BACKSPACE, KEY_DELETE, KEY_END, KEY_HOME, KEY_LEFT, KEY_RIGHT};

/// Resource toggled by `cursor_blink_system` every ~500 ms; renderers
/// query this to decide whether to draw the TextInput cursor.
#[derive(Default, Clone, Copy)]
pub struct CursorBlinkPhase(pub bool);

/// Reads the global MonoClock, flips `CursorBlinkPhase` every 500 ms,
/// and marks every focused TextInput as Dirty so it repaints. Idle
/// cost when no TextInput is present: one resource read + one
/// arithmetic check.
pub fn cursor_blink_system(world: &mut World) {
    let now_ms = match world.resource::<crate::ecs::MonoClock>() {
        Some(c) => (c.clock)() / 1_000_000,
        None => return,
    };
    let new_phase = (now_ms / 500) % 2 == 0;
    let prev_phase = world
        .resource::<CursorBlinkPhase>()
        .map(|p| p.0)
        .unwrap_or(!new_phase);
    if new_phase == prev_phase {
        return;
    }
    world.insert_resource(CursorBlinkPhase(new_phase));
    let entities: alloc::vec::Vec<_> = world.query::<TextInput>().collect();
    for e in entities {
        if world
            .get::<TextInput>(e)
            .map(|t| t.focused)
            .unwrap_or(false)
        {
            world.insert(e, Dirty);
        }
    }
}

/// Press feedback on Button: highlight while the gesture is in flight,
/// release on Tap / DragEnd. Without DragStart we'd never see a "held"
/// state because Tap is press+release in one event.
pub(crate) fn button_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragStart { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = true;
            }
            world.insert(entity, Dirty);
            false
        }
        GestureEvent::Tap { .. } | GestureEvent::DragEnd { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = false;
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
}

pub(crate) fn checkbox_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        if let Some(cb) = world.get_mut::<Checkbox>(entity) {
            cb.toggle();
        }
        world.insert(entity, Dirty);
        return true;
    }
    false
}

/// TabBar tap → snap selected, jump indicator_offset. Apps that want
/// a smooth slide attach an animate! component on the same entity and
/// drive `indicator_offset` from a Tween (same pattern Switch uses for
/// its thumb position).
fn tabbar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world
        .get::<crate::widget::ComputedRect>(entity)
        .map(|c| c.0)
    else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let count = match world.get::<TabBar>(entity) {
        Some(tb) if tb.count > 0 => tb.count,
        _ => return false,
    };
    let local = (x - rect.x).max(Fixed::ZERO);
    let tab_w = rect.w / Fixed::from_int(count as i32);
    let idx = (local / tab_w).to_int().clamp(0, count as i32 - 1) as u8;
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.selected = idx;
        tb.indicator_offset = Fixed::from_int(idx as i32);
    }
    world.insert(entity, Dirty);
    true
}

/// TextInput's gesture handler is a no-op pass-through; focus is set
/// by the focus_on_tap helper in App::run when the Tap target's
/// ancestors carry `Focusable`. We mirror the resulting focus state
/// onto TextInput.focused so the renderer can read it without going
/// through FocusState.
fn textinput_gesture_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        sync_textinput_focus(world);
        world.insert(entity, Dirty);
        return true;
    }
    false
}

/// Walk every TextInput in the world and update `focused` from the
/// shared `FocusState`. Called from the gesture handler (focus changes
/// only on Tap) and after backspace/insert (Dirty already).
fn sync_textinput_focus(world: &mut World) {
    let focused = world
        .resource::<crate::event::focus::FocusState>()
        .and_then(|fs| fs.focused);
    let entities: alloc::vec::Vec<_> = world.query::<TextInput>().collect();
    for e in entities {
        let want = Some(e) == focused;
        let current = world
            .get::<TextInput>(e)
            .map(|t| t.focused)
            .unwrap_or(false);
        if want != current {
            if let Some(ti) = world.get_mut::<TextInput>(e) {
                ti.focused = want;
            }
            world.insert(e, Dirty);
        }
    }
}

fn textinput_key_handler(world: &mut World, entity: Entity, event: &InputEvent) -> bool {
    let Some(ti) = world.get_mut::<TextInput>(entity) else {
        return false;
    };
    let mut changed = false;
    match event {
        InputEvent::CharInput { ch } => {
            if (*ch as u32) < 128 {
                changed |= ti.insert(*ch as u8);
            }
        }
        InputEvent::Key { code, pressed } if *pressed => match *code {
            KEY_BACKSPACE => changed |= ti.backspace(),
            KEY_DELETE => changed |= ti.delete_forward(),
            KEY_LEFT => {
                ti.move_left();
                changed = true;
            }
            KEY_RIGHT => {
                ti.move_right();
                changed = true;
            }
            KEY_HOME => {
                ti.move_home();
                changed = true;
            }
            KEY_END => {
                ti.move_end();
                changed = true;
            }
            _ => return false,
        },
        _ => return false,
    }
    if changed {
        world.insert(entity, Dirty);
    }
    true
}

/// ProgressBar reads the last rendered rect (PrevRect) so it can map
/// the pointer x to a 0..1 ratio without a layout re-walk.
fn progress_bar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world
        .get::<crate::widget::ComputedRect>(entity)
        .map(|c| c.0)
    else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let ratio = ((x - rect.x).to_f32() / rect.w.to_f32()).clamp(0.0, 1.0);
    if let Some(pb) = world.get_mut::<ProgressBar>(entity) {
        pb.value = ratio;
    }
    world.insert(entity, Dirty);
    true
}

// Snapshot ViewAttach fn pointers so the borrow on ViewRegistry
// drops before each fn gets &mut World. The mut-borrow conflict
// is the reason for the two-step copy instead of streaming.
fn run_registry_attach(world: &mut World, entity: Entity) {
    let mut pending: alloc::vec::Vec<crate::widget::view::ViewAttach> = alloc::vec::Vec::new();
    if let Some(reg) = world.resource::<crate::widget::view::ViewRegistry>() {
        for v in reg.iter() {
            if let Some(f) = v.auto_attach {
                pending.push(f);
            }
        }
    }
    for f in pending {
        f(world, entity);
    }
}

/// Pick the right input handler for a single entity. Idempotent:
/// returns early if the entity already has a `GestureHandler` so
/// user-supplied overrides win. Each branch checks one component
/// type and installs the corresponding handler(s).
fn attach_handlers_for(world: &mut World, entity: Entity) {
    // Registry-driven attach runs first; per-view fns guard against
    // double-install when GestureHandler is already on the entity.
    run_registry_attach(world, entity);

    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    if world.get::<ProgressBar>(entity).is_some() {
        world.insert(
            entity,
            GestureHandler {
                on_gesture: progress_bar_handler,
            },
        );
        return;
    }
    if world.get::<TabBar>(entity).is_some() {
        world.insert(
            entity,
            GestureHandler {
                on_gesture: tabbar_handler,
            },
        );
        return;
    }
    if world.get::<TextInput>(entity).is_some() {
        world.insert(
            entity,
            GestureHandler {
                on_gesture: textinput_gesture_handler,
            },
        );
        world.insert(entity, Focusable);
        world.insert(
            entity,
            KeyHandler {
                on_key: textinput_key_handler,
            },
        );
    }
}

/// Walk the widget tree from `root` and auto-install gesture/key
/// handlers on built-in widgets that don't already have one. Call once
/// after building the tree.
pub fn attach_widget_input_handlers(world: &mut World, root: Entity) {
    let mut stack = alloc::vec::Vec::with_capacity(16);
    stack.push(root);
    while let Some(entity) = stack.pop() {
        if let Some(children) = world.get::<crate::widget::Children>(entity) {
            for &child in &children.0 {
                stack.push(child);
            }
        }
        attach_handlers_for(world, entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Rect;
    use crate::widget::ComputedRect;

    #[test]
    fn tabbar_tap_picks_correct_tab() {
        let mut world = World::default();
        let e = world.spawn();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        // Tab width = 100. Tap at x=50 → tab 0; x=150 → tab 1; x=270 → tab 2.
        for (x, expected) in [(50, 0u8), (150, 1), (270, 2), (0, 0), (299, 2)] {
            tabbar_handler(
                &mut world,
                e,
                &GestureEvent::Tap {
                    x: Fixed::from_int(x),
                    y: Fixed::from_int(20),
                    target: e,
                },
            );
            let tb = world.get::<TabBar>(e).unwrap();
            assert_eq!(tb.selected, expected, "x={x} → expected {expected}");
            assert_eq!(tb.indicator_offset, Fixed::from_int(expected as i32));
        }
    }

    #[test]
    fn registry_attach_installs_button_gesture_handler() {
        use crate::types::Color;
        use crate::widget::view::ViewRegistry;

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.register(crate::components::button::view());
        reg.sort_by_priority();
        world.insert_resource(reg);

        let e = world.spawn();
        world.insert(e, Button::new(Color::rgb(0, 0, 0), Color::rgb(0, 0, 0)));

        attach_handlers_for(&mut world, e);

        assert!(
            world.get::<GestureHandler>(e).is_some(),
            "registry-driven attach must install a GestureHandler on Button entities"
        );
    }

    #[test]
    fn registry_attach_skips_when_handler_already_present() {
        use crate::types::Color;
        use crate::widget::view::ViewRegistry;

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.register(crate::components::button::view());
        reg.sort_by_priority();
        world.insert_resource(reg);

        let e = world.spawn();
        world.insert(e, Button::new(Color::rgb(0, 0, 0), Color::rgb(0, 0, 0)));
        fn user_handler(_: &mut World, _: Entity, _: &GestureEvent) -> bool {
            false
        }
        world.insert(
            e,
            GestureHandler {
                on_gesture: user_handler,
            },
        );

        attach_handlers_for(&mut world, e);

        let h = world.get::<GestureHandler>(e).expect("handler present");
        let installed: *const () = h.on_gesture as *const ();
        let expected: *const () = user_handler as *const ();
        assert!(
            core::ptr::eq(installed, expected),
            "user-supplied handler must not be overwritten"
        );
    }

    #[test]
    fn registry_attach_installs_checkbox_gesture_handler() {
        use crate::types::Color;
        use crate::widget::view::ViewRegistry;

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.register(crate::components::checkbox::view());
        reg.sort_by_priority();
        world.insert_resource(reg);

        let e = world.spawn();
        world.insert(e, Checkbox::new(Color::rgb(0, 0, 0), Color::rgb(0, 0, 0)));

        attach_handlers_for(&mut world, e);

        assert!(
            world.get::<GestureHandler>(e).is_some(),
            "registry-driven attach must install a GestureHandler on Checkbox entities"
        );
    }

    #[test]
    fn checkbox_tap_toggles_checked() {
        use crate::types::Color;
        use crate::widget::view::ViewRegistry;

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.register(crate::components::checkbox::view());
        reg.sort_by_priority();
        world.insert_resource(reg);

        let e = world.spawn();
        world.insert(e, Checkbox::new(Color::rgb(0, 0, 0), Color::rgb(0, 0, 0)));
        attach_handlers_for(&mut world, e);

        let consumed = checkbox_handler(
            &mut world,
            e,
            &GestureEvent::Tap {
                x: Fixed::ZERO,
                y: Fixed::ZERO,
                target: e,
            },
        );
        assert!(consumed);
        assert!(world.get::<Checkbox>(e).unwrap().checked);
    }

    #[test]
    fn tabbar_ignores_non_tap() {
        let mut world = World::default();
        let e = world.spawn();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        let consumed = tabbar_handler(
            &mut world,
            e,
            &GestureEvent::DragMove {
                x: Fixed::from_int(50),
                y: Fixed::from_int(20),
                dx: Fixed::ZERO,
                dy: Fixed::ZERO,
                target: e,
            },
        );
        assert!(!consumed);
        assert_eq!(world.get::<TabBar>(e).unwrap().selected, 0);
    }
}
