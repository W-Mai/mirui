use crate::ecs::{Entity, World};

// Backwards-compat re-export; canonical path is components::text_input.
pub use crate::components::text_input::{CursorBlinkPhase, cursor_blink_system};

// Snapshot ViewAttach fn pointers so the borrow on ViewRegistry
// drops before each fn gets &mut World. The mut-borrow conflict
// is the reason for the two-step copy instead of streaming.
pub fn attach_handlers_for(world: &mut World, entity: Entity) {
    let mut pending: alloc::vec::Vec<crate::widget::view::ViewAttach> = alloc::vec::Vec::new();
    if let Some(reg) = world.resource::<crate::widget::view::ViewRegistry>() {
        for v in reg.iter() {
            if let Some(f) = v.auto_attach() {
                pending.push(f);
            }
        }
    }
    for f in pending {
        f(world, entity);
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
    use crate::components::button::Button;
    use crate::components::checkbox::{Checkbox, checkbox_handler};
    use crate::components::tabbar::{TabBar, tabbar_handler};
    use crate::components::text_input::TextInput;
    use crate::event::GestureHandler;
    use crate::event::gesture::GestureEvent;
    use crate::types::{Fixed, Rect};
    use crate::widget::ComputedRect;
    use crate::widget::view::ViewRegistry;

    #[test]
    fn tabbar_tap_picks_correct_tab() {
        let mut world = World::default();
        let e = world.spawn_empty();
        world.insert(e, ComputedRect(Rect::new(0, 0, 300, 40)));
        world.insert(e, TabBar::new(3));
        for (x, expected) in [(150, 1u8), (270, 2), (0, 0), (299, 2)] {
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
        }
    }

    #[test]
    fn registry_carries_button_internal_gesture() {
        let view = crate::components::button::view();
        assert!(
            view.internal_gesture().is_some(),
            "Button view must expose an internal gesture handler"
        );
    }

    #[test]
    fn user_gesture_handler_coexists_with_button_internal() {
        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::components::button::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, Button::new());
        fn user_handler(_: &mut World, _: Entity, _: &GestureEvent) -> bool {
            false
        }
        world.insert(e, GestureHandler::from_fn(user_handler));

        attach_handlers_for(&mut world, e);

        let h = world.get::<GestureHandler>(e).expect("user handler stays");
        let installed = match h.on_gesture {
            crate::event::GestureCallback::Fn(f) => f as *const (),
            crate::event::GestureCallback::Closure(_) => panic!("expected the user fn handler"),
        };
        assert!(
            core::ptr::eq(installed, user_handler as *const ()),
            "user-supplied GestureHandler stays on the user channel; \
             button internals run on the View internal channel"
        );
    }

    #[test]
    fn text_input_attach_installs_focus_and_key_handler() {
        use crate::event::focus::{Focusable, KeyHandler};

        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::components::text_input::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, TextInput::new());

        attach_handlers_for(&mut world, e);

        assert!(world.get::<Focusable>(e).is_some());
        assert!(world.get::<KeyHandler>(e).is_some());
        assert!(
            crate::components::text_input::view()
                .internal_gesture()
                .is_some(),
            "TextInput view must expose internal gesture for focus + tap-to-focus"
        );
    }

    #[test]
    fn registry_carries_progress_bar_internal_gesture() {
        let view = crate::components::progress_bar::view();
        assert!(
            view.internal_gesture().is_some(),
            "ProgressBar view must expose an internal gesture handler"
        );
    }

    #[test]
    fn registry_carries_checkbox_internal_gesture() {
        let view = crate::components::checkbox::view();
        assert!(
            view.internal_gesture().is_some(),
            "Checkbox view must expose an internal gesture handler"
        );
    }

    #[test]
    fn checkbox_tap_toggles_checked() {
        let mut world = World::default();
        let mut reg = ViewRegistry::default();
        reg.insert(crate::components::checkbox::view());
        world.insert_resource(reg);

        let e = world.spawn_empty();
        world.insert(e, Checkbox::new());
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
        let e = world.spawn_empty();
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
